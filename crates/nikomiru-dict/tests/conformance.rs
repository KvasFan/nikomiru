// SPDX-License-Identifier: GPL-3.0-only

//! Conformance with Fushi's dictionary engine (TEST §5): the cases of
//! `tests/conformance/lookup.json`, extracted from Fushi's tests at commit
//! `ac8034e` and checked there against Fushi's own code. The dictionaries of
//! the file are imported like any Yomitan zip. A failing case shows its id and
//! provenance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use nikomiru_core::Cancel;
use nikomiru_dict::deinflect::Deinflector;
use nikomiru_dict::import::import;
use nikomiru_dict::lookup::{Dictionary, Entry, Lookup, Options, best_length};
use nikomiru_dict::store::{DictInfo, DictStore, RedbStore};
use nikomiru_dict::yomitan::{Frequency, PitchPosition};
use nikomiru_dict::{frequency, pitch, present};
use serde_json::{Value, json};

struct Env {
    tmp: tempfile::TempDir,
    sources: serde_json::Map<String, Value>,
    imported: HashMap<String, (Arc<RedbStore>, DictInfo)>,
}

impl Env {
    /// The dictionary with this id of the file, imported on first use; an
    /// import that fails fails the case.
    fn dictionary(&mut self, id: &str) -> Result<Dictionary, String> {
        if !self.imported.contains_key(id) {
            let source = &self.sources[id];
            let zip_path = self.tmp.path().join(format!("{id}.zip"));
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            for (key, file) in [
                ("index", "index.json"),
                ("term_bank", "term_bank_1.json"),
                ("term_meta_bank", "term_meta_bank_1.json"),
                ("kanji_bank", "kanji_bank_1.json"),
                ("kanji_meta_bank", "kanji_meta_bank_1.json"),
                ("tag_bank", "tag_bank_1.json"),
            ] {
                if let Some(content) = source.get(key) {
                    zip.start_file(file, options).unwrap();
                    zip.write_all(content.to_string().as_bytes()).unwrap();
                }
            }
            zip.finish().unwrap();
            let imported = import(
                &zip_path,
                &self.tmp.path().join("dicts"),
                &Cancel::new(),
                |_| {},
            )
            .map_err(|e| format!("importing {id}: {e}"))?;
            let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
            let info = store.info().clone();
            self.imported
                .insert(id.to_string(), (Arc::new(store), info));
        }
        let (store, info) = &self.imported[id];
        Ok(Dictionary {
            id: id.to_string(),
            name: info.index.title.clone(),
            store: Arc::clone(store) as Arc<dyn DictStore>,
        })
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// An entry as the file describes it.
fn describe(entry: &Entry, lookup: &Lookup) -> Value {
    let id = |i: usize| lookup.dictionaries()[i].id.clone();
    let shown: Vec<_> = entry
        .glossaries
        .iter()
        .map(|g| {
            (
                g,
                serde_json::from_str::<Value>(&g.term.glossary().unwrap()).unwrap(),
            )
        })
        .filter(|(g, json)| {
            !present::is_redirect_label(&g.term.definition_tags, &g.term.term_tags, json)
        })
        .collect();
    let frequencies: Vec<Value> = entry
        .frequencies
        .iter()
        .map(|f| json!({"dictionary": id(f.dictionary), "values": f.values.iter().map(frequency::shown).collect::<Vec<_>>()}))
        .collect();
    let pitches: Vec<Value> = entry
        .pitches
        .iter()
        .map(|p| {
            let positions: Vec<u32> = p
                .pitches
                .iter()
                .filter_map(|a| match &a.position {
                    PitchPosition::Downstep(n) => Some(*n),
                    PitchPosition::Pattern(_) => None,
                })
                .collect();
            let patterns: Vec<&str> = p
                .pitches
                .iter()
                .filter_map(|a| match &a.position {
                    PitchPosition::Pattern(s) => Some(s.as_str()),
                    PitchPosition::Downstep(_) => None,
                })
                .collect();
            let ipa: Vec<&str> = p.ipa.iter().map(|t| t.ipa.as_str()).collect();
            json!({"dictionary": id(p.dictionary), "positions": positions, "patterns": patterns, "transcriptions": ipa})
        })
        .collect();
    json!({
        "expression": entry.expression,
        "reading": entry.reading,
        "matched": entry.matched,
        "trace": entry.trace.iter().map(|s| &*s.name).collect::<Vec<_>>(),
        "trace_display": present::trace_tags(entry)
            .iter()
            .map(|s| json!({"name": &*s.name, "description": &*s.description}))
            .collect::<Vec<_>>(),
        "steps": entry.preprocessor_steps,
        "dictionaries": shown.iter().map(|(g, _)| id(g.dictionary)).collect::<Vec<_>>(),
        "glossaries": shown.iter().map(|(_, json)| json.clone()).collect::<Vec<_>>(),
        "frequencies": frequencies,
        "pitches": pitches,
    })
}

/// Whether `actual` has every field of `expected`.
fn matches(expected: &Value, actual: &Value) -> Result<(), String> {
    for (key, want) in expected.as_object().unwrap() {
        match key.as_str() {
            "trace_contains" | "trace_excludes" => {
                let trace = strings(&actual["trace"]);
                for name in strings(want) {
                    if trace.contains(&name) != (key == "trace_contains") {
                        return Err(format!("{key} {name}: trace {trace:?}"));
                    }
                }
            }
            _ if actual.get(key) != Some(want) => {
                return Err(format!(
                    "{key}: {} instead of {want}",
                    actual.get(key).unwrap_or(&Value::Null)
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_entries(expected: &Value, actual: &[Value]) -> Result<(), String> {
    let wanted = expected["entries"].as_array().cloned().unwrap_or_default();
    let label = |a: &Value| format!("{}/{}", a["expression"], a["reading"]);
    let got: Vec<String> = actual.iter().map(label).collect();
    match expected["order"].as_str().unwrap_or("any") {
        "exact" | "prefix" => {
            if expected["order"] == "exact" && wanted.len() != actual.len() {
                return Err(format!(
                    "{} groups instead of {}: {got:?}",
                    actual.len(),
                    wanted.len()
                ));
            }
            for (i, w) in wanted.iter().enumerate() {
                let a = actual
                    .get(i)
                    .ok_or_else(|| format!("group {i} missing: {got:?}"))?;
                matches(w, a)
                    .map_err(|e| format!("group {i} ({}): {e}; groups {got:?}", label(a)))?;
            }
        }
        _ => {
            for w in &wanted {
                if !actual.iter().any(|a| matches(w, a).is_ok()) {
                    let near: Vec<String> = actual
                        .iter()
                        .filter(|a| a["expression"] == w["expression"])
                        .map(|a| matches(w, a).unwrap_err())
                        .collect();
                    return Err(format!("no group like {w}: {got:?} {near:?}"));
                }
            }
        }
    }
    for a in expected["absent"].as_array().into_iter().flatten() {
        if actual.iter().any(|g| matches(a, g).is_ok()) {
            return Err(format!("{a} must be absent: {got:?}"));
        }
    }
    if let Some(count) = expected["count"].as_u64()
        && count as usize != actual.len()
    {
        return Err(format!(
            "{} groups instead of {count}: {got:?}",
            actual.len()
        ));
    }
    Ok(())
}

fn check_counts(env: &Env, expected: &Value) -> Result<(), String> {
    for (id, want) in expected["import_counts"].as_object().into_iter().flatten() {
        let c = env.imported[id].1.counts;
        let got =
            json!({"terms": c.terms, "freq": c.frequencies, "pitch": c.pitches, "kanji": c.kanji});
        if &got != want {
            return Err(format!("import counts of {id}: {got} instead of {want}"));
        }
    }
    for (id, want) in expected["import_titles"].as_object().into_iter().flatten() {
        let got = &env.imported[id].1.index.title;
        if want.as_str() != Some(got.as_str()) {
            return Err(format!("title of {id}: {got:?} instead of {want}"));
        }
    }
    Ok(())
}

fn lookup_case(env: &mut Env, case: &Value) -> Result<(), String> {
    let disabled = strings(&case["disabled"]);
    let dicts: Vec<Dictionary> = strings(&case["dictionaries"])
        .iter()
        .map(|id| env.dictionary(id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|d| !disabled.contains(&d.id))
        .collect();
    let lookup = Lookup::new(dicts, case["language"].as_str().unwrap());
    let max_results = case["options"]["max_results"].as_u64().unwrap_or(10) as usize;
    let query = case["query"].as_str().unwrap();
    let entries = lookup
        .lookup(query, Options { max_results })
        .map_err(|e| e.to_string())?;
    let actual: Vec<Value> = entries.iter().map(|e| describe(e, &lookup)).collect();
    let expected = &case["expected"];
    check_entries(expected, &actual)?;
    if let Some(best) = expected["best_length"].as_u64()
        && best as usize != best_length(&entries)
    {
        return Err(format!(
            "best length {} instead of {best}",
            best_length(&entries)
        ));
    }
    check_counts(env, expected)
}

fn kanji_case(env: &mut Env, case: &Value) -> Result<(), String> {
    let dicts: Vec<Dictionary> = strings(&case["dictionaries"])
        .iter()
        .map(|id| env.dictionary(id))
        .collect::<Result<_, _>>()?;
    let lookup = Lookup::new(dicts, "ja");
    let found = lookup
        .kanji(case["query"].as_str().unwrap())
        .map_err(|e| e.to_string())?;
    let actual: Vec<Value> = found
        .iter()
        .map(|k| {
            let stat = |name: &str| {
                k.kanji
                    .stats
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.clone())
            };
            let others: serde_json::Map<String, Value> = k
                .kanji
                .stats
                .iter()
                .filter(|(n, _)| n != "radical" && n != "strokes")
                .map(|(n, v)| (n.clone(), Value::String(v.clone())))
                .collect();
            json!({
                "dictionary": lookup.dictionaries()[k.dictionary].id,
                "character": k.kanji.character,
                "onyomi": k.kanji.onyomi,
                "kunyomi": k.kanji.kunyomi,
                "meanings": k.kanji.meanings,
                "radical": stat("radical"),
                "strokes": stat("strokes").and_then(|s| s.parse::<u64>().ok()),
                "stats": others,
            })
        })
        .collect();
    let expected = &case["expected"];
    let wanted = expected["entries"].as_array().cloned().unwrap_or_default();
    if wanted.len() != actual.len() {
        return Err(format!(
            "{} kanji entries instead of {}: {actual:?}",
            actual.len(),
            wanted.len()
        ));
    }
    for (w, a) in wanted.iter().zip(&actual) {
        matches(w, a)?;
    }
    for (id, want) in expected["dictionary_types"]
        .as_object()
        .into_iter()
        .flatten()
    {
        let kinds = env.imported[id].1.kinds;
        let mut got = Vec::new();
        if kinds.terms {
            got.push("term");
        }
        if kinds.kanji {
            got.push("kanji");
        }
        if json!(got) != *want {
            return Err(format!("types of {id}: {got:?} instead of {want}"));
        }
    }
    check_counts(env, expected)
}

fn deinflect_case(case: &Value) -> Result<(), String> {
    let d = Deinflector::for_language(case["language"].as_str().unwrap()).ok_or("no tables")?;
    let forms: Vec<(String, Vec<String>)> = d
        .deinflect(case["input"].as_str().unwrap())
        .into_iter()
        .map(|x| {
            let trace = x
                .trace
                .iter()
                .map(|&t| d.transform(t).name.clone())
                .collect();
            (x.text, trace)
        })
        .collect();
    let found = |w: &Value| {
        forms.iter().any(|(text, trace)| {
            w["text"] == *text.as_str() && w.get("trace").is_none_or(|t| strings(t) == *trace)
        })
    };
    for w in case["present"].as_array().into_iter().flatten() {
        if !found(w) {
            return Err(format!("{w} not produced"));
        }
    }
    for w in case["absent"].as_array().into_iter().flatten() {
        if found(w) {
            return Err(format!("{w} must not be produced"));
        }
    }
    Ok(())
}

fn frequency_case(case: &Value) -> Result<(), String> {
    let groups: Vec<(String, Vec<Frequency>)> = case["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            let values = g["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Frequency {
                    reading: None,
                    value: v["value"].as_i64().unwrap(),
                    display: v["display"]
                        .as_str()
                        .filter(|d| !d.is_empty())
                        .map(String::from),
                })
                .collect();
            (g["dictionary"].as_str().unwrap().to_string(), values)
        })
        .collect();
    let borrowed = || groups.iter().map(|(n, v)| (n.as_str(), v.as_slice()));
    let expected = &case["expected"];
    if let Some(h) = expected["harmonic"].as_u64() {
        let got = frequency::harmonic_rank(borrowed());
        if got != h {
            return Err(format!("harmonic {got} instead of {h}"));
        }
    }
    if let Some(html) = expected["html"].as_str() {
        let got = frequency::html(borrowed());
        if got != html {
            return Err(format!("html {got:?}"));
        }
    }
    Ok(())
}

fn pitch_case(case: &Value) -> Result<(), String> {
    let expected = &case["expected"];
    let reading = case["reading"].as_str().unwrap_or_default();
    let morae = pitch::morae(reading);
    if let Some(want) = expected.get("morae")
        && json!(morae) != *want
    {
        return Err(format!("morae {morae:?}"));
    }
    if let Some(want) = expected.get("high_low") {
        let mut seen = Vec::new();
        let mut got = Vec::new();
        for p in case["positions"].as_array().into_iter().flatten() {
            let p = p.as_u64().unwrap() as u32;
            if seen.contains(&p) || morae.is_empty() {
                continue;
            }
            seen.push(p);
            got.push(pitch::high_low(morae.len(), p));
        }
        if json!(got) != *want {
            return Err(format!("high/low {got:?}"));
        }
    }
    if let Some(want) = expected.get("rows") {
        let groups: Vec<pitch::Group> = case["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| pitch::Group {
                dictionary: g["dictionary"].as_str().unwrap().to_string(),
                positions: g["positions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_u64().unwrap() as u32)
                    .collect(),
                patterns: strings(&g["patterns"]),
                transcriptions: strings(&g["transcriptions"]),
            })
            .collect();
        let rows = pitch::rows(
            &groups,
            case["options"]["dedupe_pitch"].as_bool().unwrap_or(true),
        );
        let got: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({"dictionaries": r.dictionaries, "positions": r.positions, "patterns": r.patterns, "transcriptions": r.transcriptions})
            })
            .collect();
        if json!(got) != *want {
            return Err(format!("rows {got:?}"));
        }
    }
    Ok(())
}

// T-LKP-020, T-LKP-021, T-LKP-022, T-LKP-023, T-LKP-024, T-LKP-030, T-LKP-031, T-LKP-032, T-LKP-033
// T-LKP-034, T-LKP-035, T-LKP-036, T-LKP-037, T-LKP-040, T-LKP-041, T-LKP-043, T-DIC-015, T-DIC-032
#[test]
fn fushi_lookup_vectors() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/lookup.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut env = Env {
        tmp: tempfile::tempdir().unwrap(),
        sources: doc["dictionaries"].as_object().unwrap().clone(),
        imported: HashMap::new(),
    };
    let mut failures = Vec::new();
    let (mut run, mut known) = (0, 0);
    for case in doc["cases"].as_array().unwrap() {
        if case["skip"] == Value::Bool(true) {
            continue;
        }
        run += 1;
        let result = match case["kind"].as_str().unwrap() {
            "lookup" => lookup_case(&mut env, case),
            "kanji" => kanji_case(&mut env, case),
            "deinflect" => deinflect_case(case),
            "frequency" => frequency_case(case),
            "pitch" => pitch_case(case),
            kind => Err(format!("unknown kind {kind}")),
        };
        let label = format!(
            "{} ({} › {})",
            case["id"].as_str().unwrap(),
            case["provenance"]["file"].as_str().unwrap_or("?"),
            case["provenance"]["test"].as_str().unwrap_or("?"),
        );
        // a known failure must keep failing until its mark is removed
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
    eprintln!("{run} lookup cases, {known} known failures");
    assert!(run >= 240, "only {run} cases ran");
    assert!(
        failures.is_empty(),
        "{} of {run} cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
