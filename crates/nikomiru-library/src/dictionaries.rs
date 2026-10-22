// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary registry (DIC-021, DIC-030…DIC-035) in the `dictionaries`
//! table of `library.redb`, and [`DictionaryManager`], which keeps it in step
//! with the folders under `dicts/`: import and update (DIC-016), deletion,
//! order, enabling, and the [`Lookup`] over the enabled dictionaries.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use nikomiru_core::{Cancel, Clock};
use nikomiru_dict::import::{self, ImportError, Imported, Progress};
use nikomiru_dict::lookup::{Dictionary, Lookup};
use nikomiru_dict::store::{Counts, RedbStore, StoreError};
use nikomiru_dict::yomitan::Kinds;
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError};

const DICTIONARIES: TableDefinition<&str, &[u8]> = TableDefinition::new("dictionaries");

/// How a dictionary's block starts in the popup (DIC-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Collapse {
    /// Expanded if among the first ones.
    #[default]
    Automatic,
    /// Always expanded.
    Expanded,
    /// Always collapsed.
    Collapsed,
}

/// A registered dictionary (SPECIFICATION §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryRecord {
    /// `index.json` title.
    pub title: String,
    /// `index.json` revision.
    pub revision: String,
    /// What it contains.
    pub kinds: Kinds,
    /// Position in the user's order (DIC-031).
    pub order: u32,
    /// Queried or not (DIC-032).
    pub enabled: bool,
    /// Declared source language.
    pub language: Option<String>,
    /// Name shown instead of the title (DIC-033).
    pub display_name: Option<String>,
    /// Popup block state (DIC-034).
    pub collapse: Collapse,
    /// The user's CSS for this dictionary (DIC-035).
    pub user_css: String,
    /// Entry counts (DIC-030).
    pub counts: Counts,
    /// Import time, Unix ms.
    pub imported_at: i64,
    /// Bytes on disk.
    pub size: u64,
}

impl DictionaryRecord {
    /// The name shown: the display name, or the title.
    pub fn name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.title)
    }
}

/// The records of the registry table, in the user's order.
fn in_order(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
) -> Result<Vec<(String, DictionaryRecord)>, LibraryError> {
    let mut out = Vec::new();
    for row in table.iter().map_err(|e| LibraryError::Db(e.into()))? {
        let (k, v) = row.map_err(|e| LibraryError::Db(e.into()))?;
        out.push((
            k.value().to_string(),
            postcard::from_bytes::<DictionaryRecord>(v.value())?,
        ));
    }
    out.sort_by(|a, b| a.1.order.cmp(&b.1.order).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

impl Library {
    /// The registered dictionaries, in the user's order.
    pub fn dictionaries(&self) -> Result<Vec<(String, DictionaryRecord)>, LibraryError> {
        self.read(|tx| match tx.open_table(DICTIONARIES) {
            Ok(t) => in_order(&t),
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(Vec::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Applies `edit` to the registry and returns what it returns: the list it
    /// gets is in order, and the order it leaves is renumbered from 0.
    ///
    /// The list is read and written back in one write transaction, and redb
    /// runs those one at a time: a page edit on the UI thread and an import's
    /// registration on its own thread never overwrite each other, and a
    /// deleted dictionary never comes back. `edit` must not write to the
    /// library: it would wait for its own transaction.
    pub fn edit_dictionaries<T>(
        &self,
        edit: impl FnOnce(&mut Vec<(String, DictionaryRecord)>) -> T,
    ) -> Result<T, LibraryError> {
        let tx = self.write()?;
        let out = {
            let mut t = tx
                .open_table(DICTIONARIES)
                .map_err(|e| LibraryError::Db(e.into()))?;
            let mut list = in_order(&t)?;
            let before: Vec<String> = list.iter().map(|(id, _)| id.clone()).collect();
            let out = edit(&mut list);
            for id in &before {
                if !list.iter().any(|(i, _)| i == id) {
                    t.remove(id.as_str())
                        .map_err(|e| LibraryError::Db(e.into()))?;
                }
            }
            for (n, (id, rec)) in list.iter_mut().enumerate() {
                rec.order = u32::try_from(n).unwrap_or(u32::MAX);
                let bytes = postcard::to_stdvec(&*rec)?;
                t.insert(id.as_str(), bytes.as_slice())
                    .map_err(|e| LibraryError::Db(e.into()))?;
            }
            out
        };
        tx.commit().map_err(|e| LibraryError::Db(e.into()))?;
        Ok(out)
    }
}

/// Errors of the dictionary manager.
#[derive(Debug, thiserror::Error)]
pub enum DictionaryError {
    /// The import failed.
    #[error(transparent)]
    Import(#[from] ImportError),
    /// A dictionary file cannot be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The registry cannot be read or written.
    #[error(transparent)]
    Library(#[from] LibraryError),
    /// The same title and revision are installed (DIC-016).
    #[error("{title} revision {revision} is already installed")]
    SameRevision {
        /// The title.
        title: String,
        /// The revision.
        revision: String,
    },
    /// The folder of a dictionary cannot be removed.
    #[error("{path}: {source}")]
    Io {
        /// The folder.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// No dictionary has this id.
    #[error("unknown dictionary {0}")]
    Unknown(String),
}

impl nikomiru_core::Localized for DictionaryError {
    fn message_id(&self) -> &'static str {
        match self {
            DictionaryError::Import(e) => e.message_id(),
            DictionaryError::Store(e) => e.message_id(),
            DictionaryError::Library(e) => e.message_id(),
            DictionaryError::SameRevision { .. } => "error-dict-same-revision",
            DictionaryError::Io { .. } => "error-dict-io",
            DictionaryError::Unknown(_) => "error-dict-unknown",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            DictionaryError::Import(e) => e.message_args(),
            DictionaryError::Store(e) => e.message_args(),
            DictionaryError::Library(e) => e.message_args(),
            DictionaryError::SameRevision { title, revision } => {
                vec![("title", title.clone()), ("revision", revision.clone())]
            }
            DictionaryError::Io { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            DictionaryError::Unknown(id) => vec![("id", id.clone())],
        }
    }
}

/// What importing a zip would do (DIC-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// A new dictionary.
    New,
    /// The same title and revision are installed: refused.
    SameRevision {
        /// The installed one.
        id: String,
    },
    /// Another revision is installed: offer to update it.
    Update {
        /// The installed one.
        id: String,
        /// Its revision.
        installed: String,
    },
}

/// The registry and the dictionary folders, kept in step.
pub struct DictionaryManager {
    library: Arc<Library>,
    dir: PathBuf,
    clock: Arc<dyn Clock>,
    study_language: Mutex<String>,
    stores: Mutex<HashMap<String, Arc<RedbStore>>>,
    lookup: Mutex<Option<Arc<Lookup>>>,
}

impl std::fmt::Debug for DictionaryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DictionaryManager")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => dir_size(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

impl DictionaryManager {
    /// The manager of the dictionaries in `dir` (`$XDG_DATA_HOME/nikomiru/dicts`).
    /// It removes what an interrupted import left (DIC-013): staging folders,
    /// folders the registry does not know, records whose folder is gone.
    pub fn open(
        library: Arc<Library>,
        dir: PathBuf,
        clock: Arc<dyn Clock>,
        study_language: &str,
    ) -> Result<Self, DictionaryError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| DictionaryError::Io { path, source }
        };
        import::cleanup(&dir).map_err(io(&dir))?;
        let known = library.dictionaries()?;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.path().is_dir() && !known.iter().any(|(id, _)| *id == name) {
                    tracing::warn!(folder = %name, "removing a dictionary folder the registry does not know");
                    std::fs::remove_dir_all(e.path()).map_err(io(&e.path()))?;
                }
            }
        }
        if known
            .iter()
            .any(|(id, _)| !dir.join(id).join(import::DB_FILE).is_file())
        {
            library.edit_dictionaries(|list| {
                list.retain(|(id, _)| dir.join(id).join(import::DB_FILE).is_file())
            })?;
        }
        Ok(Self {
            library,
            dir,
            clock,
            study_language: Mutex::new(study_language.to_string()),
            stores: Mutex::new(HashMap::new()),
            lookup: Mutex::new(None),
        })
    }

    /// The registered dictionaries, in order.
    pub fn list(&self) -> Result<Vec<(String, DictionaryRecord)>, DictionaryError> {
        Ok(self.library.dictionaries()?)
    }

    /// What importing `zip` would do (DIC-016).
    pub fn check(&self, zip: &Path) -> Result<Check, DictionaryError> {
        let index = import::read_index(zip)?;
        for (id, rec) in self.library.dictionaries()? {
            if rec.title == index.title {
                return Ok(if rec.revision == index.revision {
                    Check::SameRevision { id }
                } else {
                    Check::Update {
                        id,
                        installed: rec.revision,
                    }
                });
            }
        }
        Ok(Check::New)
    }

    /// Imports `zip`. A new dictionary goes last and enabled; an update
    /// (`replace`, after [`Check::Update`]) takes the place and the settings of
    /// the old one, which is deleted only once the new one is registered. Until
    /// then `cancel` stops it, and a cancelled import leaves nothing (DIC-010,
    /// DIC-013).
    pub fn import(
        &self,
        zip: &Path,
        replace: Option<&str>,
        cancel: &Cancel,
        progress: impl FnMut(Progress),
    ) -> Result<String, DictionaryError> {
        let check = self.check(zip)?;
        if let Check::SameRevision { id } = &check {
            let rec = self.record(id)?;
            return Err(DictionaryError::SameRevision {
                title: rec.title,
                revision: rec.revision,
            });
        }
        let imported = import::import(zip, &self.dir, cancel, progress)?;
        let registered = if cancel.is_cancelled() {
            Err(ImportError::Cancelled.into())
        } else {
            self.register(&imported, replace)
        };
        let replaced = match registered {
            Ok(replaced) => replaced,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&imported.dir);
                return Err(e);
            }
        };
        self.invalidate();
        // The update is done: a folder that cannot be removed now is one the
        // registry no longer knows, and the next start removes it (DIC-013).
        if let Some(old) = replaced
            && let Err(e) = self.remove_folder(&old)
        {
            tracing::warn!("the replaced dictionary stays on disk until the next start: {e}");
        }
        Ok(imported.id)
    }

    /// Registers a finished import, in place of `replace` if that is still
    /// registered; returns the id it replaced. The registry is checked again
    /// in the transaction that writes it: it may have changed while the
    /// dictionary was written (an edit on the page, another import).
    fn register(
        &self,
        imported: &Imported,
        replace: Option<&str>,
    ) -> Result<Option<String>, DictionaryError> {
        let info = &imported.info;
        let imported_at = self
            .clock
            .now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let mut record = DictionaryRecord {
            title: info.index.title.clone(),
            revision: info.index.revision.clone(),
            kinds: info.kinds,
            order: u32::MAX,
            enabled: true,
            language: info.index.source_language.clone(),
            display_name: None,
            collapse: Collapse::Automatic,
            user_css: String::new(),
            counts: info.counts,
            imported_at,
            size: dir_size(&imported.dir),
        };
        self.library.edit_dictionaries(|list| {
            if list
                .iter()
                .any(|(_, r)| r.title == record.title && r.revision == record.revision)
            {
                return Err(DictionaryError::SameRevision {
                    title: record.title.clone(),
                    revision: record.revision.clone(),
                });
            }
            let id = imported.id.clone();
            match replace.and_then(|old| list.iter_mut().find(|(i, _)| i == old)) {
                Some(slot) => {
                    let prev = &slot.1;
                    record.enabled = prev.enabled;
                    record.display_name.clone_from(&prev.display_name);
                    record.collapse = prev.collapse;
                    record.user_css.clone_from(&prev.user_css);
                    let (old, _) = std::mem::replace(slot, (id, record));
                    Ok(Some(old))
                }
                // a new dictionary, or an update whose old one was deleted meanwhile
                None => {
                    list.push((id, record));
                    Ok(None)
                }
            }
        })?
    }

    fn record(&self, id: &str) -> Result<DictionaryRecord, DictionaryError> {
        self.library
            .dictionaries()?
            .into_iter()
            .find(|(i, _)| i == id)
            .map(|(_, r)| r)
            .ok_or_else(|| DictionaryError::Unknown(id.to_string()))
    }

    fn remove_folder(&self, id: &str) -> Result<(), DictionaryError> {
        if let Ok(mut stores) = self.stores.lock() {
            stores.remove(id);
        }
        let path = self.dir.join(id);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(DictionaryError::Io { path, source }),
        }
    }

    /// Deletes a dictionary: out of the registry, then off the disk (DIC-021).
    pub fn delete(&self, id: &str) -> Result<(), DictionaryError> {
        let found = self.library.edit_dictionaries(|list| {
            let before = list.len();
            list.retain(|(i, _)| i != id);
            list.len() < before
        })?;
        if !found {
            return Err(DictionaryError::Unknown(id.to_string()));
        }
        self.invalidate();
        self.remove_folder(id)
    }

    /// Enables or disables a dictionary (DIC-032).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), DictionaryError> {
        self.update(id, |r| r.enabled = enabled)
    }

    /// Renames a dictionary for display only (DIC-033); `None` restores the title.
    pub fn rename(&self, id: &str, name: Option<String>) -> Result<(), DictionaryError> {
        self.update(id, |r| {
            r.display_name = name.filter(|n| !n.trim().is_empty())
        })
    }

    /// Sets the popup state of a dictionary (DIC-034).
    pub fn set_collapse(&self, id: &str, collapse: Collapse) -> Result<(), DictionaryError> {
        self.update(id, |r| r.collapse = collapse)
    }

    /// Sets the user's CSS of a dictionary (DIC-035), applied after its
    /// `styles.css` through the same safe subset; an update keeps it.
    pub fn set_user_css(&self, id: &str, css: &str) -> Result<(), DictionaryError> {
        self.update(id, |r| r.user_css = css.to_string())
    }

    /// Puts the dictionaries in this order (DIC-031); ids not listed keep
    /// their relative order after the listed ones.
    pub fn reorder(&self, ids: &[String]) -> Result<(), DictionaryError> {
        self.library.edit_dictionaries(|list| {
            list.sort_by_key(|(id, _)| ids.iter().position(|i| i == id).unwrap_or(usize::MAX));
        })?;
        self.invalidate();
        Ok(())
    }

    /// Changes the study language (LKP-012).
    pub fn set_study_language(&self, language: &str) {
        if let Ok(mut l) = self.study_language.lock() {
            *l = language.to_string();
        }
        self.invalidate();
    }

    fn update(
        &self,
        id: &str,
        edit: impl FnOnce(&mut DictionaryRecord),
    ) -> Result<(), DictionaryError> {
        let found = self.library.edit_dictionaries(|list| {
            list.iter_mut()
                .find(|(i, _)| i == id)
                .map(|(_, r)| edit(r))
                .is_some()
        })?;
        if !found {
            return Err(DictionaryError::Unknown(id.to_string()));
        }
        self.invalidate();
        Ok(())
    }

    /// Drops the current lookup and its cache (LKP-038).
    fn invalidate(&self) {
        if let Ok(mut l) = self.lookup.lock() {
            *l = None;
        }
    }

    /// The lookup over the enabled dictionaries, in order; the same one until
    /// something changes.
    pub fn lookup(&self) -> Result<Arc<Lookup>, DictionaryError> {
        if let Some(l) = self.lookup.lock().ok().and_then(|l| l.clone()) {
            return Ok(l);
        }
        let mut dictionaries = Vec::new();
        for (id, rec) in self.library.dictionaries()? {
            if !rec.enabled {
                continue;
            }
            let store = self.store(&id)?;
            dictionaries.push(Dictionary {
                name: rec.name().to_string(),
                id,
                store,
            });
        }
        let language = self
            .study_language
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default();
        let lookup = Arc::new(Lookup::new(dictionaries, &language));
        if let Ok(mut l) = self.lookup.lock() {
            *l = Some(Arc::clone(&lookup));
        }
        Ok(lookup)
    }

    /// The open store of a dictionary (for media and tags).
    pub fn store(&self, id: &str) -> Result<Arc<RedbStore>, DictionaryError> {
        if let Some(s) = self.stores.lock().ok().and_then(|s| s.get(id).cloned()) {
            return Ok(s);
        }
        let store = Arc::new(RedbStore::open(&self.dir.join(id).join(import::DB_FILE))?);
        if let Ok(mut stores) = self.stores.lock() {
            stores.insert(id.to_string(), Arc::clone(&store));
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(title: &str, order: u32) -> DictionaryRecord {
        DictionaryRecord {
            title: title.into(),
            revision: "1".into(),
            kinds: Kinds::default(),
            order,
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

    #[test]
    fn the_registry_keeps_the_order() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert!(
            lib.dictionaries().unwrap().is_empty(),
            "a schema-1 file has no table yet"
        );
        lib.edit_dictionaries(|l| {
            l.push(("b".into(), record("B", 0)));
            l.push(("a".into(), record("A", 0)));
        })
        .unwrap();
        let ids: Vec<String> = lib
            .dictionaries()
            .unwrap()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(ids, ["b", "a"]);
        lib.edit_dictionaries(|l| l.retain(|(i, _)| i == "a"))
            .unwrap();
        let all = lib.dictionaries().unwrap();
        assert_eq!((all.len(), all[0].1.order), (1, 0));
        assert_eq!(all[0].1.name(), "A");
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let errors = [
            DictionaryError::SameRevision {
                title: "t".into(),
                revision: "1".into(),
            },
            DictionaryError::Io {
                path: "/d".into(),
                source: std::io::Error::other("x"),
            },
            DictionaryError::Unknown("x".into()),
            DictionaryError::Import(ImportError::Cancelled),
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let problems =
            nikomiru_core::error::message_problems(Path::new(nikomiru_core::error::I18N_DIR), &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
