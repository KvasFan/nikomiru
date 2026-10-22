// SPDX-License-Identifier: GPL-3.0-only

//! Conformance with Fushi's importer (TEST §5): the cases of
//! `tests/conformance/import.json`, from the import-robustness tests of
//! fushidicts at commit `ac8034e`. Each case is a zip written field by field,
//! so it can hold what no zip library writes (raw names, ZIP64 sentinels,
//! broken deflate streams, lying sizes), imported like any Yomitan zip. A
//! failing case shows its id and provenance.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use nikomiru_core::{Cancel, Localized};
use nikomiru_dict::import::{DB_FILE, ImportError, Imported, import};
use nikomiru_dict::store::{DictStore, Field, RedbStore};
use serde_json::{Value, json};

/// A zip entry as written.
struct Entry {
    name: Vec<u8>,
    /// The content, uncompressed.
    data: Vec<u8>,
    /// What the archive holds for it.
    stored: Vec<u8>,
    method: u16,
    declared: u32,
    flags: u16,
    extra: Vec<u8>,
    /// Central directory fields written as ZIP64 sentinels.
    zip64: Vec<String>,
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// Fushi's noise bytes (`import_memory_bound_test.cpp`): xorshift64.
fn noise(spec: &Value) -> Vec<u8> {
    let size = spec["size"].as_u64().unwrap() as usize;
    let mut x = spec["seed"]
        .as_u64()
        .unwrap()
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(1);
    (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn template(spec: &Value) -> Vec<u8> {
    let text = spec["text"].as_str().unwrap();
    let count = spec["count"].as_u64().unwrap() as usize;
    let separator = spec["separator"].as_str().unwrap_or("");
    let mut out = String::from(spec["prefix"].as_str().unwrap_or(""));
    if !text.contains("{i}") && separator.is_empty() {
        out.push_str(&text.repeat(count));
    } else {
        for i in 0..count {
            if i > 0 {
                out.push_str(separator);
            }
            out.push_str(&text.replace("{i}", &i.to_string()));
        }
    }
    out.push_str(spec["suffix"].as_str().unwrap_or(""));
    out.into_bytes()
}

fn content(e: &Value) -> Vec<u8> {
    if let Some(v) = e.get("json") {
        serde_json::to_vec(v).unwrap()
    } else if let Some(t) = e.get("text") {
        t.as_str().unwrap().as_bytes().to_vec()
    } else if let Some(h) = e.get("hex") {
        hex(h.as_str().unwrap())
    } else if let Some(n) = e.get("noise") {
        noise(n)
    } else if let Some(t) = e.get("template") {
        template(t)
    } else {
        panic!("no content in {e}")
    }
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn entry(e: &Value) -> Entry {
    let name = match e.get("name_hex") {
        Some(h) => hex(h.as_str().unwrap()),
        None => e["name"].as_str().unwrap().as_bytes().to_vec(),
    };
    let data = content(e);
    let deflated = e["method"] == "deflate";
    let mut stored = match e.get("compressed") {
        Some(c) if c.get("noise").is_some() => noise(&c["noise"]),
        Some(c) => hex(c["hex"].as_str().unwrap()),
        None if deflated => deflate(&data),
        None => data.clone(),
    };
    match e["corrupt"].as_str() {
        Some("half") => stored.truncate(stored.len() / 2),
        Some("first-64") => stored.truncate(64),
        Some("scramble") => stored
            .iter_mut()
            .for_each(|b| *b = (*b ^ 0xA5).wrapping_add(0x13)),
        Some("invert") => stored.iter_mut().for_each(|b| *b = !*b),
        Some(other) => panic!("unknown corruption {other}"),
        None => {}
    }
    Entry {
        declared: e["declared_size"]
            .as_u64()
            .map_or(data.len() as u32, |n| n as u32),
        method: if deflated { 8 } else { 0 },
        flags: e["flags"].as_u64().unwrap_or(0) as u16,
        extra: e["extra_hex"].as_str().map(hex).unwrap_or_default(),
        zip64: e["zip64"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| f.as_str().unwrap().to_string())
            .collect(),
        name,
        data,
        stored,
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    crc.sum()
}

/// A zip written field by field (APPNOTE 4.3), with an end of central
/// directory record and no ZIP64 one.
fn write_zip(entries: &[Entry]) -> Vec<u8> {
    const SENTINEL: u32 = 0xFFFF_FFFF;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        let crc = crc32(&e.data);
        let compressed = e.stored.len() as u32;
        let version: u16 = if e.zip64.is_empty() { 20 } else { 45 };
        let has = |field: &str| e.zip64.iter().any(|f| f == field);
        let mut zip64 = Vec::new();
        for (field, value) in [
            ("size", u64::from(e.declared)),
            ("compressed_size", u64::from(compressed)),
            ("offset", u64::from(offset)),
        ] {
            if has(field) {
                zip64.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut central_extra = e.extra.clone();
        if !zip64.is_empty() {
            central_extra.extend_from_slice(&1u16.to_le_bytes());
            central_extra.extend_from_slice(&(zip64.len() as u16).to_le_bytes());
            central_extra.extend_from_slice(&zip64);
        }
        let sentinel_or = |field: &str, value: u32| if has(field) { SENTINEL } else { value };

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [version, e.flags, e.method, 0, 0x21] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, compressed, e.declared] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&(e.extra.len() as u16).to_le_bytes());
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.extra);
        out.extend_from_slice(&e.stored);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [version, version, e.flags, e.method, 0, 0x21] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [
            crc,
            sentinel_or("compressed_size", compressed),
            sentinel_or("size", e.declared),
        ] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.name.len() as u16, central_extra.len() as u16, 0, 0, 0] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&sentinel_or("offset", offset).to_le_bytes());
        central.extend_from_slice(&e.name);
        central.extend_from_slice(&central_extra);
    }
    let (cd_offset, cd_size) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    let n = entries.len() as u16;
    for v in [0, 0, n, n] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The kind of an import error, as the conventions name it.
fn kind(e: &ImportError) -> String {
    match e {
        ImportError::Unsafe { reason, .. } => format!("unsafe:{reason:?}"),
        other => other
            .message_id()
            .trim_start_matches("error-dict-")
            .to_string(),
    }
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

fn damage(path: &Path, changes: &[Value]) {
    let mut bytes = std::fs::read(path).unwrap();
    for change in changes {
        if let Some(n) = change["truncate"].as_u64() {
            bytes.truncate(n as usize);
        }
        if let Some(at) = change["at"].as_u64() {
            let patch = hex(change["hex"].as_str().unwrap());
            let at = at as usize;
            bytes[at..at + patch.len()].copy_from_slice(&patch);
        }
    }
    std::fs::write(path, bytes).unwrap();
}

/// Opens a damaged dictionary file and looks `queries` up: it must return,
/// with results or errors, without a panic and within 30 s.
fn survives(path: &Path, queries: Vec<String>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        if let Ok(store) = RedbStore::open(&path) {
            for q in &queries {
                let _ = store.terms(Field::Expression, q);
            }
        }
        let _ = tx.send(());
    });
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(()) => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("the damaged file hangs".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("the damaged file panics".into()),
    }
}

fn kinds(imported: &Imported) -> Value {
    let k = imported.info.kinds;
    let all = [
        (k.terms, "term"),
        (k.kanji, "kanji"),
        (k.frequency, "frequency"),
        (k.pitch, "pitch"),
    ];
    json!(
        all.iter()
            .filter(|(on, _)| *on)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
    )
}

fn check_imported(imported: &Imported, entries: &[Entry], case: &Value) -> Result<(), String> {
    let expected = &case["expected"];
    let info = &imported.info;
    if let Some(title) = expected["title"].as_str()
        && info.index.title != title
    {
        return Err(format!("title {:?} instead of {title:?}", info.index.title));
    }
    if let Some(want) = expected.get("kinds")
        && kinds(imported) != *want
    {
        return Err(format!("kinds {} instead of {want}", kinds(imported)));
    }
    let counts = serde_json::to_value(info.counts).unwrap();
    for (name, want) in expected["counts"].as_object().into_iter().flatten() {
        if counts.get(name) != Some(want) {
            return Err(format!("{name} {} instead of {want}", counts[name]));
        }
    }
    if let Some(styles) = expected["styles"].as_str()
        && info.styles != styles
    {
        return Err(format!("styles {:?} instead of {styles:?}", info.styles));
    }
    let db = imported.dir.join(DB_FILE);
    let store = RedbStore::open(&db).map_err(|e| e.to_string())?;
    for (path, want) in expected["media"].as_object().into_iter().flatten() {
        let got = store.media(path).map_err(|e| e.to_string())?;
        let want = match want {
            Value::Null => None,
            Value::String(h) => Some(hex(h)),
            Value::Object(o) => {
                let name = o["entry"].as_str().unwrap().as_bytes();
                Some(
                    entries
                        .iter()
                        .find(|e| e.name == name)
                        .unwrap()
                        .data
                        .clone(),
                )
            }
            other => panic!("media {other}"),
        };
        if got != want {
            let size = |b: &Option<Vec<u8>>| b.as_ref().map(Vec::len);
            return Err(format!(
                "media {path}: {:?} bytes instead of {:?}",
                size(&got),
                size(&want)
            ));
        }
    }
    for (expression, want) in expected["terms"].as_object().into_iter().flatten() {
        let got: Vec<Value> = store
            .terms(Field::Expression, expression)
            .map_err(|e| e.to_string())?
            .iter()
            .map(|t| serde_json::from_str(&t.glossary().unwrap()).unwrap())
            .collect();
        if json!(got) != *want {
            return Err(format!("{expression}: {got:?} instead of {want}"));
        }
    }
    if let Some(changes) = case["damage"].as_array() {
        drop(store);
        damage(&db, changes);
        let queries = expected["survives"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|q| q.as_str().unwrap().to_string())
            .collect();
        survives(&db, queries)?;
    }
    Ok(())
}

fn import_case(dir: &Path, case: &Value) -> Result<(), String> {
    let id = case["id"].as_str().unwrap();
    let file = case["zip"]["file"]
        .as_str()
        .map_or_else(|| format!("{id}.zip"), str::to_string);
    let zip = dir.join(file);
    std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
    let entries: Vec<Entry> = case["zip"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(entry)
        .collect();
    std::fs::write(&zip, write_zip(&entries)).unwrap();
    let dicts = dir.join(case["dicts_dir"].as_str().unwrap_or("dicts"));
    match (
        import(&zip, &dicts, &Cancel::new(), |_| {}),
        case["expected"]["error"].as_str(),
    ) {
        (Err(e), Some(want)) => {
            let got = kind(&e);
            if want != "any" && got != want {
                return Err(format!("error {got} ({e}) instead of {want}"));
            }
            let left = leftovers(&dicts);
            if !left.is_empty() {
                return Err(format!("the failed import left {left:?}"));
            }
            Ok(())
        }
        (Err(e), None) => Err(format!("the import failed: {} ({e})", kind(&e))),
        (Ok(_), Some(want)) => Err(format!("imported, error {want} expected")),
        (Ok(imported), None) => check_imported(&imported, &entries, case),
    }
}

// T-DIC-001, T-DIC-011, T-DIC-013, T-DIC-015, T-SEC-002
#[test]
fn fushi_import_vectors() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/import.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let (mut run, mut known) = (0, 0);
    for (n, case) in doc["cases"].as_array().unwrap().iter().enumerate() {
        if case["skip"] == Value::Bool(true) {
            continue;
        }
        run += 1;
        let result = import_case(&tmp.path().join(n.to_string()), case);
        let label = format!(
            "{} ({} › {})",
            case["id"].as_str().unwrap(),
            case["provenance"]["file"].as_str().unwrap_or("?"),
            case["provenance"]["test"].as_str().unwrap_or("?"),
        );
        match (result, case["known_failure"].is_object()) {
            (Ok(()), false) => {}
            (Err(e), false) => failures.push(format!("{label}: {e}")),
            (Err(e), true) => {
                known += 1;
                eprintln!("known failure {label}: {e}");
            }
            (Ok(()), true) => {
                failures.push(format!("{label}: passes now, remove its known_failure"))
            }
        }
    }
    eprintln!("{run} import cases, {known} known failures");
    assert!(run >= 35, "only {run} cases ran");
    assert!(
        failures.is_empty(),
        "{} of {run} cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
