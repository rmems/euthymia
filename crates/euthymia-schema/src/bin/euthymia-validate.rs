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

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use euthymia_schema::schema::{DPO_SCHEMA_FILE, SFT_SCHEMA_FILE};
use euthymia_schema::validate::{dpo_cross_field_errors, sft_cross_field_errors};
use serde::de::DeserializeOwned;
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
    // Format assertions are opt-in for Draft 2020-12; the schemas mark date
    // fields `format: "date"` so impossible dates fail at the schema layer,
    // not only in the cross-field rules.
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema_json)
        .context("embedded schema failed to compile")?;

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
    // include_str! needs literal paths; a version bump that updates
    // SFT_SCHEMA_FILE/DPO_SCHEMA_FILE but not these paths would silently
    // validate against the previous schema. The generated $id names its
    // file, so check it against the constant instead of trusting the literal.
    let (text, expected_file) = match kind {
        RecordKind::Sft => (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../schemas/sft_record.v1.json"
            )),
            SFT_SCHEMA_FILE,
        ),
        RecordKind::Dpo => (
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../schemas/dpo_pair.v1.json"
            )),
            DPO_SCHEMA_FILE,
        ),
    };
    let schema: Value = serde_json::from_str(text).context("embedded schema failed to parse")?;
    let id_file = schema
        .get("$id")
        .and_then(Value::as_str)
        .and_then(|id| id.rsplit('/').next())
        .unwrap_or_default();
    ensure!(
        id_file == expected_file,
        "embedded schema is {id_file}, expected {expected_file} — bump the include_str! paths with the version"
    );
    Ok(schema)
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
        let pos = Pos {
            path,
            line: index + 1,
        };
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                // A mid-file read error (e.g. invalid UTF-8) fails the file;
                // there is no trustworthy line boundary to resume from.
                report(pos, err);
                return failures + 1;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        failures += check_line(pos, &line, |l| validate_line(l, kind, validator));
        failures += check_duplicate_id(&mut seen_ids, pos, &line, kind);
    }
    failures
}

/// Where a JSONL line sits — every diagnostic prints as
/// `<file>:<line>: <error>`.
#[derive(Clone, Copy)]
struct Pos<'a> {
    path: &'a Path,
    line: usize,
}

fn report(pos: Pos<'_>, error: impl std::fmt::Display) {
    eprintln!("{}:{}: {error}", pos.path.display(), pos.line);
}

fn check_line(pos: Pos<'_>, line: &str, check: impl FnOnce(&str) -> Vec<String>) -> usize {
    let errors = check(line);
    for error in &errors {
        report(pos, error);
    }
    errors.len()
}

/// Duplicate record ids inside a file are corrupt data regardless of
/// per-line validity.
fn check_duplicate_id(
    seen_ids: &mut HashMap<String, usize>,
    pos: Pos<'_>,
    line: &str,
    kind: RecordKind,
) -> usize {
    let Some(id) = record_id(line, kind) else {
        return 0;
    };
    match seen_ids.get(&id) {
        Some(first) => {
            report(
                pos,
                format_args!("duplicate {id} (first seen at line {first})"),
            );
            1
        }
        None => {
            seen_ids.insert(id, pos.line);
            0
        }
    }
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

/// Deserialization plus cross-field rules, shared by both record kinds.
/// Schema failures usually make deserialization fail too, but not always
/// (e.g. maxItems has no Rust equivalent) — run the cross-field rules
/// whenever the record deserializes so a line reports every rule it breaks,
/// not just the schema's share.
fn typed_errors<T: DeserializeOwned>(
    value: Value,
    schema_failed: bool,
    check: impl FnOnce(&T) -> Vec<String>,
) -> Vec<String> {
    match serde_json::from_value::<T>(value) {
        Ok(record) => check(&record),
        Err(err) if !schema_failed => vec![format!("record failed to deserialize: {err}")],
        Err(_) => Vec::new(),
    }
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
    let schema_failed = !errors.is_empty();

    errors.extend(match kind {
        RecordKind::Sft => typed_errors(value, schema_failed, sft_cross_field_errors),
        RecordKind::Dpo => typed_errors(value, schema_failed, dpo_cross_field_errors),
    });
    errors
}
