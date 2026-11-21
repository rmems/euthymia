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

fn assert_fails_for(record: &str, file: &str, rule: &str) {
    let output = run_validator(record, file);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{file} should fail validation ({rule})"
    );
    assert!(
        stderr.contains(rule),
        "{file} should fail for its named rule {rule:?}, got:\n{stderr}"
    );
    assert!(
        stderr.contains(":1:"),
        "{file} error should carry the line number, got:\n{stderr}"
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

#[test]
fn invalid_fixtures_fail_for_named_rules() {
    let cases: &[(&str, &str, &str)] = &[
        ("sft", "invalid_sft_bad_enum.jsonl", "estrangement"),
        (
            "sft",
            "invalid_sft_unknown_field.jsonl",
            "Additional properties are not allowed",
        ),
        (
            "sft",
            "invalid_sft_secondary_equals_primary.jsonl",
            "secondary_competencies must not contain primary_competency",
        ),
        (
            "sft",
            "invalid_sft_missing_provenance.jsonl",
            "\"provenance\" is a required property",
        ),
        (
            "sft",
            "invalid_sft_negative_without_expected_rejection.jsonl",
            "negative fixture must carry expected_rejection",
        ),
        (
            "sft",
            "invalid_sft_wrong_schema_version.jsonl",
            "1 was expected",
        ),
        (
            "sft",
            "invalid_sft_secondary_duplicate.jsonl",
            "secondary_competencies must be unique",
        ),
        (
            "sft",
            "invalid_sft_too_many_secondaries.jsonl",
            "has more than 2 items",
        ),
        (
            "sft",
            "invalid_sft_negative_admitted.jsonl",
            "must never have verification.status = admitted",
        ),
        (
            "sft",
            "invalid_sft_secondary_outside_pool.jsonl",
            "outside the FailureSetbacks pool",
        ),
        (
            "sft",
            "invalid_sft_family_code_mismatch.jsonl",
            "does not match family",
        ),
        (
            "sft",
            "invalid_sft_impossible_date.jsonl",
            "not a real calendar date",
        ),
        (
            "dpo",
            "invalid_dpo_chosen_equals_rejected.jsonl",
            "chosen and rejected must differ",
        ),
    ];
    for (record, file, rule) in cases {
        assert_fails_for(record, file, rule);
    }
}
