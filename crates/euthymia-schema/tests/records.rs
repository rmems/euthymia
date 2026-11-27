//! Record-level tests: §3 negative-fixture labels survive serialization
//! unchanged (D7 round-trip), and out-of-version data fails to deserialize
//! (D2).

use euthymia_schema::validate::{dpo_cross_field_errors, sft_cross_field_errors};
use euthymia_schema::*;
use serde_json::json;

fn base_sft() -> SftRecord {
    SftRecord {
        schema_version: SchemaVersion,
        scenario_id: "eu-bnd-003".to_string(),
        family: Family::Boundaries,
        primary_competency: Competency::HealthyBoundaries,
        secondary_competencies: vec![Competency::EmotionalAwareness],
        difficulty: Difficulty::Everyday,
        life_stage: LifeStage::Age25To34,
        relationship_context: RelationshipContext::FamilyMember,
        setting: Setting::Home,
        user_message: "A family member asked for another favor when I already need rest."
            .to_string(),
        response: "You are allowed to decline without managing their disappointment.".to_string(),
        provenance: Provenance {
            generator_kind: GeneratorKind::Procedural,
            generator_id: "template-set-boundaries-v1".to_string(),
            prompt_version: "sft-prompt-0.1.0".to_string(),
            generated_at: "2026-11-18".to_string(),
            seed: Some(7),
        },
        contract_version: ContractVersion,
        is_negative_fixture: false,
        expected_rejection: None,
        verification: None,
    }
}

#[test]
fn negative_fixture_labels_survive_round_trip() {
    let mut record = base_sft();
    record.is_negative_fixture = true;
    record.expected_rejection = Some("§3.2 Shame-Based Discipline @ V0.1.0".to_string());
    record.verification = Some(VerificationResult {
        status: VerificationStatus::Rejected,
        rubric_score: None,
        critical_failure: Some(CriticalFailure::ShameBasedDiscipline),
        privacy_pass: Some(true),
        verified_by: Some("fixture-review".to_string()),
        verified_at: Some("2026-11-20".to_string()),
    });

    let json = serde_json::to_string(&record).expect("serializes");
    assert!(json.contains("\"is_negative_fixture\":true"));
    assert!(json.contains("\"expected_rejection\":\"§3.2 Shame-Based Discipline @ V0.1.0\""));

    let back: SftRecord = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, record, "labels must be preserved end-to-end");
    assert!(back.is_negative_fixture);
    assert_eq!(
        back.expected_rejection.as_deref(),
        Some("§3.2 Shame-Based Discipline @ V0.1.0")
    );
}

#[test]
fn positive_record_serializes_without_expected_rejection_key() {
    let json = serde_json::to_string(&base_sft()).expect("serializes");
    assert!(!json.contains("expected_rejection"));
}

#[test]
fn rejected_record_may_carry_critical_failure_diagnostic() {
    let mut record = base_sft();
    record.verification = Some(VerificationResult {
        status: VerificationStatus::Rejected,
        rubric_score: None,
        critical_failure: Some(CriticalFailure::FosteringAiDependence),
        privacy_pass: Some(true),
        verified_by: Some("admission-test".to_string()),
        verified_at: Some("2026-11-27".to_string()),
    });
    assert_eq!(sft_cross_field_errors(&record), Vec::<String>::new());
}

#[test]
fn schema_version_two_fails_to_deserialize() {
    let mut value = serde_json::to_value(base_sft()).expect("serializes");
    value["schema_version"] = json!(2);
    let err = serde_json::from_value::<SftRecord>(value).expect_err("version 2 must fail");
    assert!(
        err.to_string().contains("schema_version"),
        "unexpected error: {err}"
    );
}

#[test]
fn wrong_contract_version_fails_to_deserialize() {
    let mut value = serde_json::to_value(base_sft()).expect("serializes");
    value["contract_version"] = json!("V0.2.0");
    serde_json::from_value::<SftRecord>(value).expect_err("contract_version must be pinned");
}

#[test]
fn unknown_field_fails_to_deserialize() {
    let mut value = serde_json::to_value(base_sft()).expect("serializes");
    value["reviewer_notes"] = json!("x");
    serde_json::from_value::<SftRecord>(value).expect_err("deny_unknown_fields must reject it");
}

#[test]
fn cross_field_rules() {
    let mut record = base_sft();

    record.secondary_competencies = vec![Competency::HealthyBoundaries];
    assert!(
        sft_cross_field_errors(&record)
            .iter()
            .any(|e| e.contains("must not contain primary_competency"))
    );

    let mut record = base_sft();
    record.secondary_competencies = vec![
        Competency::EmotionalAwareness,
        Competency::EmotionalAwareness,
    ];
    assert!(
        sft_cross_field_errors(&record)
            .iter()
            .any(|e| e.contains("must be unique"))
    );

    let mut record = base_sft();
    record.is_negative_fixture = true;
    record.verification = Some(VerificationResult {
        status: VerificationStatus::Admitted,
        rubric_score: Some(5),
        critical_failure: None,
        privacy_pass: Some(true),
        verified_by: None,
        verified_at: None,
    });
    let errors = sft_cross_field_errors(&record);
    assert!(errors.iter().any(|e| e.contains("expected_rejection")));
    assert!(errors.iter().any(|e| e.contains("admitted")));
}

#[test]
fn dpo_chosen_must_differ_from_rejected() {
    let record = DpoPairRecord {
        schema_version: SchemaVersion,
        pair_id: "eu-dpo-003".to_string(),
        scenario_id: "eu-fal-010".to_string(),
        family: Family::FailureSetbacks,
        primary_competency: Competency::PersonalAccountability,
        difficulty: Difficulty::Moderate,
        prompt: "A project I worked on was rejected and I keep replaying it.".to_string(),
        chosen: "same".to_string(),
        rejected: "same".to_string(),
        rejected_failure_mode: FailureMode::Shaming,
        provenance: Provenance {
            generator_kind: GeneratorKind::Teacher,
            generator_id: "fictional-teacher-v1".to_string(),
            prompt_version: "dpo-prompt-0.1.0".to_string(),
            generated_at: "2026-11-18".to_string(),
            seed: None,
        },
        contract_version: ContractVersion,
        is_negative_fixture: false,
        verification: None,
    };
    assert!(
        dpo_cross_field_errors(&record)
            .iter()
            .any(|e| e.contains("chosen and rejected must differ"))
    );
}
