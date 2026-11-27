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
use serde_json::Value;
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
    /// SHA-256 hex of the exact JSONL line this score reviewed, excluding the
    /// trailing newline. This prevents a stale score from approving edited content.
    pub content_sha256: String,
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
                entry.content_sha256.len() == 64
                    && entry.content_sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}:{}: content_sha256 must be lowercase-or-uppercase sha256 hex",
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

/// Admission run configuration, grouped so the batch entry point stays small.
pub struct AdmissionConfig<'a> {
    pub batch: &'a str,
    pub output_root: &'a Path,
    pub report_path: &'a Path,
}

impl AdmissionConfig<'_> {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.batch
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
            "batch name may contain only ASCII letters, digits, '-' and '_'"
        );
        Ok(())
    }
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

/// Hash the exact candidate JSONL line a score reviewed, excluding the line
/// ending. Scores are valid only for this exact bytes-level content.
pub fn admission_content_sha256(line: &str) -> String {
    sha256_bytes(line.as_bytes())
}

fn read_input_file(path: &Path) -> Result<(InputFileReport, Vec<BatchLine>)> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read input {}", path.display()))?;
    let report = InputFileReport {
        path: path.display().to_string(),
        sha256: sha256_bytes(&bytes),
    };
    let text = String::from_utf8(bytes)
        .with_context(|| format!("input {} is not valid UTF-8", path.display()))?;
    let rows = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| BatchLine {
            input_path: path.to_path_buf(),
            line_number: index + 1,
            line: line.to_string(),
            scenario_id: record_id(line, RecordKind::Sft)
                .unwrap_or_else(|| format!("<unidentified:{}:{}>", path.display(), index + 1)),
        })
        .collect();
    Ok((report, rows))
}

fn ensure_unique_scenario_ids(rows: &[BatchLine]) -> Result<()> {
    let mut seen = HashSet::new();
    let duplicates: Vec<String> = rows
        .iter()
        .filter(|row| !seen.insert(row.scenario_id.clone()))
        .map(|row| row.scenario_id.clone())
        .collect();
    ensure!(
        duplicates.is_empty(),
        "batch contains duplicate scenario_id values: {}",
        duplicates.join(", ")
    );
    Ok(())
}

fn read_batch(paths: &[PathBuf]) -> Result<(Vec<BatchLine>, Vec<InputFileReport>)> {
    let mut rows = Vec::new();
    let mut files = Vec::new();
    for path in paths {
        let (report, mut file_rows) = read_input_file(path)?;
        files.push(report);
        rows.append(&mut file_rows);
    }
    ensure_unique_scenario_ids(&rows)?;
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

fn valid_email_domain(domain: &str) -> bool {
    domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

fn looks_like_email(token: &str) -> bool {
    let trimmed = trim_punctuation(token);
    match trimmed.split_once('@') {
        Some((local, domain)) => !local.is_empty() && valid_email_domain(domain),
        None => false,
    }
}

fn looks_like_handle(token: &str) -> bool {
    let trimmed = trim_punctuation(token);
    trimmed.starts_with('@') && trimmed.len() > 1 && !trimmed[1..].contains('@')
}

fn phone_chars_only(trimmed: &str) -> bool {
    trimmed.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_whitespace()
            || matches!(c, '+' | '-' | '.' | '(' | ')' | 'x' | 'X')
    })
}

fn is_iso_date_token(trimmed: &str) -> bool {
    trimmed.len() == 10
        && trimmed.chars().enumerate().all(|(index, c)| {
            if matches!(index, 4 | 7) {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn is_year_range_token(trimmed: &str) -> bool {
    let parts: Vec<&str> = trimmed.split('-').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()))
}

fn looks_like_phone(segment: &str) -> bool {
    let trimmed = trim_punctuation(segment.trim());
    let digits = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
    // ISO dates and year ranges are ordinary numeric references, not phone
    // numbers. Other digit-dense segments receive conservative treatment.
    digits >= 7
        && phone_chars_only(trimmed)
        && !is_iso_date_token(trimmed)
        && !is_year_range_token(trimmed)
}

fn phone_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_digit()
            || c.is_whitespace()
            || matches!(c, '+' | '-' | '.' | '(' | ')' | 'x' | 'X')
        {
            current.push(c);
        } else if !current.trim().is_empty() {
            segments.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        segments.push(current);
    }
    segments
}

fn phone_findings(text: &str) -> Vec<&'static str> {
    phone_segments(text)
        .iter()
        .any(|segment| looks_like_phone(segment))
        .then_some("phone")
        .into_iter()
        .collect()
}

fn identifier_finding(token: &str) -> Option<&'static str> {
    if looks_like_email(token) {
        Some("email")
    } else if looks_like_handle(token) {
        Some("handle")
    } else {
        None
    }
}

fn token_privacy_findings(text: &str) -> Vec<&'static str> {
    tokens(text).filter_map(identifier_finding).collect()
}

fn url_findings(lower: &str) -> Vec<&'static str> {
    ["http://", "https://", "www."]
        .into_iter()
        .any(|needle| lower.contains(needle))
        .then_some("url")
        .into_iter()
        .collect()
}

fn self_identification_findings(lower: &str) -> Vec<&'static str> {
    [
        "my name is ",
        "my address is ",
        "i live at ",
        "my email is ",
        "my phone number is ",
        "call me at ",
    ]
    .into_iter()
    .filter(|phrase| lower.contains(phrase))
    .map(|_| "self-identification")
    .collect()
}

/// Deterministic §4 identifier screen over prompt and response text. This is
/// deliberately conservative and mechanical; generalized roles and ordinary
/// emotional content do not trip it.
pub fn privacy_findings(user_message: &str, response: &str) -> Vec<&'static str> {
    let text = format!(
        "{user_message}
{response}"
    );
    let lower = text.to_lowercase();
    let mut findings = url_findings(&lower);
    findings.extend(token_privacy_findings(&text));
    findings.extend(phone_findings(&text));
    findings.extend(self_identification_findings(&lower));
    findings.sort_unstable();
    findings.dedup();
    findings
}

fn negated_before(text: &str, start: usize) -> bool {
    let window_start = start.saturating_sub(32);
    let window = &text[window_start..start];
    [
        "don't",
        "do not",
        "never",
        "not ",
        "avoid",
        "rather than",
        "instead of",
        "without",
    ]
    .iter()
    .any(|negation| window.contains(negation))
}

fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| {
        text.match_indices(phrase)
            .any(|(start, _)| !negated_before(text, start))
    })
}

fn expected_negative_fixture_failure(record: &SftRecord) -> Option<CriticalFailure> {
    match record.expected_rejection.as_deref().map(str::trim) {
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

#[derive(Clone)]
struct VerificationFields {
    rule: Option<AdmissionRule>,
    rubric_score: Option<u8>,
    privacy_pass: Option<bool>,
    scorer_id: Option<String>,
    scorer_rationale: Option<String>,
    run_date: String,
}

impl VerificationFields {
    fn gate(run_date: &str) -> Self {
        Self {
            rule: None,
            rubric_score: None,
            privacy_pass: None,
            scorer_id: None,
            scorer_rationale: None,
            run_date: run_date.to_string(),
        }
    }

    fn scored(score: &ScoreEntry, run_date: &str) -> Self {
        Self {
            scorer_id: Some(score.scorer_id.clone()),
            scorer_rationale: Some(score.rationale.clone()),
            rubric_score: Some(score.score),
            privacy_pass: Some(true),
            ..Self::gate(run_date)
        }
    }
}

struct RejectSpec {
    rule: AdmissionRule,
    detail: String,
    fields: VerificationFields,
}

impl RejectSpec {
    fn gate(
        rule: AdmissionRule,
        detail: String,
        privacy_pass: Option<bool>,
        run_date: &str,
    ) -> Self {
        let mut fields = VerificationFields::gate(run_date);
        fields.rule = Some(rule);
        fields.privacy_pass = privacy_pass;
        Self {
            rule,
            detail,
            fields,
        }
    }

    fn scored(rule: AdmissionRule, detail: String, score: &ScoreEntry, run_date: &str) -> Self {
        let mut fields = VerificationFields::scored(score, run_date);
        fields.rule = Some(rule);
        Self {
            rule,
            detail,
            fields,
        }
    }
}

fn decision_record(
    mut record: SftRecord,
    status: euthymia_schema::enums::VerificationStatus,
    fields: VerificationFields,
) -> SftRecord {
    let critical_failure = fields.rule.and_then(AdmissionRule::critical_failure);
    record.verification = Some(VerificationResult {
        status,
        rubric_score: fields.rubric_score,
        critical_failure,
        privacy_pass: fields.privacy_pass,
        verified_by: Some(match fields.scorer_id.as_deref() {
            Some(scorer) => format!("euthymia-admit {PIPELINE_VERSION} + scorer {scorer}"),
            None => format!("euthymia-admit {PIPELINE_VERSION}"),
        }),
        verified_at: Some(fields.run_date),
    });
    record
}

fn reject(mut record: SftRecord, spec: RejectSpec, run_date: &str) -> Decision {
    if spec.rule == AdmissionRule::PrivacyViolation {
        record.user_message = "[redacted: privacy_violation]".to_string();
        record.response = "[redacted: privacy_violation]".to_string();
    }
    let scenario_id = record.scenario_id.clone();
    let fields = VerificationFields {
        run_date: run_date.to_string(),
        ..spec.fields
    };
    let critical_failure = spec.rule.critical_failure();
    let output = decision_record(
        record,
        euthymia_schema::enums::VerificationStatus::Rejected,
        fields.clone(),
    );
    Decision {
        scenario_id,
        record: Some(output),
        verdict: VerificationStatusName::Rejected,
        rule: Some(spec.rule),
        detail: spec.detail,
        rubric_score: fields.rubric_score,
        critical_failure,
        privacy_pass: fields.privacy_pass,
        scorer_id: fields.scorer_id,
        scorer_rationale: fields.scorer_rationale,
    }
}

fn schema_invalid_spec(errors: Vec<String>, row: &BatchLine, run_date: &str) -> RejectSpec {
    RejectSpec::gate(
        AdmissionRule::SchemaInvalid,
        format!(
            "{}:{}: schema validation failed with {} error(s)",
            row.input_path.display(),
            row.line_number,
            errors.len()
        ),
        None,
        run_date,
    )
}

fn schema_invalid_decision(row: &BatchLine, errors: Vec<String>, run_date: &str) -> Decision {
    let spec = schema_invalid_spec(errors, row, run_date);
    if let Ok(record) = serde_json::from_str::<SftRecord>(&row.line) {
        return reject(record, spec, run_date);
    }
    Decision {
        scenario_id: row.scenario_id.clone(),
        record: None,
        verdict: VerificationStatusName::Rejected,
        rule: Some(spec.rule),
        detail: spec.detail,
        rubric_score: None,
        critical_failure: None,
        privacy_pass: None,
        scorer_id: None,
        scorer_rationale: None,
    }
}

fn privacy_rejection(record: &SftRecord, run_date: &str) -> Option<RejectSpec> {
    let privacy = privacy_findings(&record.user_message, &record.response);
    (!privacy.is_empty()).then(|| {
        RejectSpec::gate(
            AdmissionRule::PrivacyViolation,
            format!("privacy identifiers detected: {}", privacy.join(", ")),
            Some(false),
            run_date,
        )
    })
}

fn lexical_rejection(record: &SftRecord, run_date: &str) -> Option<RejectSpec> {
    lexical_critical_failure(record).map(|failure| {
        let rule = AdmissionRule::from_critical_failure(failure);
        RejectSpec::gate(
            rule,
            format!("lexical §3 critical-failure screen matched {}", rule.id()),
            Some(true),
            run_date,
        )
    })
}

fn negative_fixture_rejection(record: &SftRecord, run_date: &str) -> Option<RejectSpec> {
    if !record.is_negative_fixture {
        return None;
    }
    expected_negative_fixture_failure(record).map(|failure| {
        let rule = AdmissionRule::from_critical_failure(failure);
        RejectSpec::gate(
            rule,
            format!("labeled negative fixture fail-closed as {}", rule.id()),
            Some(true),
            run_date,
        )
    })
}

fn pre_score_rejection(record: &SftRecord, run_date: &str) -> Option<RejectSpec> {
    privacy_rejection(record, run_date)
        .or_else(|| lexical_rejection(record, run_date))
        .or_else(|| negative_fixture_rejection(record, run_date))
}

fn unscored_rejection(run_date: &str) -> RejectSpec {
    RejectSpec::gate(
        AdmissionRule::RubricBelowThreshold,
        "unscored".to_string(),
        Some(true),
        run_date,
    )
}

fn stale_score_rejection(run_date: &str) -> RejectSpec {
    RejectSpec::gate(
        AdmissionRule::RubricBelowThreshold,
        "stale score: content_sha256 does not match candidate line".to_string(),
        Some(true),
        run_date,
    )
}

fn score_rejection(score: &ScoreEntry, run_date: &str) -> Option<RejectSpec> {
    if let Some(failure) = score.critical_failure {
        let rule = AdmissionRule::from_critical_failure(failure);
        return Some(RejectSpec::scored(
            rule,
            format!("scorer {} raised {}", score.scorer_id, rule.id()),
            score,
            run_date,
        ));
    }
    (score.score < ADMIT_THRESHOLD).then(|| {
        RejectSpec::scored(
            AdmissionRule::RubricBelowThreshold,
            format!(
                "score {} below admission threshold {ADMIT_THRESHOLD}",
                score.score
            ),
            score,
            run_date,
        )
    })
}

fn admit_decision(record: SftRecord, score: &ScoreEntry, run_date: &str) -> Decision {
    let scenario_id = record.scenario_id.clone();
    let fields = VerificationFields::scored(score, run_date);
    let admitted = decision_record(
        record,
        euthymia_schema::enums::VerificationStatus::Admitted,
        fields.clone(),
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
        scorer_id: fields.scorer_id,
        scorer_rationale: fields.scorer_rationale,
    }
}

fn admission_validation_line(line: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(line) else {
        return line.to_string();
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("verification".to_string(), Value::Null);
    }
    serde_json::to_string(&value).unwrap_or_else(|_| line.to_string())
}

fn parse_valid_record(row: &BatchLine) -> Result<SftRecord, String> {
    serde_json::from_str(&row.line).map_err(|err| format!("record failed to deserialize: {err}"))
}

fn score_decision(
    row: &BatchLine,
    record: SftRecord,
    scorer: &impl Scorer,
    run_date: &str,
) -> Decision {
    let Some(score) = scorer.score(&record.scenario_id) else {
        return reject(record, unscored_rejection(run_date), run_date);
    };
    if score.content_sha256 != admission_content_sha256(&row.line) {
        return reject(record, stale_score_rejection(run_date), run_date);
    }
    if let Some(spec) = score_rejection(score, run_date) {
        return reject(record, spec, run_date);
    }
    admit_decision(record, score, run_date)
}

fn decide_row(
    row: &BatchLine,
    validator: &RecordValidator,
    scorer: &impl Scorer,
    run_date: &str,
) -> Decision {
    // Incoming verification is admission output, not candidate truth. Null it
    // before the schema gate so a stale rejection cannot block re-verification;
    // the original line is still what deserializes into the candidate row.
    let validation_line = admission_validation_line(&row.line);
    let schema_errors = validator.validate_line(&validation_line);
    if !schema_errors.is_empty() {
        return schema_invalid_decision(row, schema_errors, run_date);
    }
    let record = match parse_valid_record(row) {
        Ok(record) => record,
        Err(err) => return schema_invalid_decision(row, vec![err], run_date),
    };
    if let Some(spec) = pre_score_rejection(&record, run_date) {
        return reject(record, spec, run_date);
    }
    score_decision(row, record, scorer, run_date)
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

struct BatchOutcome {
    admitted: Vec<SftRecord>,
    rejected: Vec<SftRecord>,
    rows: BTreeMap<String, RowVerdict>,
    counts: AdmissionCounts,
}

fn row_verdict(decision: Decision, output_written: bool) -> RowVerdict {
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
    }
}

fn empty_rule_counts() -> BTreeMap<String, usize> {
    AdmissionRule::ALL
        .into_iter()
        .map(|rule| (rule.id().to_string(), 0))
        .collect()
}

fn collect_decisions(decisions: Vec<Decision>, input_rows: usize) -> BatchOutcome {
    let mut admitted = Vec::new();
    let mut rejected = Vec::new();
    let mut rows = BTreeMap::new();
    let mut per_rule = empty_rule_counts();
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
        rows.insert(
            decision.scenario_id.clone(),
            row_verdict(decision, output_written),
        );
    }

    BatchOutcome {
        admitted,
        rejected,
        rows,
        counts: AdmissionCounts {
            input_rows,
            admitted: admitted_count,
            rejected: rejected_count,
            per_rule,
        },
    }
}

fn build_report(
    config: &AdmissionConfig<'_>,
    scorer: &impl Scorer,
    input_files: Vec<InputFileReport>,
    outcome: &BatchOutcome,
) -> AdmissionReport {
    AdmissionReport {
        batch: config.batch.to_string(),
        pipeline_version: format!("euthymia-admit {PIPELINE_VERSION}"),
        schema_version: CURRENT_SCHEMA_VERSION,
        contract_version: CONTRACT_VERSION.to_string(),
        contract_sections_evaluated: contract_sections(),
        input_files,
        score_file: scorer.source(),
        counts: outcome.counts.clone(),
        rows: outcome.rows.clone(),
    }
}

fn write_report(path: &Path, report: &AdmissionReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let report_json = serde_json::to_string_pretty(report)?
        + "
";
    fs::write(path, report_json).with_context(|| format!("failed to write {}", path.display()))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize {}", path.display()))
}

fn normalize_output_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return normalize_existing_path(path);
    }
    let absolute = absolute_lexical(path)?;
    let Some(parent) = absolute.parent() else {
        return Ok(absolute);
    };
    let parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    Ok(parent.join(absolute.file_name().unwrap_or_default()))
}

fn batch_output_paths(config: &AdmissionConfig<'_>) -> (PathBuf, PathBuf) {
    (
        config
            .output_root
            .join("admitted")
            .join(format!("{}.jsonl", config.batch)),
        config
            .output_root
            .join("rejected")
            .join(format!("{}.jsonl", config.batch)),
    )
}

fn ensure_output_paths_safe(inputs: &[PathBuf], config: &AdmissionConfig<'_>) -> Result<()> {
    let (admitted, rejected) = batch_output_paths(config);
    let outputs = [admitted, rejected, config.report_path.to_path_buf()];
    let mut normalized_outputs = HashSet::new();
    for output in &outputs {
        let normalized = normalize_output_path(output)?;
        ensure!(
            normalized_outputs.insert(normalized.clone()),
            "admission output paths must not collide: {}",
            output.display()
        );
        for input in inputs {
            ensure!(
                normalized != normalize_existing_path(input)?,
                "admission output {} would overwrite input {}",
                output.display(),
                input.display()
            );
        }
    }
    Ok(())
}

fn write_outputs(
    config: &AdmissionConfig<'_>,
    report: &AdmissionReport,
    outcome: &BatchOutcome,
) -> Result<AdmissionOutputs> {
    let (admitted_path, rejected_path) = batch_output_paths(config);
    write_jsonl(&admitted_path, &outcome.admitted)?;
    write_jsonl(&rejected_path, &outcome.rejected)?;
    write_report(config.report_path, report)?;
    Ok(AdmissionOutputs {
        admitted: admitted_path,
        rejected: rejected_path,
        report: config.report_path.to_path_buf(),
    })
}

/// Run admission for one batch and write admitted rows, rejected rows, and a
/// machine-readable report.
pub fn admit_batch(
    inputs: &[PathBuf],
    scorer: &impl Scorer,
    config: &AdmissionConfig<'_>,
) -> Result<AdmissionOutputs> {
    ensure!(
        !inputs.is_empty(),
        "at least one input JSONL file is required"
    );
    config.validate()?;
    ensure_output_paths_safe(inputs, config)?;

    let (rows, input_files) = read_batch(inputs)?;
    let validator = RecordValidator::new(RecordKind::Sft)?;
    let run_date = today_utc();
    let decisions: Vec<Decision> = rows
        .par_iter()
        .map(|row| decide_row(row, &validator, scorer, &run_date))
        .collect();
    let outcome = collect_decisions(decisions, rows.len());
    let report = build_report(config, scorer, input_files, &outcome);
    write_outputs(config, &report, &outcome)
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

#[cfg(test)]
mod tests {
    use super::privacy_findings;

    #[test]
    fn privacy_screen_catches_formatted_phone_numbers() {
        assert!(privacy_findings("Call me at (555) 123-4567 tonight.", "").contains(&"phone"));
    }

    #[test]
    fn privacy_screen_ignores_dates_and_year_ranges() {
        assert!(!privacy_findings("During 2020-2021, things changed.", "").contains(&"phone"));
        assert!(!privacy_findings("The date 2026-11-27 matters here.", "").contains(&"phone"));
    }
}
