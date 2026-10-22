// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan / Yomichan dictionary formats (DIC-001…DIC-003, DIC-015):
//! `index.json`, term, term meta, kanji, kanji meta and tag banks. Rows are
//! read as raw JSON slices, so a glossary is stored exactly as the file has it
//! and odd rows (format 1, decimal scores, missing fields) do not stop an
//! import.

use std::marker::PhantomData;

use nikomiru_core::Cancel;
use serde::de::{self, Deserializer as _, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

/// A row that is not what the format says.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FormatError(pub String);

fn err(message: impl Into<String>) -> FormatError {
    FormatError(message.into())
}

/// How frequency values sort (DIC-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FrequencyMode {
    /// Lower is more frequent (a rank): the default.
    #[default]
    RankBased,
    /// Higher is more frequent (an occurrence count).
    OccurrenceBased,
}

/// `index.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Index {
    /// Title.
    pub title: String,
    /// Revision.
    pub revision: String,
    /// Format version (1, 2 or 3).
    pub format: u32,
    /// Whether `sequence` numbers are meaningful.
    pub sequenced: bool,
    /// How frequencies sort.
    pub frequency_mode: FrequencyMode,
    /// Language of the headwords, if declared (LKP-012).
    pub source_language: Option<String>,
    /// Language of the definitions, if declared.
    pub target_language: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Attribution text.
    pub attribution: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Home page.
    pub url: Option<String>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Parses `index.json`. Bytes that are not UTF-8 become U+FFFD, as in Fushi:
/// a damaged title must not stop the import (BUG-2952).
pub fn parse_index(bytes: &[u8]) -> Result<Index, FormatError> {
    let json = String::from_utf8_lossy(bytes);
    let v: Value = serde_json::from_str(&json).map_err(|e| err(format!("index.json: {e}")))?;
    let title = text(&v, "title").ok_or_else(|| err("index.json has no title"))?;
    let revision = match v.get("revision") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err(err("index.json has no revision")),
    };
    let format = v
        .get("format")
        .or_else(|| v.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(3) as u32;
    Ok(Index {
        title,
        revision,
        format,
        sequenced: v.get("sequenced").and_then(Value::as_bool).unwrap_or(false),
        frequency_mode: match v.get("frequencyMode").and_then(Value::as_str) {
            Some("occurrence-based") => FrequencyMode::OccurrenceBased,
            _ => FrequencyMode::RankBased,
        },
        source_language: text(&v, "sourceLanguage"),
        target_language: text(&v, "targetLanguage"),
        author: text(&v, "author"),
        attribution: text(&v, "attribution"),
        description: text(&v, "description"),
        url: text(&v, "url"),
    })
}

/// Glossaries this short may be a redirect (LKP-032).
pub const MAX_REDIRECT_BYTES: usize = 1024;

/// `[formOf, [rules…]]`: a glossary item that points to another headword.
pub type Redirect = (String, Vec<String>);

/// The redirects of a glossary made only of `[formOf, [rules…]]` items
/// (LKP-032, at most [`MAX_REDIRECT_BYTES`]); `None` for anything else, a
/// real definition next to a redirect label included (BUG-2566).
pub fn parse_redirects(glossary: &str) -> Option<Vec<Redirect>> {
    if glossary.len() > MAX_REDIRECT_BYTES {
        return None;
    }
    let mut rest = glossary.trim_start();
    for _ in 0..2 {
        rest = rest.strip_prefix('[')?.trim_start();
    }
    let items: Vec<Redirect> = serde_json::from_str(glossary).ok()?;
    (!items.is_empty() && items.iter().all(|(form, _)| !form.is_empty())).then_some(items)
}

/// A term entry. The glossary is the raw JSON array.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// Headword.
    pub expression: String,
    /// Reading (the headword when empty in the file).
    pub reading: String,
    /// Definition tags, space separated.
    pub definition_tags: String,
    /// Deinflection rules, space separated (`v5k vt`).
    pub rules: String,
    /// Popularity score.
    pub score: f64,
    /// The glossary, as JSON.
    pub glossary: String,
    /// Sequence number (DIC-002).
    pub sequence: i64,
    /// Term tags, space separated.
    pub term_tags: String,
}

/// Rows of a bank read between two looks at the cancel flag.
const ROWS_PER_CHECK: usize = 1024;

/// What a row becomes: `None` skips it, an error stops the bank.
type Convert<T> = Result<Option<T>, FormatError>;

/// Reads the rows of a bank one at a time and converts each as it comes.
struct BankReader<'a, R, F> {
    convert: F,
    cancel: &'a Cancel,
    failed: &'a mut Option<FormatError>,
    row: PhantomData<R>,
}

impl<'de, R, T, F> Visitor<'de> for BankReader<'_, R, F>
where
    R: Deserialize<'de>,
    F: FnMut(usize, R) -> Convert<T>,
{
    type Value = Vec<T>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an array of rows")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<Vec<T>, A::Error> {
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(row) = seq.next_element()? {
            match (self.convert)(i, row) {
                Ok(Some(v)) => out.push(v),
                Ok(None) => {}
                Err(e) => {
                    *self.failed = Some(e);
                    return Err(de::Error::custom("invalid row"));
                }
            }
            i += 1;
            if i.is_multiple_of(ROWS_PER_CHECK) && self.cancel.is_cancelled() {
                return Err(de::Error::custom("cancelled"));
            }
        }
        Ok(out)
    }
}

/// The rows of a bank, each converted by `convert` as soon as it is read.
/// A large bank (a whole JMdict in one file) takes seconds to read, so
/// `cancel` is looked at every [`ROWS_PER_CHECK`] rows: `Ok(None)` when it
/// stopped the read (DIC-010).
fn rows<'a, R: Deserialize<'a>, T>(
    bytes: &'a [u8],
    cancel: &Cancel,
    convert: impl FnMut(usize, R) -> Convert<T>,
) -> Result<Option<Vec<T>>, FormatError> {
    let mut failed = None;
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let read = (&mut reader).deserialize_seq(BankReader {
        convert,
        cancel,
        failed: &mut failed,
        row: PhantomData,
    });
    if let Some(e) = failed {
        return Err(e);
    }
    match read {
        Ok(rows) => {
            reader.end().map_err(|e| err(e.to_string()))?;
            Ok(Some(rows))
        }
        Err(_) if cancel.is_cancelled() => Ok(None),
        Err(e) => Err(err(e.to_string())),
    }
}

fn raw_str(raw: Option<&&RawValue>) -> String {
    raw.and_then(|r| {
        serde_json::from_str::<Option<String>>(r.get())
            .ok()
            .flatten()
    })
    .unwrap_or_default()
}

fn raw_f64(raw: Option<&&RawValue>) -> f64 {
    raw.and_then(|r| serde_json::from_str::<f64>(r.get()).ok())
        .unwrap_or(0.0)
}

/// Parses a `term_bank_*.json`; `None` when `cancel` stopped it. Format 1
/// puts the glossary strings after the score instead of an array.
pub fn parse_terms(
    bytes: &[u8],
    format: u32,
    cancel: &Cancel,
) -> Result<Option<Vec<Term>>, FormatError> {
    rows(bytes, cancel, |i, row: Vec<&RawValue>| {
        if row.len() < 5 {
            return Err(err(format!("term {i}: {} fields", row.len())));
        }
        let expression = raw_str(row.first());
        if expression.is_empty() {
            return Err(err(format!("term {i}: no expression")));
        }
        let reading = Some(raw_str(row.get(1)))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| expression.clone());
        let glossary = if format == 1 {
            let items: Vec<&str> = row[5..].iter().map(|r| r.get()).collect();
            format!("[{}]", items.join(","))
        } else {
            row.get(5)
                .map_or_else(|| "[]".to_string(), |r| r.get().to_string())
        };
        let (sequence, term_tags) = if format == 1 {
            (0, String::new())
        } else {
            (raw_f64(row.get(6)) as i64, raw_str(row.get(7)))
        };
        Ok(Some(Term {
            expression,
            reading,
            definition_tags: raw_str(row.get(2)),
            rules: raw_str(row.get(3)),
            score: raw_f64(row.get(4)),
            glossary,
            sequence,
            term_tags,
        }))
    })
}

/// A frequency value (DIC-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frequency {
    /// The reading it applies to, when the dictionary says.
    pub reading: Option<String>,
    /// The numeric value; for strings the leading integer, -1 if none.
    pub value: i64,
    /// What to show, if not the number.
    pub display: Option<String>,
}

/// A pitch accent position: a downstep mora, or an H/L pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PitchPosition {
    /// Mora after which the pitch falls (0 = heiban).
    Downstep(u32),
    /// A pattern such as `LHHL`.
    Pattern(String),
}

/// One pitch accent of a reading (LKP-041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pitch {
    /// The position.
    pub position: PitchPosition,
    /// Nasal morae (1-based).
    pub nasal: Vec<u32>,
    /// Devoiced morae (1-based).
    pub devoice: Vec<u32>,
    /// Tags (`v5`, `adj-i`…).
    pub tags: Vec<String>,
}

/// An IPA transcription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ipa {
    /// The transcription.
    pub ipa: String,
    /// Tags.
    pub tags: Vec<String>,
}

/// A term meta entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Meta {
    /// A frequency.
    Frequency(Frequency),
    /// Pitch accents of a reading.
    Pitch {
        /// The reading.
        reading: String,
        /// Its accents.
        pitches: Vec<Pitch>,
    },
    /// IPA transcriptions of a reading.
    Ipa {
        /// The reading.
        reading: String,
        /// The transcriptions.
        transcriptions: Vec<Ipa>,
    },
}

impl Meta {
    /// `freq`, `pitch` or `ipa`.
    pub fn mode(&self) -> &'static str {
        match self {
            Meta::Frequency(_) => "freq",
            Meta::Pitch { .. } => "pitch",
            Meta::Ipa { .. } => "ipa",
        }
    }
}

/// A frequency in any of its shapes: number, string, `{value, displayValue}`.
fn frequency_value(v: &Value) -> Option<(i64, Option<String>)> {
    match v {
        Value::Number(n) => n.as_f64().map(|f| (f as i64, None)),
        Value::String(s) => {
            let digits: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
            Some((digits.parse().unwrap_or(-1), Some(s.clone())))
        }
        Value::Object(o) => {
            let (value, _) = frequency_value(o.get("value")?)?;
            let display = o
                .get("displayValue")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some((value, display))
        }
        _ => None,
    }
}

fn frequency(data: &Value) -> Option<Frequency> {
    if let Some(reading) = data.get("reading").and_then(Value::as_str)
        && let Some(f) = data.get("frequency")
    {
        let (value, display) = frequency_value(f)?;
        return Some(Frequency {
            reading: Some(reading.to_string()),
            value,
            display,
        });
    }
    let (value, display) = frequency_value(data)?;
    Some(Frequency {
        // a reading next to the value, `{reading, value}`, as Fushi reads it
        reading: data
            .get("reading")
            .and_then(Value::as_str)
            .map(str::to_string),
        value,
        display,
    })
}

fn numbers(v: Option<&Value>) -> Vec<u32> {
    match v {
        Some(Value::Number(n)) => n.as_u64().map(|n| vec![n as u32]).unwrap_or_default(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|n| n.as_u64().map(|n| n as u32))
            .collect(),
        _ => Vec::new(),
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_meta(mode: &str, data: &Value) -> Option<Meta> {
    match mode {
        "freq" => frequency(data).map(Meta::Frequency),
        "pitch" => {
            let reading = data
                .get("reading")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pitches = data
                .get("pitches")?
                .as_array()?
                .iter()
                .filter_map(|p| {
                    let position = match p.get("position")? {
                        Value::Number(n) => PitchPosition::Downstep(n.as_u64()? as u32),
                        Value::String(s) => PitchPosition::Pattern(s.clone()),
                        _ => return None,
                    };
                    Some(Pitch {
                        position,
                        nasal: numbers(p.get("nasal")),
                        devoice: numbers(p.get("devoice")),
                        tags: strings(p.get("tags")),
                    })
                })
                .collect();
            Some(Meta::Pitch { reading, pitches })
        }
        "ipa" => {
            let reading = data
                .get("reading")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let transcriptions = data
                .get("transcriptions")?
                .as_array()?
                .iter()
                .filter_map(|t| {
                    Some(Ipa {
                        ipa: t.get("ipa")?.as_str()?.to_string(),
                        tags: strings(t.get("tags")),
                    })
                })
                .collect();
            Some(Meta::Ipa {
                reading,
                transcriptions,
            })
        }
        _ => None,
    }
}

/// Parses a `term_meta_bank_*.json` or `kanji_meta_bank_*.json`: `(key, meta)`
/// pairs; unknown modes and malformed rows are skipped. `None` when `cancel`
/// stopped it.
pub fn parse_meta_bank(
    bytes: &[u8],
    cancel: &Cancel,
) -> Result<Option<Vec<(String, Meta)>>, FormatError> {
    fn pair(row: &Value) -> Option<(String, Meta)> {
        let key = row.get(0)?.as_str()?.to_string();
        let mode = row.get(1)?.as_str()?;
        Some((key, parse_meta(mode, row.get(2)?)?))
    }
    rows(bytes, cancel, |_, row: Value| Ok(pair(&row)))
}

/// A kanji entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kanji {
    /// The character.
    pub character: String,
    /// On readings, space separated.
    pub onyomi: String,
    /// Kun readings, space separated.
    pub kunyomi: String,
    /// Tags, space separated.
    pub tags: String,
    /// Meanings.
    pub meanings: Vec<String>,
    /// Statistics (strokes, grade, frequency…) as name → value.
    pub stats: Vec<(String, String)>,
}

/// Parses a `kanji_bank_*.json` (format 1 puts the meanings after the tags);
/// `None` when `cancel` stopped it. A malformed row (a number where a string
/// belongs, meanings that are not a list of strings, no character) is
/// skipped, as in Fushi: one bad row must not drop the bank (BUG-2952).
pub fn parse_kanji(
    bytes: &[u8],
    format: u32,
    cancel: &Cancel,
) -> Result<Option<Vec<Kanji>>, FormatError> {
    rows(bytes, cancel, |_, row: Value| Ok(kanji_row(&row, format)))
}

fn kanji_row(row: &Value, format: u32) -> Option<Kanji> {
    let a = row.as_array()?;
    // the four text fields are strings when present
    let s = |k: usize| match a.get(k) {
        None => Some(String::new()),
        Some(v) => v.as_str().map(str::to_string),
    };
    let character = s(0).filter(|c| !c.is_empty())?;
    let (meanings, stats) = if format == 1 {
        (
            a[4.min(a.len())..]
                .iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect(),
            Vec::new(),
        )
    } else {
        let meanings = match a.get(4) {
            None => Vec::new(),
            Some(m) => m
                .as_array()?
                .iter()
                .map(|m| m.as_str().map(str::to_string))
                .collect::<Option<_>>()?,
        };
        let stats = a
            .get(5)
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.as_str().map_or_else(|| v.to_string(), str::to_string),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        (meanings, stats)
    };
    Some(Kanji {
        character,
        onyomi: s(1)?,
        kunyomi: s(2)?,
        tags: s(3)?,
        meanings,
        stats,
    })
}

/// A tag (DIC-002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Name, as used in entries.
    pub name: String,
    /// Category (`partOfSpeech`, `frequent`, `name`…).
    pub category: String,
    /// Sorting order.
    pub order: i64,
    /// Explanation.
    pub notes: String,
    /// Score.
    pub score: f64,
}

/// Parses a `tag_bank_*.json`; `None` when `cancel` stopped it.
pub fn parse_tags(bytes: &[u8], cancel: &Cancel) -> Result<Option<Vec<Tag>>, FormatError> {
    fn tag(row: &Value) -> Option<Tag> {
        let a = row.as_array()?;
        let s = |k: usize| {
            a.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Some(Tag {
            name: Some(s(0)).filter(|n| !n.is_empty())?,
            category: s(1),
            order: a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as i64,
            notes: s(3),
            score: a.get(4).and_then(Value::as_f64).unwrap_or(0.0),
        })
    }
    rows(bytes, cancel, |_, row: Value| Ok(tag(&row)))
}

/// What a dictionary contains (DIC-015): every role its banks give it. A
/// term dictionary with a frequency bank also gives frequencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Kinds {
    /// Term entries.
    pub terms: bool,
    /// Kanji entries.
    pub kanji: bool,
    /// Frequencies (of terms or of kanji).
    pub frequency: bool,
    /// Pitch accent or IPA.
    pub pitch: bool,
}

/// Rows of each kind written by an import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rows {
    /// Term rows.
    pub terms: u64,
    /// Kanji rows.
    pub kanji: u64,
    /// Frequency rows of terms and kanji.
    pub frequencies: u64,
    /// Pitch and IPA rows.
    pub pitches: u64,
}

/// DIC-015: the roles of a dictionary from what it contains.
pub fn detect_kinds(rows: Rows) -> Kinds {
    Kinds {
        terms: rows.terms > 0,
        kanji: rows.kanji > 0,
        frequency: rows.frequencies > 0,
        pitch: rows.pitches > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows of a bank that was not cancelled.
    fn all<T>(parsed: Result<Option<Vec<T>>, FormatError>) -> Vec<T> {
        parsed.unwrap().expect("not cancelled")
    }

    #[test]
    fn index_fields() {
        let i = parse_index(r#"{"title":"Mini","revision":3,"format":3,"sequenced":true,"frequencyMode":"occurrence-based","sourceLanguage":"ja"}"#.as_bytes()).unwrap();
        assert_eq!(
            (i.title.as_str(), i.revision.as_str(), i.format),
            ("Mini", "3", 3)
        );
        assert_eq!(i.frequency_mode, FrequencyMode::OccurrenceBased);
        assert_eq!(i.source_language.as_deref(), Some("ja"));
        assert!(parse_index(r#"{"revision":"1"}"#.as_bytes()).is_err());
    }

    #[test]
    fn term_rows_in_both_formats() {
        let v3 = all(parse_terms(
            r#"[["食べる","たべる","v1 vt","v1",1.5,["to eat",{"type":"structured-content","content":"x"}],1358280,"P"],
                 ["猫","",null,"",0,["cat"],2,""]]"#.as_bytes(),
            3,
            &Cancel::new(),
        ));
        assert_eq!(v3[0].rules, "v1");
        assert_eq!(v3[0].score, 1.5);
        assert_eq!(v3[0].sequence, 1_358_280);
        assert_eq!(
            v3[0].glossary,
            r#"["to eat",{"type":"structured-content","content":"x"}]"#
        );
        assert_eq!(v3[1].reading, "猫", "an empty reading is the headword");
        let v1 = all(parse_terms(
            r#"[["犬","いぬ","n","",0,"dog","hound"]]"#.as_bytes(),
            1,
            &Cancel::new(),
        ));
        assert_eq!(v1[0].glossary, r#"["dog","hound"]"#);
        assert_eq!(
            parse_terms(
                r#"[["猫","",null,"",0,["cat"]],["x"]]"#.as_bytes(),
                3,
                &Cancel::new()
            ),
            Err(err("term 1: 1 fields"))
        );
        assert!(parse_terms(r#"{"x":1}"#.as_bytes(), 3, &Cancel::new()).is_err());
        assert!(
            parse_terms(
                r#"[["猫","",null,"",0,[]]] x"#.as_bytes(),
                3,
                &Cancel::new()
            )
            .is_err()
        );
    }

    // T-DIC-010
    #[test]
    fn a_cancel_stops_a_bank_at_the_next_check() {
        let bank = format!("[{}]", vec!["[1]"; 10 * ROWS_PER_CHECK].join(","));
        let cancel = Cancel::new();
        let mut read = 0;
        let stopped = rows(bank.as_bytes(), &cancel, |i, row: Vec<u32>| {
            read += 1;
            if i == ROWS_PER_CHECK + 10 {
                cancel.cancel();
            }
            Ok(Some(row))
        });
        assert_eq!(stopped, Ok(None));
        assert_eq!(read, 2 * ROWS_PER_CHECK, "the rest of the bank is not read");
        let whole = rows(bank.as_bytes(), &Cancel::new(), |_, row: Vec<u32>| {
            Ok(Some(row))
        });
        assert_eq!(all(whole).len(), 10 * ROWS_PER_CHECK);
        let terms = format!(
            "[{}]",
            vec![r#"["語","ご","n","",0,["word"],1,""]"#; ROWS_PER_CHECK].join(",")
        );
        assert_eq!(parse_terms(terms.as_bytes(), 3, &cancel), Ok(None));
    }

    // T-DIC-003
    #[test]
    fn frequencies_in_every_shape() {
        let m = all(parse_meta_bank(
            r#"[["a","freq",12],["b","freq","1500㋕"],["c","freq",{"value":7,"displayValue":"7★"}],
                 ["d","freq",{"reading":"でぃー","frequency":{"value":3}}],["e","weird",1]]"#
                .as_bytes(),
            &Cancel::new(),
        ));
        let f: Vec<(i64, Option<String>)> = m
            .iter()
            .filter_map(|(_, x)| match x {
                Meta::Frequency(f) => Some((f.value, f.display.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            f,
            [
                (12, None),
                (1500, Some("1500㋕".into())),
                (7, Some("7★".into())),
                (3, None)
            ]
        );
        assert_eq!(m.len(), 4, "unknown modes are skipped");
        let flat = all(parse_meta_bank(
            r#"[["猫","freq",{"reading":"ねこ","value":100}]]"#.as_bytes(),
            &Cancel::new(),
        ));
        assert!(
            matches!(&flat[0].1, Meta::Frequency(f) if f.reading.as_deref() == Some("ねこ") && f.value == 100),
            "{flat:?}"
        );
    }

    #[test]
    fn pitch_and_ipa() {
        let m = all(parse_meta_bank(
            r#"[["箸","pitch",{"reading":"はし","pitches":[{"position":1,"nasal":2,"devoice":[1],"tags":["n"]},{"position":"LHH"}]}],
                 ["箸","ipa",{"reading":"はし","transcriptions":[{"ipa":"ha̠ɕi","tags":[]}]}]]"#.as_bytes(),
            &Cancel::new(),
        ));
        let Meta::Pitch { pitches, .. } = &m[0].1 else {
            panic!()
        };
        assert_eq!(pitches[0].position, PitchPosition::Downstep(1));
        assert_eq!(
            (pitches[0].nasal.as_slice(), pitches[0].devoice.as_slice()),
            (&[2][..], &[1][..])
        );
        assert_eq!(pitches[1].position, PitchPosition::Pattern("LHH".into()));
        assert_eq!(m[1].1.mode(), "ipa");
    }

    #[test]
    fn kanji_and_tags() {
        let k = all(parse_kanji(
            r#"[["日","ニチ ジツ","ひ -び","jouyou",["day","sun"],{"strokes":"4","grade":"1"}]]"#
                .as_bytes(),
            3,
            &Cancel::new(),
        ));
        assert_eq!(k[0].meanings, ["day", "sun"]);
        assert!(
            k[0].stats
                .contains(&("strokes".to_string(), "4".to_string()))
        );
        let t = all(parse_tags(
            r#"[["v5k","partOfSpeech",-3,"Godan verb with 'ku' ending",0],[""]]"#.as_bytes(),
            &Cancel::new(),
        ));
        assert_eq!((t[0].category.as_str(), t[0].order), ("partOfSpeech", -3));
        assert_eq!(t.len(), 1, "a tag without a name is skipped");
    }

    #[test]
    fn a_malformed_kanji_row_or_title_does_not_stop_the_import() {
        let k = parse_kanji(
            r#"[["日","nichi","hi","jouyou",["day","","sun"],{"strokes":"4"}],
                [123,"","","",["number as character"],{}],
                ["x","","","","meanings-not-an-array",{}],
                ["𠮟","","","",["異体字",""],{}],
                ["y","","","",[],"stats-as-string"]]"#
                .as_bytes(),
            3,
            &Cancel::new(),
        );
        let k = all(k);
        let characters: Vec<&str> = k.iter().map(|k| k.character.as_str()).collect();
        assert_eq!(characters, ["日", "𠮟", "y"]);
        let index =
            parse_index(b"{\"title\":\"bad\xff\xfe title\",\"format\":3,\"revision\":\"t\"}")
                .unwrap();
        assert_eq!(index.title, "bad\u{fffd}\u{fffd} title");
    }

    // T-DIC-015
    #[test]
    fn kinds_are_detected() {
        let rows = |terms, kanji, frequencies, pitches| Rows {
            terms,
            kanji,
            frequencies,
            pitches,
        };
        let kinds = |terms, kanji, frequency, pitch| Kinds {
            terms,
            kanji,
            frequency,
            pitch,
        };
        assert_eq!(
            detect_kinds(rows(3, 0, 0, 0)),
            kinds(true, false, false, false)
        );
        assert_eq!(
            detect_kinds(rows(3, 1, 0, 0)),
            kinds(true, true, false, false)
        );
        assert_eq!(
            detect_kinds(rows(0, 0, 5, 0)),
            kinds(false, false, true, false)
        );
        assert_eq!(
            detect_kinds(rows(0, 0, 0, 2)),
            kinds(false, false, false, true)
        );
        assert_eq!(
            detect_kinds(rows(0, 4, 4, 0)),
            kinds(false, true, true, false),
            "kanji with their frequencies"
        );
        assert_eq!(
            detect_kinds(rows(9, 0, 1, 1)),
            kinds(true, false, true, true),
            "terms with meta"
        );
        assert_eq!(detect_kinds(Rows::default()), Kinds::default());
    }
}
