// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checks the shipped examples and the invalid-template corpus against both
//! the Rust validator and the JSON Schema, so the two cannot drift apart.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

fn schema_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema")
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json") && !p.ends_with("expected.json"))
        .collect();
    files.sort();
    files
}

fn json_schema() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_slice(&fs::read(schema_dir().join("template.v1.json")).unwrap()).unwrap();
    jsonschema::validator_for(&schema).expect("template.v1.json is a valid JSON Schema")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    error: String,
    schema_rejects: bool,
}

#[test]
fn starter_templates_are_valid() {
    let examples = json_files(&schema_dir().join("examples"));
    assert_eq!(
        examples.len(),
        4,
        "the four built-in starter templates (TPL-012)"
    );
    let schema = json_schema();
    for file in examples {
        let bytes = fs::read(&file).unwrap();
        if let Err(issues) = chalkboard_template::validate(&bytes) {
            panic!("{} should be valid: {issues:?}", file.display());
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let errors: Vec<String> = schema.iter_errors(&value).map(|e| e.to_string()).collect();
        assert!(
            errors.is_empty(),
            "{} fails the JSON Schema: {errors:?}",
            file.display()
        );
    }
}

#[test]
fn invalid_templates_fail_with_readable_errors() {
    let dir = schema_dir().join("tests/invalid");
    let expected: BTreeMap<String, Expected> =
        serde_json::from_slice(&fs::read(dir.join("expected.json")).unwrap()).unwrap();
    let files = json_files(&dir);
    assert_eq!(
        files.len(),
        expected.len(),
        "every invalid file needs an expectation"
    );
    let schema = json_schema();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let exp = expected
            .get(&name)
            .unwrap_or_else(|| panic!("no expectation for {name}"));
        let bytes = fs::read(&file).unwrap();
        let issues = chalkboard_template::validate(&bytes).expect_err(&name);
        let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|m| m.contains(&exp.error)),
            "{name}: expected an error containing {:?}, got {rendered:?}",
            exp.error
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            !schema.is_valid(&value),
            exp.schema_rejects,
            "{name}: schemaRejects is wrong — keep expected.json in step with template.v1.json"
        );
    }
}

/// Stable, dependency-free stand-in for the cargo-fuzz target (TPL-011):
/// randomly damaged templates must be rejected or accepted, never panic.
#[test]
fn mutated_templates_never_panic() {
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let originals: Vec<Vec<u8>> = json_files(&schema_dir().join("examples"))
        .into_iter()
        .chain(json_files(&schema_dir().join("tests/invalid")))
        .map(|f| fs::read(f).unwrap())
        .collect();
    let alphabet = b"{}[]\":,0123456789abc -.|{{}}\\\x00\xff";
    for i in 0..5000 {
        let mut doc = originals[i % originals.len()].clone();
        for _ in 0..1 + next() % 4 {
            if doc.is_empty() {
                break;
            }
            let at = (next() as usize) % doc.len();
            let byte = alphabet[(next() as usize) % alphabet.len()];
            match next() % 4 {
                0 => {
                    doc.remove(at);
                }
                1 => doc.insert(at, byte),
                2 => doc[at] = byte,
                _ => {
                    let end = (at + (next() as usize) % 64).min(doc.len());
                    let chunk = doc[at..end].to_vec();
                    doc.splice(at..at, chunk);
                }
            }
        }
        let _ = chalkboard_template::validate(&doc);
    }
}

#[test]
fn oversized_documents_are_rejected_before_parsing() {
    let big = vec![b' '; chalkboard_template::MAX_DOCUMENT_BYTES + 1];
    let issues = chalkboard_template::validate(&big).unwrap_err();
    assert!(issues[0].message.contains("at most"));
}

#[test]
fn deeply_nested_templates_are_rejected() {
    let mut node = r#"{"type":"text","text":"x","fit":{"mode":"clip"}}"#.to_string();
    for _ in 0..40 {
        node = format!(r#"{{"type":"container","children":[{node}]}}"#);
    }
    let doc = format!(
        r#"{{"schemaVersion":1,"id":"deep","name":"Deep","canvas":{{"width":1920,"height":1080}},"root":{node}}}"#
    );
    let issues = chalkboard_template::validate(doc.as_bytes()).unwrap_err();
    assert!(
        issues.iter().any(|i| i.message.contains("nesting deeper")),
        "{issues:?}"
    );
}
