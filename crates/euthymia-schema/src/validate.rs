//! Cross-field validation rules that JSON Schema cannot express (D7–D9).
//! `euthymia-validate` applies these after a line passes JSON Schema
//! validation; unit and fixture tests exercise them directly.

use std::collections::HashSet;

use crate::enums::VerificationStatus;
use crate::records::{DpoPairRecord, SftRecord, VerificationResult};

/// The leg of D7 shared by both record types: a labeled negative fixture
/// must never be admitted.
fn never_admitted(
    is_negative_fixture: bool,
    verification: Option<&VerificationResult>,
) -> Option<String> {
    if is_negative_fixture
        && matches!(
            verification.map(|v| v.status),
            Some(VerificationStatus::Admitted)
        )
    {
        Some(
            "negative fixture must never have verification.status = admitted (contract §3)"
                .to_string(),
        )
    } else {
        None
    }
}

/// Cross-field rules for an [`SftRecord`] (D4, D7–D9).
pub fn sft_cross_field_errors(record: &SftRecord) -> Vec<String> {
    let mut errors = Vec::new();

    if record.is_negative_fixture && record.expected_rejection.is_none() {
        errors.push(
            "negative fixture must carry expected_rejection (contract §3 labeled negative fixture)"
                .to_string(),
        );
    }
    if !record.is_negative_fixture && record.expected_rejection.is_some() {
        errors.push(
            "expected_rejection may only be present on a labeled negative fixture".to_string(),
        );
    }
    errors.extend(never_admitted(
        record.is_negative_fixture,
        record.verification.as_ref(),
    ));

    if record
        .secondary_competencies
        .contains(&record.primary_competency)
    {
        errors.push("secondary_competencies must not contain primary_competency".to_string());
    }
    let mut seen = HashSet::new();
    if record
        .secondary_competencies
        .iter()
        .any(|c| !seen.insert(c))
    {
        errors.push("secondary_competencies must be unique".to_string());
    }
    if record.secondary_competencies.len() > 2 {
        errors.push("secondary_competencies allows at most 2 entries".to_string());
    }
    errors
}

/// Cross-field rules for a [`DpoPairRecord`] (D5, D7–D9).
pub fn dpo_cross_field_errors(record: &DpoPairRecord) -> Vec<String> {
    // The DPO record has no expected_rejection field (D5): a pair labels a
    // §3-violating rejected side with is_negative_fixture, so only the
    // "never admitted" leg of D7 applies.
    let mut errors: Vec<String> =
        never_admitted(record.is_negative_fixture, record.verification.as_ref())
            .into_iter()
            .collect();

    if record.chosen == record.rejected {
        errors.push("chosen and rejected must differ".to_string());
    }
    errors
}
