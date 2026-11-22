//! Drift test (D1): the checked-in schemas must match what the canonical
//! Rust types generate. If this fails, run
//! `cargo run -p euthymia-schema --bin schema-gen` and commit the result —
//! never hand-edit the files.
//!
//! The last test also pins what the checked-in schemas assert on their own:
//! `format: "date"` must reject impossible dates for a schema-only consumer
//! that enables format assertions.

use std::fs;
use std::path::Path;

use euthymia_schema::schema::{
    DPO_SCHEMA_FILE, SFT_SCHEMA_FILE, dpo_pair_schema, render_schema, schemas_dir,
    sft_record_schema,
};
use serde_json::{Value, json};

#[test]
fn sft_record_schema_is_current() {
    let checked_in = fs::read_to_string(schemas_dir().join(SFT_SCHEMA_FILE))
        .expect("schemas/sft_record.v1.json is checked in");
    assert_eq!(
        checked_in,
        render_schema(&sft_record_schema()),
        "{SFT_SCHEMA_FILE} is stale — regenerate with `cargo run -p euthymia-schema --bin schema-gen`"
    );
}

#[test]
fn dpo_pair_schema_is_current() {
    let checked_in = fs::read_to_string(schemas_dir().join(DPO_SCHEMA_FILE))
        .expect("schemas/dpo_pair.v1.json is checked in");
    assert_eq!(
        checked_in,
        render_schema(&dpo_pair_schema()),
        "{DPO_SCHEMA_FILE} is stale — regenerate with `cargo run -p euthymia-schema --bin schema-gen`"
    );
}

fn schema_validator(schema_file: &str) -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(schemas_dir().join(schema_file)).expect("checked-in schema exists"),
    )
    .expect("checked-in schema parses");
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("schema compiles")
}

fn first_fixture_record(fixture_file: &str) -> Value {
    let fixture = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture_file),
    )
    .expect("fixture exists");
    serde_json::from_str(fixture.lines().next().unwrap()).expect("fixture line parses")
}

fn assert_generated_at(validator: &jsonschema::Validator, record: &mut Value, schema_file: &str) {
    for bad_date in ["2026-02-30", "2026-13-01"] {
        *record.pointer_mut("/provenance/generated_at").unwrap() = json!(bad_date);
        assert!(
            !validator.is_valid(record),
            "{schema_file} must reject generated_at {bad_date}"
        );
    }
    *record.pointer_mut("/provenance/generated_at").unwrap() = json!("2026-01-15");
    assert!(
        validator.is_valid(record),
        "{schema_file} must accept a real generated_at"
    );
}

fn assert_verified_at(validator: &jsonschema::Validator, record: &mut Value, schema_file: &str) {
    for (verified_at, expect_valid) in [
        (Value::Null, true),
        (json!("2026-01-16"), true),
        (json!("2026-02-30"), false),
    ] {
        record["verification"] = json!({"status": "admitted", "verified_at": verified_at});
        assert_eq!(
            validator.is_valid(record),
            expect_valid,
            "{schema_file} verified_at {verified_at}"
        );
    }
}

/// A schema-only consumer that enables format assertions must reject
/// impossible dates — the schemas carry `format: "date"` on
/// `generated_at`/`verified_at`, not only the digit pattern. (The CLI turns
/// the same assertions on; cross-field rules remain the fallback for
/// annotation-mode consumers.)
#[test]
fn checked_in_schemas_assert_real_dates() {
    for (schema_file, fixture_file) in [
        (SFT_SCHEMA_FILE, "valid_sft.jsonl"),
        (DPO_SCHEMA_FILE, "valid_dpo.jsonl"),
    ] {
        let validator = schema_validator(schema_file);
        let mut record = first_fixture_record(fixture_file);
        assert!(
            validator.is_valid(&record),
            "{schema_file} must accept its own valid fixture row"
        );
        assert_generated_at(&validator, &mut record, schema_file);
        assert_verified_at(&validator, &mut record, schema_file);
    }
}
