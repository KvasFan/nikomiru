// SPDX-License-Identifier: GPL-3.0-only

//! L4: importing Yomitan zips into redb (DIC-001…DIC-015). The mini
//! dictionaries are zipped here from `tests/fixtures/dicts/`; the hostile zips
//! come from `cargo xtask fixtures` (TEST §4.3).

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::zipped;

use nikomiru_core::Cancel;
use nikomiru_dict::import::{ImportError, Imported, Progress, Unsafe, cleanup, import, read_index};
use nikomiru_dict::store::{DictStore, Field, RedbStore};
use nikomiru_dict::yomitan::{FrequencyMode, Meta};

fn import_ok(zip: &Path, dicts: &Path) -> RedbStore {
    let imported = import(zip, dicts, &Cancel::new(), |_| {}).unwrap();
    assert!(imported.dir.ends_with(&imported.id));
    RedbStore::open(&imported.dir.join("dict.redb")).unwrap()
}

fn leftovers(dicts: &Path) -> Vec<String> {
    std::fs::read_dir(dicts)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

// T-DIC-001
// T-DIC-002
#[test]
fn mini_ja_keeps_terms_tags_media_and_styles() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = zipped(tmp.path(), "mini-ja", &[]);
    let dicts = tmp.path().join("dicts");
    assert_eq!(read_index(&zip).unwrap().title, "Nikomiru mini-ja");
    let mut seen = Vec::new();
    let imported = import(&zip, &dicts, &Cancel::new(), |p| seen.push(p)).unwrap();
    let info = &imported.info;
    assert_eq!(info.counts.terms, 76);
    assert_eq!(info.counts.tags, 27);
    assert_eq!(info.counts.media, 1);
    assert!(info.kinds.terms && !info.kinds.kanji);
    assert!(info.styles.contains(".gloss-sc-table"));
    assert_eq!(info.index.source_language.as_deref(), Some("ja"));
    // progress reaches every file
    let last = seen.last().unwrap();
    assert_eq!(last.files_done, last.files_total);
    assert_eq!(last.entries, 76 + 27);

    let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
    let taberu = &store.terms(Field::Expression, "食べる").unwrap()[0];
    assert_eq!(
        (taberu.rules.as_str(), taberu.term_tags.as_str()),
        ("v1", "P")
    );
    assert!(taberu.sequence > 1000, "sequence kept");
    let v5k = store.tag("v5k").unwrap().unwrap();
    assert_eq!(v5k.category, "partOfSpeech");
    assert!(v5k.notes.contains("Godan"));
    let png = store.media("img/a.png").unwrap().unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let kanji = store.terms(Field::Expression, "漢字").unwrap();
    assert!(kanji[0].glossary().unwrap().contains("structured-content"));
    // two spellings share a reading
    assert_eq!(
        store.terms(Field::Reading, "とりあつかい").unwrap().len(),
        2
    );
}

// T-DIC-003
#[test]
fn occurrence_frequencies_and_string_values() {
    let tmp = tempfile::tempdir().unwrap();
    let store = import_ok(
        &zipped(tmp.path(), "mini-freq-occ", &[]),
        &tmp.path().join("dicts"),
    );
    assert_eq!(
        store.info().index.frequency_mode,
        FrequencyMode::OccurrenceBased
    );
    assert!(store.info().kinds.frequency);
    let Meta::Frequency(f) = &store.term_meta("見る").unwrap()[0] else {
        panic!("frequency expected")
    };
    assert_eq!(
        (f.value, f.display.as_deref()),
        (45_000, Some("45000 (very common)"))
    );
}

#[test]
fn pitch_kanji_and_norules_dictionaries() {
    let tmp = tempfile::tempdir().unwrap();
    let dicts = tmp.path().join("dicts");
    let pitch = import_ok(&zipped(tmp.path(), "mini-pitch", &[]), &dicts);
    assert!(pitch.info().kinds.pitch);
    let rows = pitch.term_meta("日本").unwrap();
    assert_eq!(rows.len(), 2, "one pitch row and one IPA row");
    let Meta::Pitch { pitches, .. } = &rows[0] else {
        panic!("pitch first")
    };
    assert_eq!(
        pitches.len(),
        2,
        "equal pitches are kept as written; the popup collapses them"
    );
    assert!(matches!(rows[1], Meta::Ipa { .. }));
    let kanji = import_ok(&zipped(tmp.path(), "mini-kanji", &[]), &dicts);
    assert!(kanji.info().kinds.kanji);
    assert_eq!(
        kanji.kanji("食").unwrap().unwrap().meanings,
        ["eat", "food"]
    );
    assert_eq!(kanji.kanji_meta("日").unwrap().len(), 1);
    let norules = import_ok(&zipped(tmp.path(), "mini-norules", &[]), &dicts);
    assert_eq!(
        norules.terms(Field::Expression, "食べる").unwrap()[0].rules,
        ""
    );
}

// T-DIC-010
#[test]
fn a_cancelled_import_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = zipped(tmp.path(), "mini-ja", &[]);
    let dicts = tmp.path().join("dicts");
    let cancel = Cancel::new();
    let err = import(&zip, &dicts, &cancel, |p| {
        if p.files_done == 1 {
            cancel.cancel();
        }
    })
    .unwrap_err();
    assert!(matches!(err, ImportError::Cancelled), "{err}");
    assert!(leftovers(&dicts).is_empty(), "{:?}", leftovers(&dicts));
}

// T-DIC-012
#[test]
fn a_glossary_over_10_mib_fails_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let huge = "x".repeat(10 << 20);
    let bank = format!(r#"[["巨大","きょだい","n","",0,["{huge}"],1,""]]"#);
    let zip = zipped(
        tmp.path(),
        "mini-norules",
        &[("term_bank_2.json", bank.into_bytes())],
    );
    let dicts = tmp.path().join("dicts");
    let err = import(&zip, &dicts, &Cancel::new(), |_| {}).unwrap_err();
    assert!(matches!(err, ImportError::GlossaryTooLarge { .. }), "{err}");
    assert!(leftovers(&dicts).is_empty());
}

/// A zip with `n` terms whose glossaries do not compress well, in files of
/// `per_bank` terms: big enough to fill a small filesystem, slow enough to
/// kill.
fn synthetic(dir: &Path, n: usize, per_bank: usize) -> PathBuf {
    let path = dir.join("synthetic.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("index.json", options).unwrap();
    zip.write_all(br#"{"title":"Synthetic","revision":"1","format":3}"#)
        .unwrap();
    let mut state = 0x2545_f491_u64;
    for bank in 0..n.div_ceil(per_bank) {
        let terms: Vec<String> = (bank * per_bank..n.min((bank + 1) * per_bank))
            .map(|i| {
                let gloss: String = (0..200)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        char::from(b'a' + (state % 26) as u8)
                    })
                    .collect();
                format!(r#"["語{i}","ご{i}","n","",0,["{gloss}"],{i},""]"#)
            })
            .collect();
        zip.start_file(format!("term_bank_{}.json", bank + 1), options)
            .unwrap();
        zip.write_all(format!("[{}]", terms.join(",")).as_bytes())
            .unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Imports `zip`, cancelling it `after` the first progress that `when`
/// accepts; returns the result and how long the import went on after the
/// cancel.
fn cancelled(
    zip: &Path,
    dicts: &Path,
    when: fn(Progress) -> bool,
    after: Duration,
) -> (Result<Imported, ImportError>, Duration) {
    let cancel = Cancel::new();
    let fired = Arc::new(Mutex::new(None));
    let mut timer = None;
    let result = import(zip, dicts, &cancel, |p| {
        if timer.is_none() && when(p) {
            let (cancel, fired) = (cancel.clone(), Arc::clone(&fired));
            timer = Some(std::thread::spawn(move || {
                std::thread::sleep(after);
                *fired.lock().unwrap() = Some(Instant::now());
                cancel.cancel();
            }));
        }
    });
    let ended = Instant::now();
    timer.unwrap().join().unwrap();
    let fired = fired.lock().unwrap().unwrap();
    (result, ended.saturating_duration_since(fired))
}

// T-DIC-010
#[test]
fn a_cancel_inside_one_large_bank_or_the_final_step_stops_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    // some dictionaries come as one large bank
    let zip = synthetic(tmp.path(), 60_000, 60_000);
    let dicts = tmp.path().join("dicts");
    let check = |step: &str, when: fn(Progress) -> bool, after: u64| {
        let (result, waited) = cancelled(&zip, &dicts, when, Duration::from_millis(after));
        eprintln!("T-DIC-010 cancel {step}: the import stopped {waited:?} later");
        assert!(
            matches!(result, Err(ImportError::Cancelled)),
            "{step}: {result:?}"
        );
        assert!(
            leftovers(&dicts).is_empty(),
            "{step}: {:?}",
            leftovers(&dicts)
        );
        assert!(waited < Duration::from_secs(1), "{step}: {waited:?}");
    };
    check("inside the bank", |p| p.files_done == 0, 150);
    check("in the final step", |p| p.files_done == p.files_total, 50);
}

const CHILD_ZIP: &str = "NIKOMIRU_TEST_IMPORT_ZIP";
const CHILD_DICTS: &str = "NIKOMIRU_TEST_IMPORT_DICTS";
const CHILD_MODE: &str = "NIKOMIRU_TEST_IMPORT_MODE";
const HALFWAY: &str = "nikomiru-import-halfway";

/// Runs only as the child of the tests below: imports `CHILD_ZIP` into
/// `CHILD_DICTS`, either stalling halfway (`stall`, to be killed), expecting
/// a full disk (`full`) or any failed write (`limit`).
#[test]
#[ignore = "child process of the L4 crash, disk-full and write-limit tests"]
fn child_import() {
    let (Ok(zip), Ok(dicts), Ok(mode)) = (
        std::env::var(CHILD_ZIP),
        std::env::var(CHILD_DICTS),
        std::env::var(CHILD_MODE),
    ) else {
        return;
    };
    let dicts = Path::new(&dicts);
    let result = import(Path::new(&zip), dicts, &Cancel::new(), |p| {
        if mode == "stall" && p.files_done == p.files_total / 2 && p.files_done > 0 {
            println!("{HALFWAY}");
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
    match mode.as_str() {
        "full" => {
            let err = result.unwrap_err();
            assert!(matches!(err, ImportError::StorageFull), "{err}");
        }
        "limit" => assert!(result.is_err(), "the import wrote past the limit"),
        _ => panic!("the stalled import must be killed"),
    }
    assert!(leftovers(dicts).is_empty(), "{:?}", leftovers(dicts));
}

fn child(mode: &str, zip: &Path, dicts: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "child_import",
        "--exact",
        "--include-ignored",
        "--nocapture",
    ])
    .env(CHILD_ZIP, zip)
    .env(CHILD_DICTS, dicts)
    .env(CHILD_MODE, mode);
    cmd
}

// T-DIC-013
#[test]
#[ignore = "L4: kills a child process"]
fn an_import_killed_halfway_is_cleaned_up_on_restart() {
    use std::io::BufRead;
    let tmp = tempfile::tempdir().unwrap();
    let zip = synthetic(tmp.path(), 8000, 1000);
    let dicts = tmp.path().join("dicts");
    std::fs::create_dir_all(dicts.join("installed")).unwrap();
    let mut proc = child("stall", &zip, &dicts)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = std::io::BufReader::new(proc.stdout.take().unwrap());
    let reached = stdout.lines().map_while(Result::ok).any(|l| l == HALFWAY);
    proc.kill().unwrap(); // SIGKILL
    proc.wait().unwrap();
    assert!(reached, "the child never got halfway");
    let staging: Vec<_> = leftovers(&dicts)
        .into_iter()
        .filter(|n| n.ends_with(".importing"))
        .collect();
    assert_eq!(
        staging.len(),
        1,
        "the killed import left its staging folder"
    );

    // what the next start does
    assert_eq!(cleanup(&dicts).unwrap(), 1);
    assert_eq!(leftovers(&dicts), ["installed"]);
}

// T-DIC-014
#[test]
#[ignore = "L4: needs unprivileged user namespaces (unshare)"]
fn a_full_disk_fails_with_storage_full_and_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = synthetic(tmp.path(), 20_000, 1000);
    let mount = tmp.path().join("small");
    std::fs::create_dir_all(&mount).unwrap();
    // a 1 MiB tmpfs in a private mount namespace; the import needs ~5 MiB
    let inner = child("full", &zip, &mount.join("dicts"));
    let mut cmd = std::process::Command::new("unshare");
    cmd.args(["--user", "--map-root-user", "--mount", "sh", "-c"])
        .arg(r#"mount -t tmpfs -o size=1m tmpfs "$1" && shift && exec "$@""#)
        .arg("sh")
        .arg(&mount)
        .arg(inner.get_program())
        .args(inner.get_args())
        .envs(inner.get_envs().filter_map(|(k, v)| Some((k, v?))));
    let out = cmd.output().expect("unshare (util-linux) is needed");
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1 passed"),
        "the child test did not run: {stdout}"
    );
}

// T-DIC-014: a write that fails halfway, as on a full disk; ported from
// Fushi's import_failure_boundary_test.cpp, case 6 (ac8034e)
#[test]
fn an_import_that_cannot_write_fails_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = synthetic(tmp.path(), 4000, 1000);
    let dicts = tmp.path().join("dicts");
    // the file size limit stands for the full disk: every write past it fails
    // (EFBIG); SIGXFSZ, which would kill the child first, is ignored
    let inner = child("limit", &zip, &dicts);
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(r#"trap '' XFSZ && ulimit -f 64 && exec "$@""#)
        .arg("sh")
        .arg(inner.get_program())
        .args(inner.get_args())
        .envs(inner.get_envs().filter_map(|(k, v)| Some((k, v?))))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(leftovers(&dicts).is_empty(), "{:?}", leftovers(&dicts));
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/dicts")
        .join(name)
}

// T-DIC-011
// T-SEC-002
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn hostile_zips_are_refused_and_odd_ones_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    let dicts = tmp.path().join("dicts");
    for (name, expected) in [
        ("evil-traversal.zip", Some(Unsafe::Traversal)),
        ("evil-absolute.zip", Some(Unsafe::Absolute)),
        ("evil-symlink.zip", Some(Unsafe::Symlink)),
        ("evil-huge-entry.zip", Some(Unsafe::TooLarge)),
        ("evil-bomb.zip", Some(Unsafe::Bomb)),
        ("evil-badjson.zip", None),
    ] {
        let err = import(&fixture(name), &dicts, &Cancel::new(), |_| {}).unwrap_err();
        match expected {
            Some(reason) => assert!(
                matches!(&err, ImportError::Unsafe { reason: r, .. } if *r == reason),
                "{name}: {err}"
            ),
            None => assert!(matches!(err, ImportError::Format { .. }), "{name}: {err}"),
        }
        assert!(
            leftovers(&dicts).is_empty(),
            "{name} left {:?}",
            leftovers(&dicts)
        );
    }
    assert!(!tmp.path().join("x").exists() && !dicts.join("x").exists());

    let wrapped = import_ok(&fixture("wrapped-dir.zip"), &dicts);
    assert_eq!(
        wrapped.info().counts.terms,
        76,
        "the folder and the macOS files are skipped"
    );
    assert_eq!(wrapped.info().counts.media, 1);
    let unicode = import_ok(&fixture("unicode-path.zip"), &dicts);
    assert!(
        unicode.media("img/字.png").unwrap().is_some(),
        "0x7075 name used"
    );
    let zip64 = import_ok(&fixture("zip64.zip"), &dicts);
    assert_eq!(zip64.info().counts.terms, 76);
}
