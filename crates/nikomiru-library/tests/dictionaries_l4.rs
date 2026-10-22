// SPDX-License-Identifier: GPL-3.0-only

//! L4: the dictionary registry and its folders (DIC-013, DIC-016, DIC-021,
//! DIC-031…DIC-033, DIC-035) with the mini dictionaries.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use nikomiru_core::{Cancel, ManualClock};
use nikomiru_dict::lookup::Options;
use nikomiru_dict::store::Counts;
use nikomiru_dict::yomitan::Kinds;
use nikomiru_library::{
    Check, Collapse, DictionaryError, DictionaryManager, DictionaryRecord, Library,
};

fn source(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/dicts")
        .join(name)
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Zips a fixture folder, replacing the files named in `overrides`.
fn zipped(dir: &Path, name: &str, tag: &str, overrides: &[(&str, String)]) -> PathBuf {
    let src = source(name);
    let mut all = Vec::new();
    files(&src, &mut all);
    all.sort();
    let path = dir.join(format!("{name}-{tag}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for f in all {
        let inner = f
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = match overrides.iter().find(|(n, _)| *n == inner) {
            Some((_, text)) => text.clone().into_bytes(),
            None => std::fs::read(&f).unwrap(),
        };
        zip.start_file(inner, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn index(title: &str, revision: &str) -> (&'static str, String) {
    (
        "index.json",
        format!(
            r#"{{"title":"{title}","revision":"{revision}","format":3,"sourceLanguage":"ja"}}"#
        ),
    )
}

struct Env {
    tmp: tempfile::TempDir,
    manager: DictionaryManager,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let manager = Self::open(tmp.path());
        Self { tmp, manager }
    }

    fn open(root: &Path) -> DictionaryManager {
        let library = Arc::new(Library::open(&root.join("library.redb")).unwrap());
        let clock = Arc::new(ManualClock::new(
            UNIX_EPOCH + Duration::from_secs(1_800_000_000),
        ));
        DictionaryManager::open(library, root.join("dicts"), clock, "ja").unwrap()
    }

    fn add(&self, name: &str, overrides: &[(&str, String)]) -> String {
        let zip = zipped(
            self.tmp.path(),
            name,
            &overrides.len().to_string(),
            overrides,
        );
        self.manager
            .import(&zip, None, &Cancel::new(), |_| {})
            .unwrap()
    }

    fn folders(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.tmp.path().join("dicts"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn glossaries(&self, text: &str) -> Vec<String> {
        let lookup = self.manager.lookup().unwrap();
        let entries = lookup.lookup(text, Options::default()).unwrap();
        entries
            .first()
            .map(|e| {
                e.glossaries
                    .iter()
                    .map(|g| {
                        // as compact JSON: the banks keep their own layout
                        let v: serde_json::Value =
                            serde_json::from_str(&g.term.glossary().unwrap()).unwrap();
                        v.to_string()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[test]
fn an_import_is_registered_with_its_details() {
    let env = Env::new();
    let id = env.add("mini-ja", &[]);
    let list = env.manager.list().unwrap();
    assert_eq!(list.len(), 1);
    let (got, rec) = &list[0];
    assert_eq!(got, &id);
    assert_eq!(
        (rec.title.as_str(), rec.revision.as_str()),
        ("Nikomiru mini-ja", "1")
    );
    assert!(rec.enabled && rec.kinds.terms);
    assert_eq!(rec.counts.terms, 76);
    assert_eq!(rec.imported_at, 1_800_000_000_000);
    assert!(rec.size > 0);
    assert_eq!(rec.language.as_deref(), Some("ja"));
    assert_eq!(env.glossaries("食べる"), [r#"["to eat"]"#]);
}

// T-DIC-016
#[test]
fn the_same_revision_is_refused_and_a_new_one_replaces_the_old() {
    let env = Env::new();
    let old = env.add("mini-norules", &[]);
    env.add("mini-freq", &[]);
    env.manager
        .rename(&old, Some("My dictionary".into()))
        .unwrap();
    env.manager.set_collapse(&old, Collapse::Collapsed).unwrap();

    let same = zipped(env.tmp.path(), "mini-norules", "same", &[]);
    assert_eq!(
        env.manager.check(&same).unwrap(),
        Check::SameRevision { id: old.clone() }
    );
    let err = env
        .manager
        .import(&same, None, &Cancel::new(), |_| {})
        .unwrap_err();
    assert!(matches!(err, DictionaryError::SameRevision { .. }), "{err}");

    let newer = zipped(
        env.tmp.path(),
        "mini-norules",
        "r2",
        &[index("Nikomiru mini-norules", "2")],
    );
    assert_eq!(
        env.manager.check(&newer).unwrap(),
        Check::Update {
            id: old.clone(),
            installed: "1".into()
        }
    );
    let new = env
        .manager
        .import(&newer, Some(&old), &Cancel::new(), |_| {})
        .unwrap();
    let list = env.manager.list().unwrap();
    assert_eq!(list.len(), 2);
    let (first, rec) = &list[0];
    assert_eq!(first, &new, "the update keeps the place of the old one");
    assert_eq!(rec.revision, "2");
    assert_eq!(
        (rec.name(), rec.collapse),
        ("My dictionary", Collapse::Collapsed),
        "settings kept"
    );
    assert!(!env.folders().contains(&old), "the old folder is gone");
    assert!(env.folders().contains(&new));
}

// T-DIC-021
#[test]
fn deleting_closes_removes_and_forgets() {
    let env = Env::new();
    let id = env.add("mini-ja", &[]);
    assert!(!env.glossaries("猫").is_empty());
    env.manager.delete(&id).unwrap();
    assert!(env.manager.list().unwrap().is_empty());
    assert!(env.folders().is_empty());
    assert!(
        env.glossaries("猫").is_empty(),
        "the cached lookup is dropped"
    );
    assert!(matches!(
        env.manager.delete(&id),
        Err(DictionaryError::Unknown(_))
    ));
}

// T-DIC-032
#[test]
fn a_disabled_dictionary_is_not_queried() {
    let env = Env::new();
    let ja = env.add("mini-ja", &[]);
    env.add("mini-norules", &[]);
    assert_eq!(env.glossaries("食べる").len(), 2);
    env.manager.set_enabled(&ja, false).unwrap();
    let lookup = env.manager.lookup().unwrap();
    assert_eq!(
        lookup.dictionaries().len(),
        1,
        "only enabled dictionaries are opened for lookups"
    );
    assert_eq!(
        env.glossaries("食べる"),
        [r#"["to eat (dictionary without rules)"]"#]
    );
    assert!(env.glossaries("猫").is_empty());
}

// T-DIC-031
#[test]
fn the_order_decides_the_glossaries() {
    let env = Env::new();
    let ja = env.add("mini-ja", &[]);
    let norules = env.add("mini-norules", &[]);
    assert_eq!(env.glossaries("食べる")[0], r#"["to eat"]"#);
    env.manager.reorder(&[norules.clone(), ja.clone()]).unwrap();
    assert_eq!(
        env.glossaries("食べる")[0],
        r#"["to eat (dictionary without rules)"]"#
    );
    let ids: Vec<String> = env
        .manager
        .list()
        .unwrap()
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ids, [norules, ja]);
}

// T-DIC-035
#[test]
fn the_users_css_is_kept_carried_over_by_an_update_and_drops_the_cache() {
    let env = Env::new();
    let old = env.add("mini-norules", &[]);
    let css = "span[data-sc-class=\"note\"] { color: red }";
    let before = env.manager.lookup().unwrap();
    env.manager.set_user_css(&old, css).unwrap();
    assert!(
        !Arc::ptr_eq(&before, &env.manager.lookup().unwrap()),
        "the cached lookup is dropped, as by any other change"
    );
    assert_eq!(env.manager.list().unwrap()[0].1.user_css, css);
    assert!(matches!(
        env.manager.set_user_css("gone", css),
        Err(DictionaryError::Unknown(_))
    ));

    let newer = zipped(
        env.tmp.path(),
        "mini-norules",
        "r2",
        &[index("Nikomiru mini-norules", "2")],
    );
    let new = env
        .manager
        .import(&newer, Some(&old), &Cancel::new(), |_| {})
        .unwrap();
    let (id, rec) = &env.manager.list().unwrap()[0];
    assert_eq!(
        (id, rec.user_css.as_str()),
        (&new, css),
        "the update keeps it"
    );

    // an empty one clears it, and it is in the registry, not in memory
    env.manager.set_user_css(&new, "").unwrap();
    assert_eq!(env.manager.list().unwrap()[0].1.user_css, "");
    env.manager
        .set_user_css(&new, "div { margin: 1em }")
        .unwrap();
    drop(env.manager);
    let manager = Env::open(env.tmp.path());
    assert_eq!(manager.list().unwrap()[0].1.user_css, "div { margin: 1em }");
}

// T-DIC-013
#[test]
fn opening_removes_what_a_crash_left() {
    let env = Env::new();
    let id = env.add("mini-ja", &[]);
    let dicts = env.tmp.path().join("dicts");
    // a staging folder, a folder renamed but never registered, and a record
    // whose folder is gone
    std::fs::create_dir_all(dicts.join("dead.importing")).unwrap();
    std::fs::create_dir_all(dicts.join("orphan")).unwrap();
    let ghost = env.add("mini-norules", &[]);
    std::fs::remove_dir_all(dicts.join(&ghost)).unwrap();
    drop(env.manager);
    let manager = Env::open(env.tmp.path());
    let ids: Vec<String> = manager
        .list()
        .unwrap()
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ids, std::slice::from_ref(&id));
    let folders: Vec<String> = std::fs::read_dir(&dicts)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(folders, [id]);
}

fn record(title: &str) -> DictionaryRecord {
    DictionaryRecord {
        title: title.into(),
        revision: "1".into(),
        kinds: Kinds::default(),
        order: 0,
        enabled: true,
        language: None,
        display_name: None,
        collapse: Collapse::Automatic,
        user_css: String::new(),
        counts: Counts::default(),
        imported_at: 0,
        size: 0,
    }
}

// T-DIC-022
#[test]
fn registry_edits_from_several_threads_are_never_lost() {
    const THREADS: u64 = 8;
    const EDITS: u64 = 25;
    let tmp = tempfile::tempdir().unwrap();
    let library = Arc::new(Library::open(&tmp.path().join("library.redb")).unwrap());
    library
        .edit_dictionaries(|list| {
            for name in ["counter", "deleted"] {
                list.push((name.into(), record(name)));
            }
            for t in 0..THREADS {
                list.push((format!("own-{t}"), record("own")));
            }
        })
        .unwrap();
    let threads: Vec<_> = (0..THREADS)
        .map(|t| {
            let library = Arc::clone(&library);
            std::thread::spawn(move || {
                for n in 0..EDITS {
                    library
                        .edit_dictionaries(|list| {
                            for (id, r) in list.iter_mut() {
                                if id == "counter" {
                                    r.size += 1;
                                } else if *id == format!("own-{t}") {
                                    r.display_name = Some(format!("edit {n}"));
                                }
                            }
                        })
                        .unwrap();
                }
            })
        })
        .collect();
    // the page deletes one while the others are being edited
    while library.dictionaries().unwrap()[0].1.size < THREADS * EDITS / 4
        && !threads.iter().all(std::thread::JoinHandle::is_finished)
    {
        std::thread::yield_now();
    }
    library
        .edit_dictionaries(|list| list.retain(|(id, _)| id != "deleted"))
        .unwrap();
    for t in threads {
        t.join().unwrap();
    }
    let all = library.dictionaries().unwrap();
    assert!(
        all.iter().all(|(id, _)| id != "deleted"),
        "a deleted dictionary came back"
    );
    let counter = all.iter().find(|(id, _)| id == "counter").unwrap();
    assert_eq!(counter.1.size, THREADS * EDITS, "increments were lost");
    let last = format!("edit {}", EDITS - 1);
    for t in 0..THREADS {
        let own = all
            .iter()
            .find(|(id, _)| *id == format!("own-{t}"))
            .unwrap();
        assert_eq!(own.1.display_name.as_deref(), Some(last.as_str()));
    }
    let orders: Vec<u32> = all.iter().map(|(_, r)| r.order).collect();
    assert_eq!(orders, (0..all.len() as u32).collect::<Vec<_>>());
}

// T-DIC-016
#[test]
fn the_same_dictionary_imported_twice_at_once_is_registered_once() {
    let env = Env::new();
    let first = zipped(env.tmp.path(), "mini-ja", "first", &[]);
    let second = zipped(env.tmp.path(), "mini-ja", "second", &[]);
    let mut nested = None;
    // the second import starts and ends while the first one is being written
    let err = env
        .manager
        .import(&first, None, &Cancel::new(), |p| {
            if p.files_done == 1 && nested.is_none() {
                nested = Some(env.manager.import(&second, None, &Cancel::new(), |_| {}));
            }
        })
        .unwrap_err();
    let kept = nested.unwrap().unwrap();
    assert!(matches!(err, DictionaryError::SameRevision { .. }), "{err}");
    let ids: Vec<String> = env
        .manager
        .list()
        .unwrap()
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ids, std::slice::from_ref(&kept));
    assert_eq!(env.folders(), [kept], "the refused copy is removed");
}

// T-DIC-016
#[test]
fn an_update_whose_old_dictionary_was_deleted_meanwhile_is_added() {
    let env = Env::new();
    let old = env.add("mini-norules", &[]);
    let newer = zipped(
        env.tmp.path(),
        "mini-norules",
        "r2",
        &[index("Nikomiru mini-norules", "2")],
    );
    let mut deleted = false;
    let new = env
        .manager
        .import(&newer, Some(&old), &Cancel::new(), |p| {
            if p.files_done == 1 && !deleted {
                env.manager.delete(&old).unwrap();
                deleted = true;
            }
        })
        .unwrap();
    let list = env.manager.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((&list[0].0, list[0].1.revision.as_str()), (&new, "2"));
    assert_eq!(env.folders(), [new]);
}

// T-DIC-016
#[test]
fn an_update_holds_even_if_the_old_folder_cannot_be_removed() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let old = env.add("mini-norules", &[]);
    assert_eq!(env.manager.lookup().unwrap().dictionaries()[0].id, old);
    // a folder inside the old one whose entries cannot be removed
    let locked = env.tmp.path().join("dicts").join(&old).join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("file"), b"x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let privileged = std::fs::write(locked.join("probe"), b"x").is_ok();

    let newer = zipped(
        env.tmp.path(),
        "mini-norules",
        "r2",
        &[index("Nikomiru mini-norules", "2")],
    );
    let result = env
        .manager
        .import(&newer, Some(&old), &Cancel::new(), |_| {});
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let new = result.expect("the update is done even if the old folder stays");
    let ids: Vec<String> = env
        .manager
        .list()
        .unwrap()
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ids, std::slice::from_ref(&new));
    assert_eq!(
        env.manager.lookup().unwrap().dictionaries()[0].id,
        new,
        "the cached lookup is dropped"
    );
    if privileged {
        eprintln!("running with privileges: the old folder could be removed");
    } else {
        assert!(env.folders().contains(&old), "the old folder stayed");
    }
    // the next start removes the folder the registry no longer knows
    drop(env.manager);
    let manager = Env::open(env.tmp.path());
    assert_eq!(manager.list().unwrap().len(), 1);
    let folders: Vec<String> = std::fs::read_dir(env.tmp.path().join("dicts"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(folders, [new]);
}
