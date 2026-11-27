//! euthymia-validate — record validator for euthymia JSONL files (D9).
//!
//!   euthymia-validate <sft|dpo> <file.jsonl> [...]
//!
//! Validates each non-empty line against the checked-in JSON Schema plus the
//! cross-field rules the schema cannot express. Every failure is reported as
//! `<file>:<line>: <error>`; exit status is non-zero if any line fails.
//! This is the binary CI runs, so local and CI validation cannot disagree.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use euthymia_schema::validator::{RecordKind, RecordValidator, record_id};

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

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let validator = RecordValidator::new(cli.record)?;

    let mut failures = 0usize;
    for path in &cli.files {
        failures += validate_file(path, &validator);
    }

    if failures > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn validate_file(path: &Path, validator: &RecordValidator) -> usize {
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
        failures += check_line(pos, &line, |l| validator.validate_line(l));
        failures += check_duplicate_id(&mut seen_ids, pos, &line, validator.kind());
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
