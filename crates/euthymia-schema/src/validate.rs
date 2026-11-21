//! Cross-field validation rules that JSON Schema cannot express (D7–D9).
//! `euthymia-validate` applies these after a line passes JSON Schema
//! validation; unit and fixture tests exercise them directly.

use std::collections::HashSet;

use crate::enums::{Family, VerificationStatus};
use crate::records::{DpoPairRecord, Provenance, SftRecord, VerificationResult};

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

/// The `scenario_id` family code must match `family` — the ID pattern (D8)
/// binds codes to families in a fixed order, so a mismatched code is a
/// malformed record even though each field is individually valid.
fn family_code_error(scenario_id: &str, family: Family) -> Option<String> {
    let code = scenario_id.split('-').nth(1);
    if code == Some(family.id_code()) {
        None
    } else {
        Some(format!(
            "scenario_id family code {} does not match family {:?} (expected code {})",
            code.unwrap_or("<none>"),
            family,
            family.id_code()
        ))
    }
}

/// `YYYY-MM-DD` is enforced by the schema pattern; this checks the date
/// actually exists (no `2026-02-30`).
fn valid_calendar_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    let [Ok(year), Ok(month), Ok(day)] = [
        parts.first().unwrap_or(&"").parse::<u32>(),
        parts.get(1).unwrap_or(&"").parse::<u32>(),
        parts.get(2).unwrap_or(&"").parse::<u32>(),
    ] else {
        return false;
    };
    if parts.len() != 3 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ][(month - 1) as usize];
    (1..=days_in_month).contains(&day)
}

fn provenance_errors(provenance: &Provenance) -> Option<String> {
    if valid_calendar_date(&provenance.generated_at) {
        None
    } else {
        Some(format!(
            "provenance.generated_at {:?} is not a real calendar date",
            provenance.generated_at
        ))
    }
}

fn verification_date_error(verification: &VerificationResult) -> Option<String> {
    match &verification.verified_at {
        Some(date) if !valid_calendar_date(date) => Some(format!(
            "verification.verified_at {date:?} is not a real calendar date"
        )),
        _ => None,
    }
}

fn shared_errors(
    scenario_id: &str,
    family: Family,
    is_negative_fixture: bool,
    provenance: &Provenance,
    verification: Option<&VerificationResult>,
) -> Vec<String> {
    [
        never_admitted(is_negative_fixture, verification),
        family_code_error(scenario_id, family),
        provenance_errors(provenance),
        verification.and_then(verification_date_error),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Cross-field rules for an [`SftRecord`] (D4, D7–D9, plus the taxonomy's
/// secondary-pool rule).
pub fn sft_cross_field_errors(record: &SftRecord) -> Vec<String> {
    let mut errors = shared_errors(
        &record.scenario_id,
        record.family,
        record.is_negative_fixture,
        &record.provenance,
        record.verification.as_ref(),
    );

    // A labeled negative fixture must name the rule expected to reject it
    // (D7); an empty citation is not a label.
    match record.expected_rejection.as_deref().map(str::trim) {
        _ if !record.is_negative_fixture => {
            if record.expected_rejection.is_some() {
                errors.push(
                    "expected_rejection may only be present on a labeled negative fixture"
                        .to_string(),
                );
            }
        }
        Some(citation) if !citation.is_empty() => {}
        Some(_) => errors.push(
            "negative fixture must carry a non-empty expected_rejection citation".to_string(),
        ),
        None => errors.push(
            "negative fixture must carry expected_rejection (contract §3 labeled negative fixture)"
                .to_string(),
        ),
    }

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
    let pool = record.family.secondary_pool();
    for competency in &record.secondary_competencies {
        if !pool.contains(competency) {
            errors.push(format!(
                "secondary_competency {competency:?} is outside the {:?} pool",
                record.family
            ));
        }
    }
    errors
}

/// Cross-field rules for a [`DpoPairRecord`] (D5, D7–D9).
pub fn dpo_cross_field_errors(record: &DpoPairRecord) -> Vec<String> {
    // The DPO record has no expected_rejection field (D5): a pair labels a
    // §3-violating rejected side with is_negative_fixture, so only the
    // "never admitted" leg of D7 applies.
    let mut errors = shared_errors(
        &record.scenario_id,
        record.family,
        record.is_negative_fixture,
        &record.provenance,
        record.verification.as_ref(),
    );

    if record.chosen == record.rejected {
        errors.push("chosen and rejected must differ".to_string());
    }
    errors
}
