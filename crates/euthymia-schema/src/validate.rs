//! Cross-field validation rules that JSON Schema cannot express (D7–D9).
//! `euthymia-validate` applies these after a line passes JSON Schema
//! validation; unit and fixture tests exercise them directly.

use std::collections::HashSet;

use crate::enums::{Competency, Family, VerificationStatus};
use crate::records::{DpoPairRecord, Provenance, SftRecord, VerificationResult};

/// §3 legs shared by both record types: a labeled negative fixture must
/// never be admitted, and a recorded critical failure is only coherent on a
/// labeled negative fixture — a critical failure on an ordinary record is a
/// hard reject, not a valid datum.
fn critical_failure_errors(
    is_negative_fixture: bool,
    verification: Option<&VerificationResult>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(v) = verification {
        if is_negative_fixture && v.status == VerificationStatus::Admitted {
            errors.push(
                "negative fixture must never have verification.status = admitted (contract §3)"
                    .to_string(),
            );
        }
        if !is_negative_fixture && v.critical_failure.is_some() {
            errors.push(
                "verification.critical_failure may only be recorded on a labeled negative fixture (contract §3)"
                    .to_string(),
            );
        }
    }
    errors
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

fn calendar_date_error(field: &str, date: &str) -> Option<String> {
    (!valid_calendar_date(date)).then(|| format!("{field} {date:?} is not a real calendar date"))
}

fn provenance_errors(provenance: &Provenance) -> Option<String> {
    calendar_date_error("provenance.generated_at", &provenance.generated_at)
}

fn verification_date_error(verification: &VerificationResult) -> Option<String> {
    verification
        .verified_at
        .as_deref()
        .and_then(|date| calendar_date_error("verification.verified_at", date))
}

/// The fields both record types share — the shared rules take this view
/// rather than six loose arguments.
struct SharedView<'a> {
    scenario_id: &'a str,
    family: Family,
    primary_competency: Competency,
    is_negative_fixture: bool,
    provenance: &'a Provenance,
    verification: Option<&'a VerificationResult>,
}

/// Both record types expose the identical shared field set — the impl is
/// written once per type via macro rather than copy-pasted.
macro_rules! impl_shared_view {
    ($($record:ty),+) => {
        $(
            impl<'a> From<&'a $record> for SharedView<'a> {
                fn from(record: &'a $record) -> Self {
                    Self {
                        scenario_id: &record.scenario_id,
                        family: record.family,
                        primary_competency: record.primary_competency,
                        is_negative_fixture: record.is_negative_fixture,
                        provenance: &record.provenance,
                        verification: record.verification.as_ref(),
                    }
                }
            }
        )+
    };
}
impl_shared_view!(SftRecord, DpoPairRecord);

fn shared_errors(view: &SharedView<'_>) -> Vec<String> {
    let mut errors = critical_failure_errors(view.is_negative_fixture, view.verification);
    errors.extend(
        [
            family_code_error(view.scenario_id, view.family),
            provenance_errors(view.provenance),
            view.verification.and_then(verification_date_error),
            if view
                .family
                .primary_pool()
                .contains(&view.primary_competency)
            {
                None
            } else {
                Some(format!(
                    "primary_competency {:?} is outside the {:?} pool",
                    view.primary_competency, view.family
                ))
            },
        ]
        .into_iter()
        .flatten(),
    );
    errors
}

/// A labeled negative fixture must name the rule expected to reject it
/// (D7); an empty citation is not a label, and the field is meaningless on
/// a positive record.
fn expected_rejection_error(record: &SftRecord) -> Option<String> {
    match record.expected_rejection.as_deref().map(str::trim) {
        _ if !record.is_negative_fixture => record.expected_rejection.is_some().then(|| {
            "expected_rejection may only be present on a labeled negative fixture".to_string()
        }),
        Some(citation) if !citation.is_empty() => None,
        Some(_) => {
            Some("negative fixture must carry a non-empty expected_rejection citation".to_string())
        }
        None => Some(
            "negative fixture must carry expected_rejection (contract §3 labeled negative fixture)"
                .to_string(),
        ),
    }
}

fn secondary_competency_errors(record: &SftRecord) -> Vec<String> {
    let secondaries = &record.secondary_competencies;
    let mut errors = Vec::new();
    if secondaries.contains(&record.primary_competency) {
        errors.push("secondary_competencies must not contain primary_competency".to_string());
    }
    if secondaries.iter().collect::<HashSet<_>>().len() != secondaries.len() {
        errors.push("secondary_competencies must be unique".to_string());
    }
    if secondaries.len() > 2 {
        errors.push("secondary_competencies allows at most 2 entries".to_string());
    }
    let pool = record.family.secondary_pool();
    for competency in secondaries {
        if !pool.contains(competency) {
            errors.push(format!(
                "secondary_competency {competency:?} is outside the {:?} pool",
                record.family
            ));
        }
    }
    errors
}

/// Cross-field rules for an [`SftRecord`] (D4, D7–D9, plus the taxonomy's
/// secondary-pool rule).
pub fn sft_cross_field_errors(record: &SftRecord) -> Vec<String> {
    let mut errors = shared_errors(&record.into());
    errors.extend(expected_rejection_error(record));
    errors.extend(secondary_competency_errors(record));
    errors
}

/// Cross-field rules for a [`DpoPairRecord`] (D5, D7–D9).
pub fn dpo_cross_field_errors(record: &DpoPairRecord) -> Vec<String> {
    // The DPO record has no expected_rejection field (D5): a pair labels a
    // §3-violating rejected side with is_negative_fixture, so only the
    // "never admitted" leg of D7 applies.
    let mut errors = shared_errors(&record.into());

    if record.chosen == record.rejected {
        errors.push("chosen and rejected must differ".to_string());
    }
    errors
}
