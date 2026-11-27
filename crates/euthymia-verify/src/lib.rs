//! Verification and admission pipeline for SFT records.
//!
//! Admission is deterministic in v1: schema validation reuses
//! `euthymia-schema`, privacy and §3 critical-failure screens are lexical and
//! deterministic, and rubric scores come through the [`Scorer`] interface.
//! The shipped scorer is file-based; no judge model is pinned here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use euthymia_schema::enums::CriticalFailure;
use euthymia_schema::records::{
    CONTRACT_VERSION, CURRENT_SCHEMA_VERSION, SftRecord, VerificationResult,
};
use euthymia_schema::validate::EXPECTED_REJECTION_CITATIONS;
use euthymia_schema::validator::{RecordKind, RecordValidator, record_id};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PIPELINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const ADMIT_THRESHOLD: u8 = 4;

/// Machine-readable rejection rules. These are report vocabulary, distinct
/// from the contract citations stored in `expected_rejection` on fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionRule {
    SchemaInvalid,
    PrivacyViolation,
    RevengeOrHarassment,
    ShameBasedDiscipline,
    EnablingSubstanceUse,
    FosteringAiDependence,
    RubricBelowThreshold,
}

impl AdmissionRule {
    pub const ALL: [Self; 7] = [
        Self::SchemaInvalid,
        Self::PrivacyViolation,
        Self::RevengeOrHarassment,
        Self::ShameBasedDiscipline,
        Self::EnablingSubstanceUse,
        Self::FosteringAiDependence,
        Self::RubricBelowThreshold,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::SchemaInvalid => "schema_invalid",
            Self::PrivacyViolation => "privacy_violation",
            Self::RevengeOrHarassment => "revenge_or_harassment",
            Self::ShameBasedDiscipline => "shame_based_discipline",
            Self::EnablingSubstanceUse => "enabling_substance_use",
            Self::FosteringAiDependence => "fostering_ai_dependence",
            Self::RubricBelowThreshold => "rubric_below_threshold",
        }
    }

    pub fn from_critical_failure(failure: CriticalFailure) -> Self {
        match failure {
            CriticalFailure::RevengeOrHarassment => Self::RevengeOrHarassment,
            CriticalFailure::ShameBasedDiscipline => Self::ShameBasedDiscipline,
            CriticalFailure::EnablingSubstanceUse => Self::EnablingSubstanceUse,
            CriticalFailure::FosteringAiDependence => Self::FosteringAiDependence,
        }
    }

    pub fn critical_failure(self) -> Option<CriticalFailure> {
        match self {
            Self::RevengeOrHarassment => Some(CriticalFailure::RevengeOrHarassment),
            Self::ShameBasedDiscipline => Some(CriticalFailure::ShameBasedDiscipline),
            Self::EnablingSubstanceUse => Some(CriticalFailure::EnablingSubstanceUse),
            Self::FosteringAiDependence => Some(CriticalFailure::FosteringAiDependence),
            _ => None,
        }
    }
}

/// One row in the file-based scorer input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreEntry {
    pub scenario_id: String,
    pub score: u8,
    #[serde(default)]
    pub critical_failure: Option<CriticalFailure>,
    pub scorer_id: String,
    pub rationale: String,
}

/// Deterministic scoring interface. Implementations may be backed by files,
/// reviewing agents, or judge models without changing the admission stages.
pub trait Scorer: Sync {
    fn score(&self, scenario_id: &str) -> Option<&ScoreEntry>;

    /// Where the scores came from, when the implementation is file-backed.
    /// The report carries this hash because score content is admission input.
    fn source(&self) -> Option<InputFileReport> {
        None
    }
}

/// File-based scorer: a JSONL file keyed by `scenario_id`.
pub struct FileScorer {
    entries: HashMap<String, ScoreEntry>,
    source: InputFileReport,
}

impl FileScorer {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read score file {}", path.display()))?;
        let source = InputFileReport {
            path: path.display().to_string(),
            sha256: sha256_bytes(&bytes),
        };
        let text = String::from_utf8(bytes)
            .with_context(|| format!("score file {} is not valid UTF-8", path.display()))?;
        let mut entries = HashMap::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: ScoreEntry = serde_json::from_str(line).with_context(|| {
                format!("{}:{}: invalid score-entry JSON", path.display(), index + 1)
            })?;
            ensure!(
                (1..=5).contains(&entry.score),
                "{}:{}: score must be in 1..=5",
                path.display(),
                index + 1
            );
            ensure!(
                entries
                    .insert(entry.scenario_id.clone(), entry.clone())
                    .is_none(),
                "{}:{}: duplicate score for {}",
                path.display(),
                index + 1,
                entry.scenario_id
            );
        }
        Ok(Self { entries, source })
    }
}

impl Scorer for FileScorer {
    fn score(&self, scenario_id: &str) -> Option<&ScoreEntry> {
        self.entries.get(scenario_id)
    }

    fn source(&self) -> Option<InputFileReport> {
        Some(self.source.clone())
    }
}

/// A batch row before admission.
#[derive(Debug)]
struct BatchLine {
    input_path: PathBuf,
    line_number: usize,
    line: String,
    scenario_id: String,
}

/// Per-input-file provenance in the admission report.
#[derive(Debug, Clone, Serialize)]
pub struct InputFileReport {
    pub path: String,
    pub sha256: String,
}

/// Per-row admission verdict.
#[derive(Debug, Clone, Serialize)]
pub struct RowVerdict {
    pub verdict: VerificationStatusName,
    pub rule: Option<AdmissionRule>,
    pub detail: String,
    pub rubric_score: Option<u8>,
    pub critical_failure: Option<CriticalFailure>,
    pub privacy_pass: Option<bool>,
    pub scorer_id: Option<String>,
    pub scorer_rationale: Option<String>,
    pub output_written: bool,
    pub contract_sections_evaluated: Vec<String>,
}

/// Serde uses the record enum for rows; the report uses a simple stable name.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatusName {
    Admitted,
    Rejected,
}

/// Machine-readable admission report for one batch.
#[derive(Debug, Clone, Serialize)]
pub struct AdmissionReport {
    pub batch: String,
    pub pipeline_version: String,
    pub schema_version: u32,
    pub contract_version: String,
    pub contract_sections_evaluated: Vec<String>,
    pub input_files: Vec<InputFileReport>,
    pub score_file: Option<InputFileReport>,
    pub counts: AdmissionCounts,
    pub rows: BTreeMap<String, RowVerdict>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdmissionCounts {
    pub input_rows: usize,
    pub admitted: usize,
    pub rejected: usize,
    pub per_rule: BTreeMap<String, usize>,
}

/// Admission output locations.
pub struct AdmissionOutputs {
    pub admitted: PathBuf,
    pub rejected: PathBuf,
    pub report: PathBuf,
}

fn contract_sections() -> Vec<String> {
    [
        "§1 Six Competencies",
        "§3 Critical Failures",
        "§4 Privacy Rule",
        "§5 Positive Targets",
    ]
    .into_iter()
    .map(|section| format!("{section} @ {CONTRACT_VERSION}"))
    .collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_batch(paths: &[PathBuf]) -> Result<(Vec<BatchLine>, Vec<InputFileReport>)> {
    let mut rows = Vec::new();
    let mut files = Vec::new();
    for path in paths {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read input {}", path.display()))?;
        files.push(InputFileReport {
            path: path.display().to_string(),
            sha256: sha256_bytes(&bytes),
        });
        let text = String::from_utf8(bytes)
            .with_context(|| format!("input {} is not valid UTF-8", path.display()))?;
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let scenario_id = record_id(line, RecordKind::Sft)
                .unwrap_or_else(|| format!("<unidentified:{}:{}>", path.display(), index + 1));
            rows.push(BatchLine {
                input_path: path.clone(),
                line_number: index + 1,
                line: line.to_string(),
                scenario_id,
            });
        }
    }

    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    for row in &rows {
        if !seen.insert(row.scenario_id.clone()) {
            duplicates.push(row.scenario_id.clone());
        }
    }
    ensure!(
        duplicates.is_empty(),
        "batch contains duplicate scenario_id values: {}",
        duplicates.join(", ")
    );
    Ok((rows, files))
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '“' | '”'
            )
    })
    .filter(|token| !token.is_empty())
}

fn trim_punctuation(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ':'))
}

fn looks_like_email(token: &str) -> bool {
    let trimmed = trim_punctuation(token);
    let Some((local, domain)) = trimmed.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

fn looks_like_handle(token: &str) -> bool {
    let trimmed = trim_punctuation(token);
    trimmed.starts_with('@') && trimmed.len() > 1 && !trimmed[1..].contains('@')
}

fn looks_like_phone(token: &str) -> bool {
    let trimmed = trim_punctuation(token);
    let digits = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
    let phone_chars = trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'x' | 'X'));
    if !phone_chars || digits < 7 {
        return false;
    }
    // ISO dates are numeric metadata or ordinary date references, not phone
    // numbers; contiguous 10–11 digits or a separator-bearing token otherwise
    // receives the conservative privacy treatment.
    let iso_date = trimmed.len() == 10
        && trimmed.chars().enumerate().all(|(index, c)| {
            if matches!(index, 4 | 7) {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    !iso_date
}

/// Deterministic §4 identifier screen over prompt and response text. This is
/// deliberately conservative and mechanical; generalized roles and ordinary
/// emotional content do not trip it.
pub fn privacy_findings(user_message: &str, response: &str) -> Vec<&'static str> {
    let text = format!("{user_message}\n{response}");
    let lower = text.to_lowercase();
    let mut findings = Vec::new();

    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        findings.push("url");
    }
    for token in tokens(&text) {
        if looks_like_email(token) {
            findings.push("email");
        } else if looks_like_handle(token) {
            findings.push("handle");
        } else if looks_like_phone(token) {
            findings.push("phone");
        }
    }
    for (phrase, finding) in [
        ("my name is ", "self-identification"),
        ("my address is ", "self-identification"),
        ("i live at ", "self-identification"),
        ("my email is ", "self-identification"),
        ("my phone number is ", "self-identification"),
        ("call me at ", "self-identification"),
    ] {
        if lower.contains(phrase) {
            findings.push(finding);
        }
    }
    findings.sort_unstable();
    findings.dedup();
    findings
}

fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn expected_negative_fixture_failure(record: &SftRecord) -> Option<CriticalFailure> {
    match record.expected_rejection.as_deref() {
        Some(citation) if citation == EXPECTED_REJECTION_CITATIONS[0] => {
            Some(CriticalFailure::RevengeOrHarassment)
        }
        Some(citation) if citation == EXPECTED_REJECTION_CITATIONS[1] => {
            Some(CriticalFailure::ShameBasedDiscipline)
        }
        Some(citation) if citation == EXPECTED_REJECTION_CITATIONS[2] => {
            Some(CriticalFailure::EnablingSubstanceUse)
        }
        Some(citation) if citation == EXPECTED_REJECTION_CITATIONS[3] => {
            Some(CriticalFailure::FosteringAiDependence)
        }
        _ => None,
    }
}

/// Lexical first-pass §3 screen over the record's response voice. User
/// material is intentionally not screened here: a user may describe revenge,
/// shame, craving, or loneliness without the record endorsing it.
pub fn lexical_critical_failure(record: &SftRecord) -> Option<CriticalFailure> {
    let text = record.response.to_lowercase();

    if contains_any(
        &text,
        &[
            "post the thread publicly",
            "publicly call them out",
            "publicly call out",
            "tag the team",
            "shame them into",
            "make them pay",
            "get revenge",
            "retaliate against",
            "expose them publicly",
            "harass them",
        ],
    ) {
        return Some(CriticalFailure::RevengeOrHarassment);
    }
    if contains_any(
        &text,
        &[
            "you are lazy",
            "pathetic when",
            "use that shame as fuel",
            "you should be ashamed",
            "you are worthless",
            "worthless unless",
            "shame yourself",
            "sit in how pathetic",
        ],
    ) {
        return Some(CriticalFailure::ShameBasedDiscipline);
    }
    if contains_any(
        &text,
        &[
            "one drink will not really count as relapse",
            "one drink won't hurt",
            "keep it controlled",
            "controlled drinking",
            "pressure valve",
            "just one drink won't hurt",
            "a little won't hurt",
        ],
    ) {
        return Some(CriticalFailure::EnablingSubstanceUse);
    }
    if contains_any(
        &text,
        &[
            "come back to me every night",
            "main person you lean on",
            "instead of risking disappointment with friends or family",
            "i am always here for you",
            "i'm always here for you",
            "you don't need anyone else",
            "you do not need anyone else",
            "rely on me instead",
        ],
    ) {
        return Some(CriticalFailure::FosteringAiDependence);
    }
    None
}

#[derive(Debug)]
struct Decision {
    scenario_id: String,
    record: Option<SftRecord>,
    verdict: VerificationStatusName,
    rule: Option<AdmissionRule>,
    detail: String,
    rubric_score: Option<u8>,
    critical_failure: Option<CriticalFailure>,
    privacy_pass: Option<bool>,
    scorer_id: Option<String>,
    scorer_rationale: Option<String>,
}

fn decision_record(
    mut record: SftRecord,
    status: euthymia_schema::enums::VerificationStatus,
    rule: Option<AdmissionRule>,
    rubric_score: Option<u8>,
    privacy_pass: Option<bool>,
    scorer_id: Option<&str>,
    run_date: &str,
) -> SftRecord {
    let critical_failure = rule.and_then(AdmissionRule::critical_failure);
    record.verification = Some(VerificationResult {
        status,
        rubric_score,
        critical_failure,
        privacy_pass,
        verified_by: Some(match scorer_id {
            Some(scorer) => format!("euthymia-admit {PIPELINE_VERSION} + scorer {scorer}"),
            None => format!("euthymia-admit {PIPELINE_VERSION}"),
        }),
        verified_at: Some(run_date.to_string()),
    });
    record
}

fn reject(
    record: SftRecord,
    rule: AdmissionRule,
    detail: String,
    rubric_score: Option<u8>,
    privacy_pass: Option<bool>,
    scorer_id: Option<&str>,
    scorer_rationale: Option<&str>,
    run_date: &str,
) -> Decision {
    let scenario_id = record.scenario_id.clone();
    let critical_failure = rule.critical_failure();
    Decision {
        scenario_id,
        record: Some(decision_record(
            record,
            euthymia_schema::enums::VerificationStatus::Rejected,
            Some(rule),
            rubric_score,
            privacy_pass,
            scorer_id,
            run_date,
        )),
        verdict: VerificationStatusName::Rejected,
        rule: Some(rule),
        detail,
        rubric_score,
        critical_failure,
        privacy_pass,
        scorer_id: scorer_id.map(str::to_string),
        scorer_rationale: scorer_rationale.map(str::to_string),
    }
}

fn schema_invalid_decision(row: &BatchLine, errors: Vec<String>, run_date: &str) -> Decision {
    let detail = format!(
        "{}:{}: {}",
        row.input_path.display(),
        row.line_number,
        errors.join("; ")
    );
    if let Ok(record) = serde_json::from_str::<SftRecord>(&row.line) {
        return reject(
            record,
            AdmissionRule::SchemaInvalid,
            detail,
            None,
            None,
            None,
            None,
            run_date,
        );
    }
    Decision {
        scenario_id: row.scenario_id.clone(),
        record: None,
        verdict: VerificationStatusName::Rejected,
        rule: Some(AdmissionRule::SchemaInvalid),
        detail,
        rubric_score: None,
        critical_failure: None,
        privacy_pass: None,
        scorer_id: None,
        scorer_rationale: None,
    }
}

fn decide_row(
    row: &BatchLine,
    validator: &RecordValidator,
    scorer: &impl Scorer,
    run_date: &str,
) -> Decision {
    let schema_errors = validator.validate_line(&row.line);
    if !schema_errors.is_empty() {
        return schema_invalid_decision(row, schema_errors, run_date);
    }

    let record: SftRecord = serde_json::from_str(&row.line)
        .expect("a line passing schema validation deserializes as SftRecord");

    let privacy = privacy_findings(&record.user_message, &record.response);
    if !privacy.is_empty() {
        return reject(
            record,
            AdmissionRule::PrivacyViolation,
            format!("privacy identifiers detected: {}", privacy.join(", ")),
            None,
            Some(false),
            None,
            None,
            run_date,
        );
    }

    if let Some(failure) = lexical_critical_failure(&record) {
        let rule = AdmissionRule::from_critical_failure(failure);
        return reject(
            record,
            rule,
            format!("lexical §3 critical-failure screen matched {}", rule.id()),
            None,
            Some(true),
            None,
            None,
            run_date,
        );
    }

    // A labeled negative fixture is reject-only by contract even if the
    // lexical screen misses its particular wording. Current fixtures are still
    // expected to be caught lexically above; this mapping is the fail-closed
    // guard, not the calibration signal.
    if record.is_negative_fixture
        && let Some(failure) = expected_negative_fixture_failure(&record)
    {
        let rule = AdmissionRule::from_critical_failure(failure);
        return reject(
            record,
            rule,
            format!("labeled negative fixture fail-closed as {}", rule.id()),
            None,
            Some(true),
            None,
            None,
            run_date,
        );
    }

    let Some(score) = scorer.score(&record.scenario_id) else {
        return reject(
            record,
            AdmissionRule::RubricBelowThreshold,
            "unscored".to_string(),
            None,
            Some(true),
            None,
            None,
            run_date,
        );
    };
    if let Some(failure) = score.critical_failure {
        let rule = AdmissionRule::from_critical_failure(failure);
        return reject(
            record,
            rule,
            format!("scorer {} raised {}", score.scorer_id, rule.id()),
            Some(score.score),
            Some(true),
            Some(&score.scorer_id),
            Some(&score.rationale),
            run_date,
        );
    }
    if score.score < ADMIT_THRESHOLD {
        return reject(
            record,
            AdmissionRule::RubricBelowThreshold,
            format!(
                "score {} below admission threshold {ADMIT_THRESHOLD}",
                score.score
            ),
            Some(score.score),
            Some(true),
            Some(&score.scorer_id),
            Some(&score.rationale),
            run_date,
        );
    }

    let scenario_id = record.scenario_id.clone();
    let admitted = decision_record(
        record,
        euthymia_schema::enums::VerificationStatus::Admitted,
        None,
        Some(score.score),
        Some(true),
        Some(&score.scorer_id),
        run_date,
    );
    Decision {
        scenario_id,
        record: Some(admitted),
        verdict: VerificationStatusName::Admitted,
        rule: None,
        detail: format!("admitted at rubric score {}", score.score),
        rubric_score: Some(score.score),
        critical_failure: None,
        privacy_pass: Some(true),
        scorer_id: Some(score.scorer_id.clone()),
        scorer_rationale: Some(score.rationale.clone()),
    }
}

fn write_jsonl(path: &Path, records: &[SftRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut out = String::new();
    for record in records {
        out.push_str(&serde_json::to_string(record)?);
        out.push('\n');
    }
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))
}

/// Run admission for one batch and write admitted rows, rejected rows, and a
/// machine-readable report.
pub fn admit_batch(
    inputs: &[PathBuf],
    scorer: &impl Scorer,
    batch: &str,
    output_root: &Path,
    report_path: &Path,
) -> Result<AdmissionOutputs> {
    ensure!(
        !inputs.is_empty(),
        "at least one input JSONL file is required"
    );
    ensure!(
        batch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
        "batch name may contain only ASCII letters, digits, '-' and '_'"
    );

    let (rows, input_files) = read_batch(inputs)?;
    let validator = RecordValidator::new(RecordKind::Sft)?;
    let run_date = today_utc();
    let decisions: Vec<Decision> = rows
        .par_iter()
        .map(|row| decide_row(row, &validator, scorer, &run_date))
        .collect();

    let mut admitted = Vec::new();
    let mut rejected = Vec::new();
    let mut report_rows = BTreeMap::new();
    let mut per_rule: BTreeMap<String, usize> = AdmissionRule::ALL
        .into_iter()
        .map(|rule| (rule.id().to_string(), 0))
        .collect();
    let mut admitted_count = 0usize;
    let mut rejected_count = 0usize;

    for decision in decisions {
        let output_written = decision.record.is_some();
        match decision.verdict {
            VerificationStatusName::Admitted => {
                admitted_count += 1;
                if let Some(record) = decision.record.clone() {
                    admitted.push(record);
                }
            }
            VerificationStatusName::Rejected => {
                rejected_count += 1;
                if let Some(rule) = decision.rule {
                    *per_rule.entry(rule.id().to_string()).or_insert(0) += 1;
                }
                if let Some(record) = decision.record.clone() {
                    rejected.push(record);
                }
            }
        }
        report_rows.insert(
            decision.scenario_id.clone(),
            RowVerdict {
                verdict: decision.verdict,
                rule: decision.rule,
                detail: decision.detail,
                rubric_score: decision.rubric_score,
                critical_failure: decision.critical_failure,
                privacy_pass: decision.privacy_pass,
                scorer_id: decision.scorer_id,
                scorer_rationale: decision.scorer_rationale,
                output_written,
                contract_sections_evaluated: contract_sections(),
            },
        );
    }

    let report = AdmissionReport {
        batch: batch.to_string(),
        pipeline_version: format!("euthymia-admit {PIPELINE_VERSION}"),
        schema_version: CURRENT_SCHEMA_VERSION,
        contract_version: CONTRACT_VERSION.to_string(),
        contract_sections_evaluated: contract_sections(),
        input_files,
        score_file: scorer.source(),
        counts: AdmissionCounts {
            input_rows: rows.len(),
            admitted: admitted_count,
            rejected: rejected_count,
            per_rule,
        },
        rows: report_rows,
    };

    let admitted_path = output_root.join("admitted").join(format!("{batch}.jsonl"));
    let rejected_path = output_root.join("rejected").join(format!("{batch}.jsonl"));
    write_jsonl(&admitted_path, &admitted)?;
    write_jsonl(&rejected_path, &rejected)?;
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let report_json = serde_json::to_string_pretty(&report)? + "\n";
    fs::write(report_path, report_json)
        .with_context(|| format!("failed to write {}", report_path.display()))?;

    Ok(AdmissionOutputs {
        admitted: admitted_path,
        rejected: rejected_path,
        report: report_path.to_path_buf(),
    })
}

/// Current UTC date as `YYYY-MM-DD`, implemented without a clock dependency
/// so admission remains small and deterministic in tests on a given day.
pub fn today_utc() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

/// Convenience for CLI error messages when a path is absent.
pub fn require_exists(path: &Path, what: &str) -> Result<()> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            bail!("{what} {} does not exist", path.display())
        }
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
