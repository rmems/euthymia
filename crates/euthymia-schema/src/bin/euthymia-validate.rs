//! euthymia-validate — record validator for euthymia JSONL files (D9).
//!
//!   euthymia-validate <sft|dpo> <file.jsonl> [...]
//!
//! Validates each non-empty line against the checked-in JSON Schema
//! (schemas/*.v1.json, embedded at build time) plus the cross-field rules
//! the schema cannot express. Every failure is reported as
//! `<file>:<line>: <error>`; exit status is non-zero if any line fails.
//! This is the binary CI runs, so local and CI validation cannot disagree.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use euthymia_schema::schema::{DPO_SCHEMA_FILE, SFT_SCHEMA_FILE};
use euthymia_schema::validate::{dpo_cross_field_errors, sft_cross_field_errors};
use euthymia_schema::{DpoPairRecord, SftRecord};
use serde_json::Value;

#[derive(Parser)]
#[command(
    name = "euthymia-validate",
    about = "Validate euthymia SFT/DPO JSONL records against the checked-in schemas and cross-field rules"
)]
struct Cli {
    /// Which record schema to validate against
    #[arg(value_enum)]
    record: RecordKind,
    /// JSONL files to validate (one record per line)
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum RecordKind {
    /// SFT records (schemas/sft_record.v1.json)
    Sft,
    /// DPO preference pairs (schemas/dpo_pair.v1.json)
    Dpo,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();

    let schema_json = embedded_schema(cli.record)?;
    let validator =
        jsonschema::validator_for(&schema_json).context("embedded schema failed to compile")?;

    let mut failures = 0usize;
    for path in &cli.files {
        failures += validate_file(path, cli.record, &validator);
    }

    if failures > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn embedded_schema(kind: RecordKind) -> Result<Value> {
    // Literal paths: include_str! resolves at compile time relative to this
    // file. Keep them in sync with SFT_SCHEMA_FILE / DPO_SCHEMA_FILE.
    let text = match kind {
        RecordKind::Sft => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../schemas/sft_record.v1.json"
        )),
        RecordKind::Dpo => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../schemas/dpo_pair.v1.json"
        )),
    };
    let _ = (SFT_SCHEMA_FILE, DPO_SCHEMA_FILE); // named in docs above
    serde_json::from_str(text).context("embedded schema failed to parse")
}

fn validate_file(path: &Path, kind: RecordKind, validator: &jsonschema::Validator) -> usize {
    // An unopenable file is a validation failure, not a reason to stop the
    // batch — remaining files still get their per-line report.
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("{}: {err}", path.display());
            return 1;
        }
    };

    let mut failures = 0usize;
    // Record ids seen so far -> first line, for duplicate detection.
    let mut seen_ids: HashMap<String, usize> = HashMap::new();

    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_no = index + 1;
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                // A mid-file read error (e.g. invalid UTF-8) fails the file;
                // there is no trustworthy line boundary to resume from.
                eprintln!("{}:{line_no}: {err}", path.display());
                return failures + 1;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        for error in validate_line(&line, kind, validator) {
            eprintln!("{}:{line_no}: {error}", path.display());
            failures += 1;
        }
        // Duplicate record ids inside a file are corrupt data regardless of
        // per-line validity.
        if let Some(id) = record_id(&line, kind) {
            match seen_ids.get(&id) {
                Some(first) => {
                    eprintln!(
                        "{}:{line_no}: duplicate {id} (first seen at line {first})",
                        path.display()
                    );
                    failures += 1;
                }
                None => {
                    seen_ids.insert(id, line_no);
                }
            }
        }
    }
    failures
}

/// Pull the identifying id off a line without full validation — best-effort,
/// used only for duplicate detection.
fn record_id(line: &str, kind: RecordKind) -> Option<String> {
    let key = match kind {
        RecordKind::Sft => "scenario_id",
        RecordKind::Dpo => "pair_id",
    };
    serde_json::from_str::<Value>(line)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

fn validate_line(line: &str, kind: RecordKind, validator: &jsonschema::Validator) -> Vec<String> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(err) => return vec![format!("invalid JSON: {err}")],
    };

    let mut errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|err| err.to_string())
        .collect();

    // Cross-field rules need the typed record. Schema failures usually make
    // deserialization fail too, but not always (e.g. maxItems has no Rust
    // equivalent) — run the cross-field rules whenever the record deserializes
    // so a line reports every rule it breaks, not just the schema's share.
    match kind {
        RecordKind::Sft => match serde_json::from_value::<SftRecord>(value) {
            Ok(record) => errors.extend(sft_cross_field_errors(&record)),
            Err(err) if errors.is_empty() => {
                errors.push(format!("record failed to deserialize: {err}"));
            }
            Err(_) => {}
        },
        RecordKind::Dpo => match serde_json::from_value::<DpoPairRecord>(value) {
            Ok(record) => errors.extend(dpo_cross_field_errors(&record)),
            Err(err) if errors.is_empty() => {
                errors.push(format!("record failed to deserialize: {err}"));
            }
            Err(_) => {}
        },
    }
    errors
}
