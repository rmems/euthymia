//! Fixture pass/fail set (D10): valid fixture files validate cleanly through
//! the real `euthymia-validate` binary; every invalid file fails, each for
//! its named rule.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn run_validator(record: &str, file: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_euthymia-validate"))
        .arg(record)
        .arg(fixtures_dir().join(file))
        .output()
        .expect("euthymia-validate runs")
}

fn assert_passes(record: &str, file: &str) {
    let output = run_validator(record, file);
    assert!(
        output.status.success(),
        "{file} should validate, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_fails_for(record: &str, file: &str, rules: &[&str]) {
    let output = run_validator(record, file);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{file} should fail validation ({rules:?})"
    );
    for rule in rules {
        assert!(
            stderr.contains(rule),
            "{file} should fail for its named rule {rule:?}, got:\n{stderr}"
        );
    }
    assert!(
        stderr.lines().all(|l| l.contains(".jsonl:")),
        "{file} errors should be reported as <file>:<line>: <error>, got:\n{stderr}"
    );
}

#[test]
fn valid_sft_fixtures_pass() {
    assert_passes("sft", "valid_sft.jsonl");
}

#[test]
fn valid_dpo_fixtures_pass() {
    assert_passes("dpo", "valid_dpo.jsonl");
}

/// Every invalid fixture file fails for its named rule (D10). One test per
/// case so a regression names the rule it broke; a case may pin several
/// expected error strings when a record breaks more than one layer.
macro_rules! invalid_fixture {
    ($name:ident: $record:literal, $file:literal, $($rule:literal),+) => {
        #[test]
        fn $name() {
            assert_fails_for($record, $file, &[$($rule),+]);
        }
    };
}

invalid_fixture!(bad_enum: "sft", "invalid_sft_bad_enum.jsonl", "estrangement");
invalid_fixture!(unknown_field: "sft", "invalid_sft_unknown_field.jsonl", "Additional properties are not allowed");
invalid_fixture!(secondary_equals_primary: "sft", "invalid_sft_secondary_equals_primary.jsonl", "secondary_competencies must not contain primary_competency");
invalid_fixture!(missing_provenance: "sft", "invalid_sft_missing_provenance.jsonl", "\"provenance\" is a required property");
invalid_fixture!(negative_without_expected_rejection: "sft", "invalid_sft_negative_without_expected_rejection.jsonl", "negative fixture must carry expected_rejection");
invalid_fixture!(wrong_schema_version: "sft", "invalid_sft_wrong_schema_version.jsonl", "1 was expected");
invalid_fixture!(secondary_duplicate: "sft", "invalid_sft_secondary_duplicate.jsonl", "secondary_competencies must be unique");
invalid_fixture!(too_many_secondaries: "sft", "invalid_sft_too_many_secondaries.jsonl", "has more than 2 items");
invalid_fixture!(negative_admitted: "sft", "invalid_sft_negative_admitted.jsonl", "must never have verification.status = admitted");
invalid_fixture!(secondary_outside_pool: "sft", "invalid_sft_secondary_outside_pool.jsonl", "outside the FailureSetbacks pool");
invalid_fixture!(family_code_mismatch: "sft", "invalid_sft_family_code_mismatch.jsonl", "does not match family");
invalid_fixture!(impossible_date: "sft", "invalid_sft_impossible_date.jsonl", "not a \"date\"", "not a real calendar date");
invalid_fixture!(primary_outside_pool: "sft", "invalid_sft_primary_outside_pool.jsonl", "outside the Jealousy pool");
invalid_fixture!(duplicate_id: "sft", "invalid_sft_duplicate_id.jsonl", "duplicate eu-brk-001");
invalid_fixture!(chosen_equals_rejected: "dpo", "invalid_dpo_chosen_equals_rejected.jsonl", "chosen and rejected must differ");
