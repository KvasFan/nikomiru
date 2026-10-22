// SPDX-License-Identifier: GPL-3.0-only

//! Dictionary storage (SPECIFICATION §6.5.3, DIC-020): one redb file per
//! dictionary behind the [`DictStore`] trait. Records are postcard; glossaries
//! are JSON compressed with lz4 and decompressed only for the entries that are
//! shown (LKP-035).

use std::path::Path;

use nikomiru_core::Cancel;
use redb::{
    Database, MultimapTableDefinition, ReadOnlyMultimapTable, ReadOnlyTable, ReadableDatabase,
    TableDefinition, WriteTransaction,
};
use serde::{Deserialize, Serialize};

use crate::bloom::Bloom;
use crate::yomitan::{Index, Kanji, Kinds, Meta, Tag, Term};

/// Schema version of the dictionary files.
pub const SCHEMA: u32 = 2;

const INFO: TableDefinition<&str, &[u8]> = TableDefinition::new("info");
const TERMS: TableDefinition<u64, &[u8]> = TableDefinition::new("terms");
const BY_EXPRESSION: MultimapTableDefinition<&str, &[u8]> =
    MultimapTableDefinition::new("term_by_expression");
const BY_READING: MultimapTableDefinition<&str, &[u8]> =
    MultimapTableDefinition::new("term_by_reading");
const TERM_META: MultimapTableDefinition<&str, &[u8]> = MultimapTableDefinition::new("term_meta");
const KANJI: TableDefinition<&str, &[u8]> = TableDefinition::new("kanji");
const KANJI_META: MultimapTableDefinition<&str, &[u8]> = MultimapTableDefinition::new("kanji_meta");
const TAGS: TableDefinition<&str, &[u8]> = TableDefinition::new("tags");
const MEDIA: TableDefinition<&str, &[u8]> = TableDefinition::new("media");

/// Errors of the storage.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// redb failed.
    #[error("dictionary database: {0}")]
    Db(#[from] redb::Error),
    /// A record could not be decoded.
    #[error("dictionary record: {0}")]
    Record(#[from] postcard::Error),
    /// A glossary could not be decompressed.
    #[error("dictionary glossary: {0}")]
    Glossary(String),
    /// The file was written by another schema version.
    #[error("dictionary schema {0} is not supported")]
    Schema(u32),
}

impl nikomiru_core::Localized for StoreError {
    fn message_id(&self) -> &'static str {
        match self {
            StoreError::Db(_)
            | StoreError::Record(_)
            | StoreError::Glossary(_)
            | StoreError::Schema(_) => "error-dict-store",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        vec![("reason", self.to_string())]
    }
}

macro_rules! db {
    ($e:expr) => {
        $e.map_err(|e| StoreError::Db(e.into()))
    };
}

/// Entry counts of a dictionary (DIC-030).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Terms.
    pub terms: u64,
    /// Term meta (frequencies, pitch, IPA).
    pub term_meta: u64,
    /// Kanji.
    pub kanji: u64,
    /// Kanji meta.
    pub kanji_meta: u64,
    /// Tags.
    pub tags: u64,
    /// Media files.
    pub media: u64,
    /// Terms with deinflection rules: with none, empty rules mean `*`
    /// (LKP-023, BUG-2543).
    pub with_rules: u64,
    /// Term meta rows with frequencies.
    pub frequencies: u64,
    /// Term meta rows with pitch accents or IPA.
    pub pitches: u64,
}

/// What a dictionary file says about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictInfo {
    /// `index.json`.
    pub index: Index,
    /// What it contains.
    pub kinds: Kinds,
    /// Entry counts.
    pub counts: Counts,
    /// `styles.css`, if any (DIC-035).
    pub styles: String,
}

/// A term as stored: the glossary stays compressed until asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredTerm {
    /// Id inside the dictionary.
    pub id: u64,
    /// Headword.
    pub expression: String,
    /// Reading.
    pub reading: String,
    /// Definition tags, space separated.
    pub definition_tags: String,
    /// Deinflection rules, space separated.
    pub rules: String,
    /// Score.
    pub score: f64,
    /// Sequence number.
    pub sequence: i64,
    /// Term tags, space separated.
    pub term_tags: String,
    /// The glossary only points to other headwords (LKP-032): checked once,
    /// at import, so lookups decompress only these.
    pub redirect: bool,
    glossary_lz4: Vec<u8>,
}

impl StoredTerm {
    /// The record of `term`, with id `id`.
    pub fn new(id: u64, t: Term) -> Self {
        Self {
            id,
            expression: t.expression,
            reading: t.reading,
            definition_tags: t.definition_tags,
            rules: t.rules,
            score: t.score,
            sequence: t.sequence,
            term_tags: t.term_tags,
            redirect: crate::yomitan::parse_redirects(&t.glossary).is_some(),
            glossary_lz4: lz4_flex::compress_prepend_size(t.glossary.as_bytes()),
        }
    }

    /// The glossary JSON.
    pub fn glossary(&self) -> Result<String, StoreError> {
        let bytes = lz4_flex::decompress_size_prepended(&self.glossary_lz4)
            .map_err(|e| StoreError::Glossary(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| StoreError::Glossary(e.to_string()))
    }

    /// Length of the glossary JSON in bytes, without decompressing it.
    pub fn glossary_len(&self) -> usize {
        self.glossary_lz4
            .first_chunk::<4>()
            .map_or(0, |n| u32::from_le_bytes(*n) as usize)
    }
}

/// What a lookup needs of a term before choosing what to show: the indexes
/// hold it, so a reading with hundreds of homophones is one sequential read,
/// and the record itself is read only for the entries shown (LKP-035).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TermHead {
    /// Id of the record.
    pub id: u64,
    /// Headword.
    pub expression: String,
    /// Reading.
    pub reading: String,
    /// Deinflection rules, space separated.
    pub rules: String,
    /// Score.
    pub score: f64,
    /// The glossary only points to other headwords (LKP-032).
    pub redirect: bool,
}

impl TermHead {
    /// The head of a record.
    pub fn of(t: &StoredTerm) -> Self {
        Self {
            id: t.id,
            expression: t.expression.clone(),
            reading: t.reading.clone(),
            rules: t.rules.clone(),
            score: t.score,
            redirect: t.redirect,
        }
    }
}

/// Which index a term search uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The headword.
    Expression,
    /// The reading.
    Reading,
}

/// Queries on one snapshot of a dictionary. A lookup makes hundreds of
/// queries: it opens one reader per dictionary instead of one read
/// transaction per query.
pub trait DictReader {
    /// Heads of the terms whose `field` equals `text`, by id.
    fn heads(&self, field: Field, text: &str) -> Result<Vec<TermHead>, StoreError>;
    /// The record with this id.
    fn term(&self, id: u64) -> Result<Option<StoredTerm>, StoreError>;
    /// Terms whose `field` equals `text`, by id.
    fn terms(&self, field: Field, text: &str) -> Result<Vec<StoredTerm>, StoreError> {
        let mut out = Vec::new();
        for head in self.heads(field, text)? {
            out.extend(self.term(head.id)?);
        }
        Ok(out)
    }
    /// Frequencies, pitch accents and IPA of `text`.
    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError>;
    /// The kanji entry of `character`.
    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError>;
    /// Frequencies of `character`.
    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError>;
}

/// Read access to one dictionary (DIC-020).
pub trait DictStore: Send + Sync {
    /// What the dictionary says about itself.
    fn info(&self) -> &DictInfo;
    /// A reader on the current contents.
    fn reader(&self) -> Result<Box<dyn DictReader + '_>, StoreError>;
    /// The tag called `name`.
    fn tag(&self, name: &str) -> Result<Option<Tag>, StoreError>;
    /// A media file (DIC-001, POP-010), by its normalized path.
    fn media(&self, path: &str) -> Result<Option<Vec<u8>>, StoreError>;

    /// Terms whose `field` equals `text` (one query: see [`DictReader`]).
    fn terms(&self, field: Field, text: &str) -> Result<Vec<StoredTerm>, StoreError> {
        self.reader()?.terms(field, text)
    }
    /// Frequencies, pitch accents and IPA of `text`.
    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError> {
        self.reader()?.term_meta(text)
    }
    /// The kanji entry of `character`.
    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError> {
        self.reader()?.kanji(character)
    }
    /// Frequencies of `character`.
    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError> {
        self.reader()?.kanji_meta(character)
    }
}

/// The normalized form of a media path: `/` separators, no leading `/`.
pub fn media_key(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

/// A dictionary file opened for reading.
#[derive(Debug)]
pub struct RedbStore {
    db: Database,
    info: DictInfo,
    /// The keys of both term indexes.
    filter: Bloom,
}

fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, StoreError> {
    Ok(postcard::from_bytes(bytes)?)
}

impl RedbStore {
    /// Opens a dictionary file.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db = db!(Database::builder().set_cache_size(16 << 20).open(path))?;
        let (info, filter) = {
            let txn = db!(db.begin_read())?;
            let table = db!(txn.open_table(INFO))?;
            let schema: u32 = match db!(table.get("schema"))? {
                Some(v) => decode(v.value())?,
                None => 0,
            };
            if schema != SCHEMA {
                return Err(StoreError::Schema(schema));
            }
            let info = db!(table.get("info"))?.ok_or(StoreError::Schema(0))?;
            let filter = db!(table.get("filter"))?
                .and_then(|f| Bloom::from_bytes(f.value()))
                .ok_or(StoreError::Schema(schema))?;
            (decode::<DictInfo>(info.value())?, filter)
        };
        Ok(Self { db, info, filter })
    }
}

impl DictStore for RedbStore {
    fn info(&self) -> &DictInfo {
        &self.info
    }

    fn reader(&self) -> Result<Box<dyn DictReader + '_>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        Ok(Box::new(RedbReader {
            terms: db!(txn.open_table(TERMS))?,
            by_expression: db!(txn.open_multimap_table(BY_EXPRESSION))?,
            by_reading: db!(txn.open_multimap_table(BY_READING))?,
            filter: &self.filter,
            term_meta: db!(txn.open_multimap_table(TERM_META))?,
            kanji: db!(txn.open_table(KANJI))?,
            kanji_meta: db!(txn.open_multimap_table(KANJI_META))?,
        }))
    }

    fn tag(&self, name: &str) -> Result<Option<Tag>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let table = db!(txn.open_table(TAGS))?;
        db!(table.get(name))?.map(|v| decode(v.value())).transpose()
    }

    fn media(&self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let txn = db!(self.db.begin_read())?;
        let table = db!(txn.open_table(MEDIA))?;
        Ok(db!(table.get(media_key(path).as_str()))?.map(|v| v.value().to_vec()))
    }
}

/// The tables of one read transaction (they keep its snapshot alive).
struct RedbReader<'a> {
    terms: ReadOnlyTable<u64, &'static [u8]>,
    by_expression: ReadOnlyMultimapTable<&'static str, &'static [u8]>,
    by_reading: ReadOnlyMultimapTable<&'static str, &'static [u8]>,
    filter: &'a Bloom,
    term_meta: ReadOnlyMultimapTable<&'static str, &'static [u8]>,
    kanji: ReadOnlyTable<&'static str, &'static [u8]>,
    kanji_meta: ReadOnlyMultimapTable<&'static str, &'static [u8]>,
}

impl DictReader for RedbReader<'_> {
    fn heads(&self, field: Field, text: &str) -> Result<Vec<TermHead>, StoreError> {
        if !self.filter.may_contain(text) {
            return Ok(Vec::new());
        }
        let index = match field {
            Field::Expression => &self.by_expression,
            Field::Reading => &self.by_reading,
        };
        let mut out: Vec<TermHead> = Vec::new();
        for head in db!(index.get(text))? {
            out.push(decode(db!(head)?.value())?);
        }
        out.sort_by_key(|h| h.id);
        Ok(out)
    }

    fn term(&self, id: u64) -> Result<Option<StoredTerm>, StoreError> {
        db!(self.terms.get(id))?
            .map(|v| decode(v.value()))
            .transpose()
    }

    fn term_meta(&self, text: &str) -> Result<Vec<Meta>, StoreError> {
        meta(&self.term_meta, text)
    }

    fn kanji(&self, character: &str) -> Result<Option<Kanji>, StoreError> {
        db!(self.kanji.get(character))?
            .map(|v| decode(v.value()))
            .transpose()
    }

    fn kanji_meta(&self, character: &str) -> Result<Vec<Meta>, StoreError> {
        meta(&self.kanji_meta, character)
    }
}

fn meta(
    table: &ReadOnlyMultimapTable<&'static str, &'static [u8]>,
    key: &str,
) -> Result<Vec<Meta>, StoreError> {
    let mut out = Vec::new();
    for v in db!(table.get(key))? {
        let v = db!(v)?;
        // after the sequence number that keeps the bank order
        out.push(decode(v.value().get(4..).unwrap_or_default())?);
    }
    Ok(out)
}

fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>, StoreError> {
    Ok(postcard::to_allocvec(v)?)
}

/// Index rows written by [`StoreWriter::finish`] between two looks at the
/// cancel flag.
const ROWS_PER_CHECK: usize = 4096;

/// Writes a new dictionary file in one transaction (DIC-013: the import
/// commits only at the end, and the folder is renamed only after that).
pub struct StoreWriter {
    db: Database,
    txn: Option<WriteTransaction>,
    next_id: u64,
    /// Meta values start with it: multimap values sort by bytes, and the
    /// records of one headword must keep their bank order.
    next_meta: u32,
    by_expression: Vec<(String, Vec<u8>)>,
    by_reading: Vec<(String, Vec<u8>)>,
    counts: Counts,
}

impl std::fmt::Debug for StoreWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreWriter")
            .field("counts", &self.counts)
            .finish()
    }
}

impl StoreWriter {
    /// Creates the file at `path`.
    pub fn create(path: &Path) -> Result<Self, StoreError> {
        let db = db!(Database::create(path))?;
        let txn = db!(db.begin_write())?;
        Ok(Self {
            db,
            txn: Some(txn),
            next_id: 0,
            next_meta: 0,
            by_expression: Vec::new(),
            by_reading: Vec::new(),
            counts: Counts::default(),
        })
    }

    fn txn(&self) -> Result<&WriteTransaction, StoreError> {
        self.txn
            .as_ref()
            .ok_or_else(|| StoreError::Glossary("writer already finished".into()))
    }

    /// Adds terms.
    pub fn add_terms(&mut self, terms: Vec<Term>) -> Result<(), StoreError> {
        let mut rows = Vec::with_capacity(terms.len());
        for t in terms {
            let id = self.next_id;
            self.next_id += 1;
            if !t.rules.trim().is_empty() {
                self.counts.with_rules += 1;
            }
            let record = StoredTerm::new(id, t);
            let head = encode(&TermHead::of(&record))?;
            if !record.reading.is_empty() && record.reading != record.expression {
                self.by_reading.push((record.reading.clone(), head.clone()));
            }
            self.by_expression.push((record.expression.clone(), head));
            rows.push((id, encode(&record)?));
        }
        {
            let mut table = db!(self.txn()?.open_table(TERMS))?;
            for (id, bytes) in &rows {
                db!(table.insert(*id, bytes.as_slice()))?;
            }
        }
        self.counts.terms += rows.len() as u64;
        Ok(())
    }

    /// Adds term meta (`kanji = false`) or kanji meta.
    pub fn add_meta(&mut self, meta: Vec<(String, Meta)>, kanji: bool) -> Result<(), StoreError> {
        let n = meta.len() as u64;
        let mut seq = self.next_meta;
        {
            let mut table =
                db!(self
                    .txn()?
                    .open_multimap_table(if kanji { KANJI_META } else { TERM_META }))?;
            for (key, m) in &meta {
                let mut value = seq.to_be_bytes().to_vec();
                seq = seq.wrapping_add(1);
                value.extend(encode(m)?);
                db!(table.insert(key.as_str(), value.as_slice()))?;
            }
        }
        self.next_meta = seq;
        if kanji {
            self.counts.kanji_meta += n;
        } else {
            self.counts.term_meta += n;
            for (_, m) in &meta {
                match m {
                    Meta::Frequency(_) => self.counts.frequencies += 1,
                    Meta::Pitch { .. } | Meta::Ipa { .. } => self.counts.pitches += 1,
                }
            }
        }
        Ok(())
    }

    /// Adds kanji entries.
    pub fn add_kanji(&mut self, kanji: Vec<Kanji>) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(KANJI))?;
            for k in &kanji {
                db!(table.insert(k.character.as_str(), encode(k)?.as_slice()))?;
            }
        }
        self.counts.kanji += kanji.len() as u64;
        Ok(())
    }

    /// Adds tags (DIC-002).
    pub fn add_tags(&mut self, tags: Vec<Tag>) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(TAGS))?;
            for t in &tags {
                db!(table.insert(t.name.as_str(), encode(t)?.as_slice()))?;
            }
        }
        self.counts.tags += tags.len() as u64;
        Ok(())
    }

    /// Adds a media file.
    pub fn add_media(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        {
            let mut table = db!(self.txn()?.open_table(MEDIA))?;
            db!(table.insert(media_key(path).as_str(), bytes))?;
        }
        self.counts.media += 1;
        Ok(())
    }

    /// The counts so far.
    pub fn counts(&self) -> Counts {
        self.counts
    }

    /// Writes the indexes (sorted by key, as fast as redb inserts) and the
    /// info, commits and compacts. With a JMdict-sized dictionary this takes
    /// seconds, so `cancel` is looked at every [`ROWS_PER_CHECK`] rows and
    /// before the commit and the compaction (DIC-010): `Ok(None)` when it
    /// stopped, and the file is then of no use.
    pub fn finish(
        mut self,
        index: Index,
        kinds: Kinds,
        styles: String,
        cancel: &Cancel,
    ) -> Result<Option<DictInfo>, StoreError> {
        let stopped = |n: usize| n.is_multiple_of(ROWS_PER_CHECK) && cancel.is_cancelled();
        let txn = self
            .txn
            .take()
            .ok_or_else(|| StoreError::Glossary("writer already finished".into()))?;
        let mut filter = Bloom::with_capacity(self.by_expression.len() + self.by_reading.len());
        for (n, (key, _)) in self
            .by_expression
            .iter()
            .chain(&self.by_reading)
            .enumerate()
        {
            if stopped(n) {
                return Ok(None);
            }
            filter.insert(key);
        }
        for (def, mut pairs) in [
            (BY_EXPRESSION, std::mem::take(&mut self.by_expression)),
            (BY_READING, std::mem::take(&mut self.by_reading)),
        ] {
            pairs.sort();
            let mut table = db!(txn.open_multimap_table(def))?;
            for (n, (key, head)) in pairs.iter().enumerate() {
                if stopped(n) {
                    return Ok(None);
                }
                db!(table.insert(key.as_str(), head.as_slice()))?;
            }
        }
        // every table exists, even empty, so readers can open them
        db!(txn.open_table(TERMS))?;
        db!(txn.open_multimap_table(TERM_META))?;
        db!(txn.open_table(KANJI))?;
        db!(txn.open_multimap_table(KANJI_META))?;
        db!(txn.open_table(TAGS))?;
        db!(txn.open_table(MEDIA))?;
        let info = DictInfo {
            index,
            kinds,
            counts: self.counts,
            styles,
        };
        {
            let mut table = db!(txn.open_table(INFO))?;
            db!(table.insert("schema", encode(&SCHEMA)?.as_slice()))?;
            db!(table.insert("info", encode(&info)?.as_slice()))?;
            db!(table.insert("filter", filter.to_bytes().as_slice()))?;
        }
        // the commit and the compaction cannot be interrupted
        if cancel.is_cancelled() {
            return Ok(None);
        }
        db!(txn.commit())?;
        if cancel.is_cancelled() {
            return Ok(None);
        }
        // the file shrinks by ~40% (ADR 0002)
        let mut db = self.db;
        db!(db.compact())?;
        Ok(Some(info))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yomitan::{Frequency, FrequencyMode};

    fn index() -> Index {
        Index {
            title: "t".into(),
            revision: "1".into(),
            format: 3,
            sequenced: false,
            frequency_mode: FrequencyMode::RankBased,
            source_language: None,
            target_language: None,
            author: None,
            attribution: None,
            description: None,
            url: None,
        }
    }

    fn term(expression: &str, reading: &str) -> Term {
        Term {
            expression: expression.into(),
            reading: reading.into(),
            definition_tags: "v1".into(),
            rules: "v1".into(),
            score: 1.0,
            glossary: r#"["to eat"]"#.into(),
            sequence: 7,
            term_tags: String::new(),
        }
    }

    #[test]
    fn written_entries_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dict.redb");
        let mut w = StoreWriter::create(&path).unwrap();
        w.add_terms(vec![
            term("食べる", "たべる"),
            term("喰べる", "たべる"),
            term("たべる", "たべる"),
        ])
        .unwrap();
        w.add_meta(
            vec![(
                "食べる".into(),
                Meta::Frequency(Frequency {
                    reading: None,
                    value: 100,
                    display: None,
                }),
            )],
            false,
        )
        .unwrap();
        w.add_media("\\img/a.png", &[1, 2, 3]).unwrap();
        let info = w
            .finish(
                index(),
                Kinds::default(),
                "a { color: red }".into(),
                &Cancel::new(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(info.counts.terms, 3);

        let s = RedbStore::open(&path).unwrap();
        assert_eq!(s.info().styles, "a { color: red }");
        let by_expr = s.terms(Field::Expression, "食べる").unwrap();
        assert_eq!(by_expr.len(), 1);
        assert_eq!(by_expr[0].glossary().unwrap(), r#"["to eat"]"#);
        assert_eq!(by_expr[0].glossary_len(), r#"["to eat"]"#.len());
        assert_eq!(by_expr[0].sequence, 7);
        // the reading index holds only readings that differ from the headword
        assert_eq!(s.terms(Field::Reading, "たべる").unwrap().len(), 2);
        assert_eq!(s.term_meta("食べる").unwrap().len(), 1);
        assert_eq!(s.media("img/a.png").unwrap(), Some(vec![1, 2, 3]));
        assert!(s.kanji("食").unwrap().is_none());
    }

    // T-DIC-010
    #[test]
    fn a_cancelled_finish_commits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dict.redb");
        let mut w = StoreWriter::create(&path).unwrap();
        w.add_terms(vec![term("食べる", "たべる")]).unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        let finished = w
            .finish(index(), Kinds::default(), String::new(), &cancel)
            .unwrap();
        assert_eq!(finished, None);
        assert!(RedbStore::open(&path).is_err(), "nothing was committed");
    }
}
