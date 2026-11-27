//! Reusable record-validation engine shared by `euthymia-validate` and the
//! admission pipeline. Keeping the JSON Schema compilation, deserialization,
//! and cross-field checks here is what makes admission and CI unable to
//! disagree about whether a line is a valid record.

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::schema::{DPO_SCHEMA_FILE, SFT_SCHEMA_FILE};
use crate::validate::{dpo_cross_field_errors, sft_cross_field_errors};

/// Which record schema to validate against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum RecordKind {
    /// SFT records (`schemas/sft_record.v1.json`)
    Sft,
    /// DPO preference pairs (`schemas/dpo_pair.v1.json`)
    Dpo,
}

impl RecordKind {
    /// The key uniquely identifying one record of this kind.
    pub fn id_key(self) -> &'static str {
        match self {
            Self::Sft => "scenario_id",
            Self::Dpo => "pair_id",
        }
    }
}

/// A compiled validator for one record kind.
pub struct RecordValidator {
    kind: RecordKind,
    validator: jsonschema::Validator,
}

impl RecordValidator {
    /// Compile the embedded checked-in schema for `kind`.
    pub fn new(kind: RecordKind) -> Result<Self> {
        let schema_json = embedded_schema(kind)?;
        // Format assertions are opt-in for Draft 2020-12; the schemas mark
        // date fields `format: "date"` so impossible dates fail at the schema
        // layer, not only in the cross-field rules.
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(&schema_json)
            .context("embedded schema failed to compile")?;
        Ok(Self { kind, validator })
    }

    /// The record kind this validator checks.
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// Validate one non-empty JSONL line. The result includes JSON Schema,
    /// deserialization, and cross-field errors.
    pub fn validate_line(&self, line: &str) -> Vec<String> {
        validate_line(line, self.kind, &self.validator)
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

/// Pull the identifying id off a line without full validation — best-effort,
/// used for duplicate detection and diagnostics.
pub fn record_id(line: &str, kind: RecordKind) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get(kind.id_key())?
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
