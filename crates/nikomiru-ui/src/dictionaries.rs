// SPDX-License-Identifier: GPL-3.0-only

//! The dictionaries page apart from its drawing (DIC-016, DIC-030…DIC-035): the list of
//! registered dictionaries by type and their order, and the import of a zip,
//! with the update offer for a new revision, its progress and its
//! cancellation. The work is done by a [`DictionaryService`] of the
//! application.

use std::path::{Path, PathBuf};

use nikomiru_core::Cancel;
use nikomiru_dict::import::Progress;
use nikomiru_dict::yomitan::Kinds;
use nikomiru_library::{Check, Collapse, DictionaryRecord};
use serde_json::{Value, json};

use crate::popup::Reply;

/// The types the page lists the dictionaries by (DIC-030), in its order.
///
/// A dictionary can have several types (a term dictionary with a frequency
/// bank gives frequencies too), and the order is one for all of them. Its row,
/// with its controls, is under its main type: the first it has, in DIC-015's
/// order, which is how Fushi files a dictionary under one type. Under its
/// other types a reference row marks its place in that ranking, so that each
/// group shows every dictionary its ranking takes in: a frequency dictionary
/// can be dragged above the frequencies of a term dictionary. Full rows under
/// every type would make one drag reorder two groups, and could ask for orders
/// that contradict each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Term dictionaries: their glossaries, in this order, make the popup.
    Terms,
    /// Kanji dictionaries.
    Kanji,
    /// Frequencies: the first ones decide the sorting (LKP-034).
    Frequency,
    /// Pitch accents.
    Pitch,
}

impl Group {
    /// Every type, in the page's order.
    pub const ALL: [Group; 4] = [Group::Terms, Group::Kanji, Group::Frequency, Group::Pitch];

    /// Whether a dictionary with `kinds` has this type.
    pub fn has(self, kinds: &Kinds) -> bool {
        match self {
            Group::Terms => kinds.terms,
            Group::Kanji => kinds.kanji,
            Group::Frequency => kinds.frequency,
            Group::Pitch => kinds.pitch,
        }
    }

    /// The type a dictionary's row is under. One with none (nothing a lookup
    /// uses) is with the term dictionaries, where it can still be deleted.
    pub fn main(kinds: &Kinds) -> Group {
        Group::ALL
            .into_iter()
            .find(|g| g.has(kinds))
            .unwrap_or(Group::Terms)
    }

    /// Its name for the harness and the strings (`dict-group-<name>`).
    pub fn name(self) -> &'static str {
        match self {
            Group::Terms => "terms",
            Group::Kanji => "kanji",
            Group::Frequency => "frequency",
            Group::Pitch => "pitch",
        }
    }
}

/// A row of a group.
#[derive(Debug, Clone, Copy)]
pub struct Member<'a> {
    /// The dictionary.
    pub id: &'a str,
    /// Its record.
    pub record: &'a DictionaryRecord,
    /// Its row is under another type: this one only marks its place.
    pub reference: bool,
}

/// A finished lookup or import; may run on any thread.
pub type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The dictionaries, from the application. Errors are already translated.
pub trait DictionaryService: Send {
    /// Looks `text` up away from the UI thread; `done` gets the popup content.
    fn lookup(&self, text: String, done: Done<Reply>);
    /// The registered dictionaries, in order.
    fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, String>;
    /// What importing `zip` would do (DIC-016).
    fn check(&self, zip: &Path) -> Result<Check, String>;
    /// Imports `zip` away from the UI thread, replacing `replace` (an update);
    /// `done` gets the new id.
    fn import(
        &self,
        zip: PathBuf,
        replace: Option<String>,
        cancel: Cancel,
        progress: Box<dyn Fn(Progress) + Send>,
        done: Done<String>,
    );
    /// Enables or disables a dictionary (DIC-032).
    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String>;
    /// Puts the dictionaries in this order (DIC-031).
    fn reorder(&self, ids: &[String]) -> Result<(), String>;
    /// Deletes a dictionary (DIC-021, DIC-033).
    fn delete(&self, id: &str) -> Result<(), String>;
    /// Renames a dictionary for display; `None` restores the title (DIC-033).
    fn rename(&self, id: &str, name: Option<String>) -> Result<(), String>;
    /// Sets the popup state of a dictionary's block (DIC-034).
    fn set_collapse(&self, id: &str, collapse: Collapse) -> Result<(), String>;
    /// Sets the user's CSS of a dictionary; empty clears it (DIC-035).
    fn set_user_css(&self, id: &str, css: &str) -> Result<(), String>;
}

/// Where the import of a zip is.
#[derive(Debug, Default)]
pub enum Import {
    /// Nothing running.
    #[default]
    Idle,
    /// Another revision is installed: the user confirms the update.
    Confirm {
        /// The zip.
        zip: PathBuf,
        /// The installed dictionary.
        id: String,
        /// Its title.
        title: String,
        /// Its revision.
        installed: String,
    },
    /// Running.
    Running {
        /// The zip's file name.
        file: String,
        /// The last progress.
        progress: Progress,
        /// Stops it.
        cancel: Cancel,
        /// Matches the progress and the end to this import.
        generation: u64,
    },
}

/// What the session must do after a page operation.
#[derive(Debug)]
pub enum Effect {
    /// Show a message (already translated by the caller from this id and
    /// these arguments).
    Message(&'static str, Vec<(&'static str, String)>),
    /// Show an error, already translated.
    Error(String),
    /// Start the import of `zip` (replacing `replace`) with this generation.
    Start {
        /// The zip.
        zip: PathBuf,
        /// The dictionary it updates.
        replace: Option<String>,
        /// Matches the progress and the end.
        generation: u64,
        /// Stops it.
        cancel: Cancel,
    },
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Effect::Message(a, x), Effect::Message(b, y)) => a == b && x == y,
            (Effect::Error(a), Effect::Error(b)) => a == b,
            (
                Effect::Start {
                    zip: a,
                    replace: x,
                    generation: g,
                    ..
                },
                Effect::Start {
                    zip: b,
                    replace: y,
                    generation: h,
                    ..
                },
            ) => a == b && x == y && g == h,
            _ => false,
        }
    }
}

/// The page state.
#[derive(Debug, Default)]
pub struct DictionaryPage {
    /// The registered dictionaries, in order.
    pub rows: Vec<(String, DictionaryRecord)>,
    /// The import.
    pub import: Import,
    generation: u64,
    /// Changes whenever the page changes, so the UI rebuilds it.
    pub version: u64,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl DictionaryPage {
    /// Reads the list again.
    pub fn refresh(&mut self, service: &dyn DictionaryService) -> Vec<Effect> {
        self.version += 1;
        match service.list() {
            Ok(rows) => {
                self.rows = rows;
                Vec::new()
            }
            Err(e) => vec![Effect::Error(e)],
        }
    }

    /// The user chose a zip (dialog or drop).
    pub fn choose(&mut self, service: &dyn DictionaryService, zip: PathBuf) -> Vec<Effect> {
        if !matches!(self.import, Import::Idle) {
            return vec![Effect::Message("dict-import-busy", Vec::new())];
        }
        match service.check(&zip) {
            Err(e) => vec![Effect::Error(e)],
            Ok(Check::SameRevision { id }) => {
                let title = self.title_of(&id);
                vec![Effect::Message(
                    "dict-same-revision",
                    vec![("title", title)],
                )]
            }
            Ok(Check::Update { id, installed }) => {
                let title = self.title_of(&id);
                self.import = Import::Confirm {
                    zip,
                    id,
                    title,
                    installed,
                };
                self.version += 1;
                Vec::new()
            }
            Ok(Check::New) => vec![self.start(zip, None)],
        }
    }

    fn title_of(&self, id: &str) -> String {
        self.rows
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, r)| r.name().to_string())
            .unwrap_or_default()
    }

    fn start(&mut self, zip: PathBuf, replace: Option<String>) -> Effect {
        self.generation += 1;
        let cancel = Cancel::new();
        self.import = Import::Running {
            file: file_name(&zip),
            progress: Progress::default(),
            cancel: cancel.clone(),
            generation: self.generation,
        };
        self.version += 1;
        Effect::Start {
            zip,
            replace,
            generation: self.generation,
            cancel,
        }
    }

    /// The answer to the update offer.
    pub fn confirm(&mut self, update: bool) -> Vec<Effect> {
        let Import::Confirm { zip, id, .. } = std::mem::take(&mut self.import) else {
            return Vec::new();
        };
        self.version += 1;
        if update {
            vec![self.start(zip, Some(id))]
        } else {
            Vec::new()
        }
    }

    /// Stops the running import.
    pub fn cancel(&mut self) {
        if let Import::Running { cancel, .. } = &self.import {
            cancel.cancel();
        }
    }

    /// Progress of import `generation`.
    pub fn progress(&mut self, generation: u64, p: Progress) {
        if let Import::Running {
            progress,
            generation: g,
            ..
        } = &mut self.import
            && *g == generation
        {
            *progress = p;
            self.version += 1;
        }
    }

    /// Import `generation` ended.
    pub fn finished(
        &mut self,
        service: &dyn DictionaryService,
        generation: u64,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        if !matches!(&self.import, Import::Running { generation: g, .. } if *g == generation) {
            return Vec::new();
        }
        let file = match std::mem::take(&mut self.import) {
            Import::Running { file, .. } => file,
            _ => String::new(),
        };
        let mut effects = self.refresh(service);
        match result {
            Ok(id) => {
                let title = self.title_of(&id);
                effects.push(Effect::Message("dict-imported", vec![("title", title)]));
            }
            Err(e) => effects.push(Effect::Error(format!("{file}: {e}"))),
        }
        effects
    }

    /// The rows of `group`, in the order: every dictionary of this type, those
    /// whose row is under another type as references (DIC-030).
    pub fn group(&self, group: Group) -> Vec<Member<'_>> {
        self.rows
            .iter()
            .filter_map(|(id, r)| {
                let main = Group::main(&r.kinds);
                (main == group || group.has(&r.kinds)).then_some(Member {
                    id,
                    record: r,
                    reference: main != group,
                })
            })
            .collect()
    }

    /// Moves a dictionary over the row above (`-1`) or below (`+1`) in its
    /// group, references included: the keyboard's and the screen reader's way
    /// to do what a drag does (DIC-031).
    pub fn move_by(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        delta: i32,
    ) -> Vec<Effect> {
        let Some((_, r)) = self.rows.iter().find(|(i, _)| i == id) else {
            return Vec::new();
        };
        let rows = self.group(Group::main(&r.kinds));
        let Some(at) = rows.iter().position(|m| m.id == id) else {
            return Vec::new();
        };
        let to = if delta < 0 {
            at.checked_sub(1)
        } else {
            (at + 1 < rows.len()).then_some(at + 2)
        };
        match to {
            Some(to) => self.move_to(service, id, to),
            None => Vec::new(),
        }
    }

    /// Moves a dictionary to the gap `to` of its group, as a drop does
    /// (DIC-031): 0 is above the group's first row, its length below the last.
    /// In the one order the dictionary goes just above the row under the gap,
    /// or just below the group's last row; every other dictionary keeps its
    /// place, so no other group changes but for the references to this one.
    pub fn move_to(&mut self, service: &dyn DictionaryService, id: &str, to: usize) -> Vec<Effect> {
        match self.moved(id, to) {
            Some(ids) => {
                let r = service.reorder(&ids);
                self.after(service, r)
            }
            None => Vec::new(),
        }
    }

    /// The order after [`DictionaryPage::move_to`]; `None` if nothing moves.
    fn moved(&self, id: &str, to: usize) -> Option<Vec<String>> {
        let (_, r) = self.rows.iter().find(|(i, _)| i == id)?;
        let rows: Vec<&str> = self
            .group(Group::main(&r.kinds))
            .iter()
            .map(|m| m.id)
            .collect();
        let at = rows.iter().position(|m| *m == id)?;
        let to = to.min(rows.len());
        if to == at || to == at + 1 {
            return None;
        }
        let mut ids: Vec<String> = self
            .rows
            .iter()
            .map(|(i, _)| i.clone())
            .filter(|i| i != id)
            .collect();
        let place = match rows.get(to) {
            Some(below) => ids.iter().position(|i| i == below)?,
            None => ids.iter().position(|i| Some(&i.as_str()) == rows.last())? + 1,
        };
        ids.insert(place, id.to_string());
        Some(ids)
    }

    fn after(
        &mut self,
        service: &dyn DictionaryService,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let mut effects = self.refresh(service);
        if let Err(e) = result {
            effects.push(Effect::Error(e));
        }
        effects
    }

    /// Enables or disables.
    pub fn set_enabled(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        enabled: bool,
    ) -> Vec<Effect> {
        let r = service.set_enabled(id, enabled);
        self.after(service, r)
    }

    /// Deletes (the page asked for confirmation).
    pub fn delete(&mut self, service: &dyn DictionaryService, id: &str) -> Vec<Effect> {
        let r = service.delete(id);
        self.after(service, r)
    }

    /// Renames; an empty name restores the title.
    pub fn rename(&mut self, service: &dyn DictionaryService, id: &str, name: &str) -> Vec<Effect> {
        let name = name.trim();
        let r = service.rename(id, (!name.is_empty()).then(|| name.to_string()));
        self.after(service, r)
    }

    /// Sets the block state: `automatic`, `expanded` or `collapsed`.
    pub fn set_collapse(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        state: &str,
    ) -> Vec<Effect> {
        let collapse = match state {
            "expanded" => Collapse::Expanded,
            "collapsed" => Collapse::Collapsed,
            _ => Collapse::Automatic,
        };
        let r = service.set_collapse(id, collapse);
        self.after(service, r)
    }

    /// Saves the user's CSS of a dictionary (DIC-035).
    pub fn set_user_css(
        &mut self,
        service: &dyn DictionaryService,
        id: &str,
        css: &str,
    ) -> Vec<Effect> {
        let r = service.set_user_css(id, css);
        let saved = r.is_ok();
        let mut effects = self.after(service, r);
        if saved {
            let title = self.title_of(id);
            effects.push(Effect::Message("dict-css-saved", vec![("title", title)]));
        }
        effects
    }

    /// The page as JSON, for the test harness's `state`.
    pub fn json(&self) -> Value {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|(id, r)| {
                let mut types = Vec::new();
                for (on, name) in [
                    (r.kinds.terms, "terms"),
                    (r.kinds.kanji, "kanji"),
                    (r.kinds.frequency, "frequency"),
                    (r.kinds.pitch, "pitch"),
                ] {
                    if on {
                        types.push(name);
                    }
                }
                json!({
                    "id": id,
                    "name": r.name(),
                    "title": r.title,
                    "renamed": r.display_name.is_some(),
                    "revision": r.revision,
                    "enabled": r.enabled,
                    "types": types,
                    "group": Group::main(&r.kinds).name(),
                    "terms": r.counts.terms,
                    "kanji": r.counts.kanji,
                    "frequencies": r.counts.frequencies + r.counts.kanji_meta,
                    "pitches": r.counts.pitches,
                    "size": r.size,
                    "importedAt": r.imported_at,
                    "collapse": match r.collapse {
                        Collapse::Automatic => "automatic",
                        Collapse::Expanded => "expanded",
                        Collapse::Collapsed => "collapsed",
                    },
                    "language": r.language,
                    "userCss": r.user_css,
                })
            })
            .collect();
        let import = match &self.import {
            Import::Idle => json!({"state": "idle"}),
            Import::Confirm {
                title,
                installed,
                zip,
                ..
            } => json!({
                "state": "confirm",
                "title": title,
                "installed": installed,
                "file": file_name(zip),
            }),
            Import::Running { file, progress, .. } => json!({
                "state": "running",
                "file": file,
                "done": progress.files_done,
                "total": progress.files_total,
                "entries": progress.entries,
            }),
        };
        json!({"rows": rows, "import": import})
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use nikomiru_dict::store::Counts;
    use nikomiru_dict::yomitan::Kinds;

    use super::*;

    fn record(title: &str, revision: &str) -> DictionaryRecord {
        DictionaryRecord {
            title: title.into(),
            revision: revision.into(),
            kinds: Kinds {
                terms: true,
                ..Kinds::default()
            },
            order: 0,
            enabled: true,
            language: Some("ja".into()),
            display_name: None,
            collapse: Collapse::Automatic,
            user_css: String::new(),
            counts: Counts {
                terms: 76,
                ..Counts::default()
            },
            imported_at: 0,
            size: 1024,
        }
    }

    #[derive(Default)]
    struct Fake {
        rows: Mutex<Vec<(String, DictionaryRecord)>>,
        checks: Mutex<Vec<(PathBuf, Check)>>,
        calls: Mutex<Vec<String>>,
    }

    impl DictionaryService for Fake {
        fn lookup(&self, _text: String, done: Done<Reply>) {
            done(Ok(Reply::default()));
        }
        fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, String> {
            Ok(self.rows.lock().unwrap().clone())
        }
        fn check(&self, zip: &Path) -> Result<Check, String> {
            self.checks
                .lock()
                .unwrap()
                .iter()
                .find(|(p, _)| p == zip)
                .map(|(_, c)| c.clone())
                .ok_or_else(|| "not a dictionary".to_string())
        }
        fn import(
            &self,
            zip: PathBuf,
            replace: Option<String>,
            _cancel: Cancel,
            _progress: Box<dyn Fn(Progress) + Send>,
            _done: Done<String>,
        ) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("import {} {replace:?}", zip.display()));
        }
        fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
            for (i, r) in self.rows.lock().unwrap().iter_mut() {
                if i == id {
                    r.enabled = enabled;
                }
            }
            Ok(())
        }
        fn reorder(&self, ids: &[String]) -> Result<(), String> {
            let mut rows = self.rows.lock().unwrap();
            rows.sort_by_key(|(i, _)| ids.iter().position(|x| x == i));
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.rows.lock().unwrap().retain(|(i, _)| i != id);
            Ok(())
        }
        fn rename(&self, id: &str, name: Option<String>) -> Result<(), String> {
            for (i, r) in self.rows.lock().unwrap().iter_mut() {
                if i == id {
                    r.display_name.clone_from(&name);
                }
            }
            Ok(())
        }
        fn set_collapse(&self, _id: &str, _collapse: Collapse) -> Result<(), String> {
            Err("read-only".into())
        }
        fn set_user_css(&self, id: &str, css: &str) -> Result<(), String> {
            let mut rows = self.rows.lock().unwrap();
            let (_, r) = rows
                .iter_mut()
                .find(|(i, _)| i == id)
                .ok_or_else(|| "no such dictionary".to_string())?;
            r.user_css = css.to_string();
            Ok(())
        }
    }

    fn fake() -> Fake {
        let f = Fake::default();
        *f.rows.lock().unwrap() = vec![
            ("a".into(), record("JMdict", "1")),
            ("b".into(), record("Freq", "1")),
        ];
        *f.checks.lock().unwrap() = vec![
            ("/new.zip".into(), Check::New),
            ("/same.zip".into(), Check::SameRevision { id: "a".into() }),
            (
                "/newer.zip".into(),
                Check::Update {
                    id: "a".into(),
                    installed: "1".into(),
                },
            ),
        ];
        f
    }

    // T-DIC-016 (page)
    #[test]
    fn import_new_same_and_newer() {
        let f = fake();
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        let e = page.choose(&f, "/same.zip".into());
        assert_eq!(
            e,
            [Effect::Message(
                "dict-same-revision",
                vec![("title", "JMdict".into())]
            )]
        );
        assert!(matches!(page.import, Import::Idle));

        let e = page.choose(&f, "/newer.zip".into());
        assert!(e.is_empty());
        assert_eq!(page.json()["import"]["state"], "confirm");
        assert_eq!(page.json()["import"]["installed"], "1");
        let e = page.confirm(true);
        assert!(
            matches!(&e[..], [Effect::Start { replace: Some(id), generation: 1, .. }] if id == "a")
        );
        assert_eq!(page.json()["import"]["state"], "running");
        // a second import waits for the first
        assert_eq!(
            page.choose(&f, "/new.zip".into()),
            [Effect::Message("dict-import-busy", Vec::new())]
        );

        page.progress(
            1,
            Progress {
                files_done: 2,
                files_total: 4,
                entries: 10,
            },
        );
        assert_eq!(page.json()["import"]["done"], 2);
        page.progress(
            7,
            Progress {
                files_done: 4,
                files_total: 4,
                entries: 99,
            },
        );
        assert_eq!(
            page.json()["import"]["done"],
            2,
            "another generation is ignored"
        );
        let e = page.finished(&f, 1, Ok("a".into()));
        assert_eq!(
            e,
            [Effect::Message(
                "dict-imported",
                vec![("title", "JMdict".into())]
            )]
        );
        assert_eq!(page.json()["import"]["state"], "idle");

        let e = page.choose(&f, "/broken.zip".into());
        assert_eq!(e, [Effect::Error("not a dictionary".into())]);
        let e = page.choose(&f, "/new.zip".into());
        assert!(
            matches!(&e[..], [Effect::Start { replace: None, generation: 2, cancel, .. }] if !cancel.is_cancelled())
        );
        page.cancel();
        let Import::Running { cancel, .. } = &page.import else {
            panic!()
        };
        assert!(cancel.is_cancelled());
        let e = page.finished(&f, 2, Err("import cancelled".into()));
        assert_eq!(e, [Effect::Error("new.zip: import cancelled".into())]);
    }

    // T-DIC-031, T-DIC-032, T-DIC-033
    #[test]
    fn order_enable_rename_delete() {
        let f = fake();
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        page.move_by(&f, "b", -1);
        let ids: Vec<&str> = page.rows.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert!(page.move_by(&f, "b", -1).is_empty(), "already first");
        page.set_enabled(&f, "a", false);
        assert_eq!(page.json()["rows"][1]["enabled"], false);
        page.rename(&f, "a", "  My JMdict ");
        assert_eq!(page.json()["rows"][1]["name"], "My JMdict");
        page.rename(&f, "a", "");
        assert_eq!(page.json()["rows"][1]["name"], "JMdict");
        assert_eq!(
            page.set_collapse(&f, "a", "collapsed"),
            [Effect::Error("read-only".into())]
        );
        page.delete(&f, "b");
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.json()["rows"][0]["types"], json!(["terms"]));
    }

    fn order(page: &DictionaryPage) -> Vec<&str> {
        page.rows.iter().map(|(i, _)| i.as_str()).collect()
    }

    /// A group's rows, references in brackets.
    fn rows_of(page: &DictionaryPage, group: Group) -> Vec<String> {
        page.group(group)
            .iter()
            .map(|m| {
                if m.reference {
                    format!("({})", m.id)
                } else {
                    m.id.to_string()
                }
            })
            .collect()
    }

    // T-DIC-030, T-DIC-031
    #[test]
    fn rows_go_under_their_main_type_and_move_within_it() {
        let kinds = |terms, kanji, frequency, pitch| Kinds {
            terms,
            kanji,
            frequency,
            pitch,
        };
        let of = |title: &str, kinds: Kinds| DictionaryRecord {
            kinds,
            ..record(title, "1")
        };
        let f = fake();
        *f.rows.lock().unwrap() = vec![
            ("a".into(), of("A", kinds(true, false, false, false))),
            ("f".into(), of("F", kinds(false, false, true, false))),
            ("b".into(), of("B", kinds(true, false, true, false))),
            ("p".into(), of("P", kinds(false, false, false, true))),
            ("c".into(), of("C", kinds(true, false, false, false))),
            ("k".into(), of("K", kinds(false, true, true, false))),
            ("x".into(), of("X", Kinds::default())),
        ];
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        assert_eq!(rows_of(&page, Group::Terms), ["a", "b", "c", "x"]);
        assert_eq!(rows_of(&page, Group::Kanji), ["k"]);
        assert_eq!(rows_of(&page, Group::Frequency), ["f", "(b)", "(k)"]);
        assert_eq!(rows_of(&page, Group::Pitch), ["p"]);
        assert_eq!(page.json()["rows"][2]["group"], "terms");
        assert_eq!(page.json()["rows"][5]["group"], "kanji");

        // above the group's first row: just above it in the order, and
        // nothing else moves
        page.move_to(&f, "c", 0);
        assert_eq!(order(&page), ["c", "a", "f", "b", "p", "k", "x"]);
        // below the group's last row: just below it; a frequency dictionary
        // goes after the frequencies of a term dictionary
        page.move_to(&f, "f", 3);
        assert_eq!(order(&page), ["c", "a", "b", "p", "k", "f", "x"]);
        assert_eq!(rows_of(&page, Group::Frequency), ["(b)", "(k)", "f"]);
        assert_eq!(rows_of(&page, Group::Terms), ["c", "a", "b", "x"]);
        // onto itself, or into the gaps around it: nothing
        assert!(page.move_to(&f, "f", 2).is_empty());
        assert!(page.move_to(&f, "f", 3).is_empty());
        assert_eq!(order(&page), ["c", "a", "b", "p", "k", "f", "x"]);

        // ▲ and ▼ step over the rows of the group, references included
        page.move_by(&f, "f", -1);
        assert_eq!(rows_of(&page, Group::Frequency), ["(b)", "f", "(k)"]);
        page.move_by(&f, "b", 1);
        assert_eq!(rows_of(&page, Group::Terms), ["c", "a", "x", "b"]);
        assert_eq!(
            rows_of(&page, Group::Frequency),
            ["f", "(k)", "(b)"],
            "its references follow it"
        );
        assert!(page.move_by(&f, "c", -1).is_empty(), "first of its group");
        assert!(page.move_by(&f, "b", 1).is_empty(), "last of its group");
        assert!(page.move_by(&f, "p", 1).is_empty(), "alone in its group");
        assert!(page.move_to(&f, "gone", 0).is_empty());
    }

    // T-DIC-035
    #[test]
    fn the_users_css_is_saved_and_the_page_reads_it_back() {
        let f = fake();
        let mut page = DictionaryPage::default();
        page.refresh(&f);
        let version = page.version;
        let css = "span { color: red }";
        assert_eq!(
            page.set_user_css(&f, "b", css),
            [Effect::Message(
                "dict-css-saved",
                vec![("title", "Freq".into())]
            )]
        );
        assert!(page.version > version, "the page is drawn again");
        assert_eq!(page.json()["rows"][1]["userCss"], css);
        assert_eq!(page.rows[1].1.user_css, css);
        // a failure is said, and nothing is claimed saved
        assert_eq!(
            page.set_user_css(&f, "gone", css),
            [Effect::Error("no such dictionary".into())]
        );
    }
}
