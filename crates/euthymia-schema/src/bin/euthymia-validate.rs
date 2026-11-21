//! euthymia-validate — record validator for euthymia JSONL files (D9).
//!
//!   euthymia-validate <sft|dpo> <file.jsonl> [...]
//!
//! Validates each non-empty line against the checked-in JSON Schema
//! (schemas/*.v1.json, embedded at build time) plus the cross-field rules
//! the schema cannot express. Every failure is reported as
//! `<file>:<line>: <error>`; exit status is non-zero if any line fails.
//! This is the binary CI runs, so local and CI validation cannot disagree.

use std::fs;
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
        failures += validate_file(path, cli.record, &validator)?;
    }

    if failures > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn embedded_schema(kind: RecordKind) -> Result<Value> {
    // Filenames must stay in sync with SFT_SCHEMA_FILE / DPO_SCHEMA_FILE.
    let text = match kind {
        RecordKind::Sft => {
            debug_assert_eq!(SFT_SCHEMA_FILE, "sft_record.v1.json");
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../schemas/sft_record.v1.json"
            ))
        }
        RecordKind::Dpo => {
            debug_assert_eq!(DPO_SCHEMA_FILE, "dpo_pair.v1.json");
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../schemas/dpo_pair.v1.json"
            ))
        }
    };
    serde_json::from_str(text).context("embedded schema failed to parse")
}

fn validate_file(
    path: &Path,
    kind: RecordKind,
    validator: &jsonschema::Validator,
) -> Result<usize> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut failures = 0usize;

    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        for error in validate_line(line, kind, validator) {
            eprintln!("{}:{line_no}: {error}", path.display());
            failures += 1;
        }
    }
    Ok(failures)
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

    // Cross-field rules need the typed record; when schema validation already
    // failed, deserialization usually fails too, so only run them on a clean
    // record rather than doubling the noise.
    if errors.is_empty() {
        match kind {
            RecordKind::Sft => match serde_json::from_value::<SftRecord>(value) {
                Ok(record) => errors.extend(sft_cross_field_errors(&record)),
                Err(err) => errors.push(format!("record failed to deserialize: {err}")),
            },
            RecordKind::Dpo => match serde_json::from_value::<DpoPairRecord>(value) {
                Ok(record) => errors.extend(dpo_cross_field_errors(&record)),
                Err(err) => errors.push(format!("record failed to deserialize: {err}")),
            },
        }
    }
    errors
}
