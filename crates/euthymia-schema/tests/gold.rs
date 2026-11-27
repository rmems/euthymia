//! Gold-set structural and coverage checks. These checks cover metadata and
//! coverage only, not response behavior: behavioral admission remains the job
//! of contract review, the future verifier, and the rubric.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use euthymia_schema::{Competency, Family, SftRecord, VerificationResult, VerificationStatus};

const EA_ANCHOR: (&str, Competency) = ("eu-brk-901", Competency::EmotionalAwareness);
const CD_ANCHOR: (&str, Competency) = ("eu-dis-902", Competency::CognitiveDiscipline);
const PA_ANCHOR: (&str, Competency) = ("eu-dis-901", Competency::PersonalAccountability);
const BC_ANCHOR: (&str, Competency) = ("eu-ang-901", Competency::BehavioralControl);
const HB_ANCHOR: (&str, Competency) = ("eu-bnd-901", Competency::HealthyBoundaries);
const LTR_ANCHOR: (&str, Competency) = ("eu-fal-901", Competency::LongTermResilience);

const ANCHORS: [(&str, Competency); 6] = [
    EA_ANCHOR, CD_ANCHOR, PA_ANCHOR, BC_ANCHOR, HB_ANCHOR, LTR_ANCHOR,
];

const NEGATIVE_SHAME_ID: &str = "eu-dis-903";
const NEGATIVE_SUBSTANCE_ID: &str = "eu-rec-903";
const NEGATIVE_AI_DEPENDENCE_ID: &str = "eu-brk-903";
const NEGATIVE_REVENGE_ID: &str = "eu-bnd-903";
const NEGATIVE_IDS: [&str; 4] = [
    NEGATIVE_SHAME_ID,
    NEGATIVE_SUBSTANCE_ID,
    NEGATIVE_AI_DEPENDENCE_ID,
    NEGATIVE_REVENGE_ID,
];

const ALL_FAMILIES: [Family; 7] = [
    Family::Breakups,
    Family::Anger,
    Family::DisciplineMotivation,
    Family::Recovery,
    Family::Jealousy,
    Family::Boundaries,
    Family::FailureSetbacks,
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn gold_dir() -> PathBuf {
    workspace_root().join("data").join("gold")
}

fn gold_file(file: &str) -> PathBuf {
    gold_dir().join(file)
}

fn load_sft(file: &str) -> Vec<SftRecord> {
    let path = gold_file(file);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} should be readable: {err}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<SftRecord>(line)
                .unwrap_or_else(|err| panic!("{} has an invalid SftRecord: {err}", path.display()))
        })
        .collect()
}

fn verification(record: &SftRecord) -> &VerificationResult {
    record
        .verification
        .as_ref()
        .unwrap_or_else(|| panic!("{} must carry verification", record.scenario_id))
}

fn assert_accepted_metadata(record: &SftRecord) {
    assert!(
        !record.is_negative_fixture,
        "{} must not be a negative fixture",
        record.scenario_id
    );
    assert_eq!(
        verification(record).status,
        VerificationStatus::Admitted,
        "{} must be admitted",
        record.scenario_id
    );
}

fn assert_negative_metadata(record: &SftRecord) {
    assert!(
        record.is_negative_fixture,
        "{} must be labeled as a negative fixture",
        record.scenario_id
    );
    let verification = verification(record);
    assert_eq!(
        verification.status,
        VerificationStatus::Rejected,
        "{} must be rejected",
        record.scenario_id
    );
    assert!(
        verification.critical_failure.is_some(),
        "{} must record a critical_failure",
        record.scenario_id
    );
    let expected_rejection = record
        .expected_rejection
        .as_deref()
        .unwrap_or_else(|| panic!("{} must carry expected_rejection", record.scenario_id));
    assert!(
        expected_rejection.starts_with("§3."),
        "{} expected_rejection should cite §3: {expected_rejection}",
        record.scenario_id
    );
    assert!(
        expected_rejection.ends_with("@ V0.1.0"),
        "{} expected_rejection should end with @ V0.1.0: {expected_rejection}",
        record.scenario_id
    );
}

#[test]
fn gold_files_validate_with_euthymia_validate() {
    let output = Command::new(env!("CARGO_BIN_EXE_euthymia-validate"))
        .arg("sft")
        .arg(gold_file("sft_gold.jsonl"))
        .arg(gold_file("sft_gold_negative.jsonl"))
        .output()
        .expect("euthymia-validate runs");
    assert!(
        output.status.success(),
        "gold files should validate, got:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn accepted_and_negative_rows_carry_expected_verification_metadata() {
    let accepted = load_sft("sft_gold.jsonl");
    assert!(
        !accepted.is_empty(),
        "accepted gold file should not be empty"
    );
    for record in &accepted {
        assert_accepted_metadata(record);
    }

    let negatives = load_sft("sft_gold_negative.jsonl");
    assert!(
        negatives.len() >= 2,
        "at least two negative fixtures are required, found {}",
        negatives.len()
    );
    let negative_ids: HashSet<_> = negatives
        .iter()
        .map(|record| record.scenario_id.as_str())
        .collect();
    for expected_id in NEGATIVE_IDS {
        assert!(
            negative_ids.contains(expected_id),
            "negative fixture {expected_id} should be present"
        );
    }
    for record in &negatives {
        assert_negative_metadata(record);
    }
}

#[test]
fn accepted_gold_covers_every_family_and_all_anchor_competencies() {
    let accepted = load_sft("sft_gold.jsonl");
    let mut counts: HashMap<Family, usize> = HashMap::new();
    for record in &accepted {
        *counts.entry(record.family).or_insert(0) += 1;
    }

    let zero_families: Vec<_> = ALL_FAMILIES
        .iter()
        .copied()
        .filter(|family| counts.get(family).copied().unwrap_or(0) == 0)
        .collect();
    assert!(
        zero_families.is_empty(),
        "families with zero accepted gold rows: {zero_families:?}"
    );
    let underrepresented: Vec<_> = ALL_FAMILIES
        .iter()
        .copied()
        .filter(|family| counts.get(family).copied().unwrap_or(0) < 2)
        .map(|family| (family, counts.get(&family).copied().unwrap_or(0)))
        .collect();
    assert!(
        underrepresented.is_empty(),
        "families with fewer than two accepted gold rows: {underrepresented:?}"
    );

    let by_id: HashMap<_, _> = accepted
        .iter()
        .map(|record| (record.scenario_id.as_str(), record))
        .collect();
    for (anchor_id, expected_primary) in ANCHORS {
        let record = by_id
            .get(anchor_id)
            .unwrap_or_else(|| panic!("anchor {anchor_id} should be in the accepted gold file"));
        assert_eq!(
            record.primary_competency, expected_primary,
            "anchor {anchor_id} should carry primary_competency {expected_primary:?}"
        );
    }
}

#[test]
fn scenario_ids_are_unique_and_documented() {
    let accepted = load_sft("sft_gold.jsonl");
    let negatives = load_sft("sft_gold_negative.jsonl");
    let all_ids: Vec<_> = accepted
        .iter()
        .chain(&negatives)
        .map(|record| record.scenario_id.as_str())
        .collect();
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    for id in &all_ids {
        if !seen.insert(*id) {
            duplicates.push(*id);
        }
    }
    assert!(
        duplicates.is_empty(),
        "duplicate scenario_id values across both gold files: {duplicates:?}"
    );

    let notes_path = gold_file("REVIEW_NOTES.md");
    let notes = fs::read_to_string(&notes_path)
        .unwrap_or_else(|err| panic!("{} should be readable: {err}", notes_path.display()));
    let undocumented: Vec<_> = all_ids
        .iter()
        .copied()
        .filter(|id| !notes.contains(id))
        .collect();
    assert!(
        undocumented.is_empty(),
        "scenario IDs missing from REVIEW_NOTES.md: {undocumented:?}"
    );
}
