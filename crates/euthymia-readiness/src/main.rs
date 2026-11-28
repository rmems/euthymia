//! Bounded offline prerequisite for #16 and RM-2588, using #32's real gates.
mod handoff;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use euthymia_schema::validator::{RecordKind, RecordValidator};
use euthymia_verify::{AdmissionConfig, FileScorer, admit_batch};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Provider {
    #[default]
    Mock,
    Deepseek,
}

/// Contains an environment variable NAME only. No environment lookup exists.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    provider: Provider,
    model: String,
    endpoint: Option<String>,
    api_key_env: Option<String>,
    seed: u64,
    generated_at: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::Mock,
            model: "offline-mock-v1".into(),
            endpoint: None,
            api_key_env: None,
            seed: 42,
            // An explicit fixture date, never a claim of a live generation time.
            generated_at: "2026-11-28".into(),
        }
    }
}

impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.model.is_empty()
                && self.model.len() <= 128
                && self
                    .model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
                && !self.model.to_ascii_lowercase().contains("latest"),
            "model must be a non-floating identifier (not a credential)"
        );
        match self.provider {
            Provider::Mock => ensure!(
                self.model == "offline-mock-v1"
                    && self.endpoint.is_none()
                    && self.api_key_env.is_none(),
                "mock requires model offline-mock-v1, with no endpoint or credential variable"
            ),
            Provider::Deepseek => {
                let endpoint = self.endpoint.as_deref().context("endpoint required")?;
                let rest = endpoint
                    .strip_prefix("https://")
                    .context("endpoint must use HTTPS")?;
                let host = rest.split('/').next().unwrap_or_default();
                ensure!(
                    !host.is_empty()
                        && host
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
                        && rest
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"/._-:".contains(&b)),
                    "endpoint must be a credential-free HTTPS URL without query, fragment or userinfo"
                );
                let name = self
                    .api_key_env
                    .as_deref()
                    .context("api_key_env NAME required")?;
                ensure!(
                    !name.is_empty()
                        && name.len() <= 128
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                        && !name.as_bytes()[0].is_ascii_digit(),
                    "api_key_env must be an uppercase environment variable NAME"
                );
            }
        }
        // Reuse the canonical validator, including real calendar-date validation.
        let errors = RecordValidator::new(RecordKind::Sft)?
            .validate_line(&serde_json::to_string(&candidate(self))?);
        ensure!(
            errors.is_empty(),
            "invalid candidate configuration: {errors:?}"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
enum Purpose {
    #[default]
    Smoke,
    Training,
    Export,
}

#[derive(Parser)]
#[command(about = "Offline structural smoke only; no live teacher, training or export")]
struct Cli {
    /// Optional JSON configuration. Default: built-in offline mock.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Check configuration syntax only; never contacts the configured endpoint.
    #[arg(long, conflicts_with_all = ["emit_request", "request", "response", "mock_response"])]
    check_config: bool,
    /// Emit a credential-free request for Hermes; no generation is performed.
    #[arg(long, conflicts_with_all = ["request", "response", "mock_response"])]
    emit_request: bool,
    /// Replay this exact versioned request with --response or --mock-response.
    #[arg(long, conflicts_with = "config", requires = "response_source")]
    request: Option<PathBuf>,
    /// Recorded v1 response envelope. Always unreviewed, never an approval.
    #[arg(
        long,
        requires = "request",
        conflicts_with = "mock_response",
        group = "response_source"
    )]
    response: Option<PathBuf>,
    /// Exercise the same replay path with a local fixture for a mock request.
    #[arg(long, requires = "request", group = "response_source")]
    mock_response: bool,
    /// Fresh directory name under data/raw/; existing destinations are refused.
    #[arg(long, default_value = "readiness-smoke")]
    batch: String,
    /// Training and export are explicitly blocked pending rights/review/release work.
    #[arg(long, value_enum, default_value = "smoke")]
    purpose: Purpose,
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn candidate(config: &Config) -> serde_json::Value {
    // Fixed fictional everyday example. No prompt, private input or corpus is read.
    json!({
        "schema_version": 1,
        "scenario_id": "eu-fal-001",
        "family": "failure_setbacks",
        "primary_competency": "long_term_resilience",
        "secondary_competencies": [],
        "difficulty": "everyday",
        "life_stage": "age_25_34",
        "relationship_context": "self",
        "setting": "home",
        "user_message": "A fictional adult feels disappointed after a practice puzzle does not work out. What is a small next step?",
        "response": "It is reasonable to feel disappointed. One unfinished puzzle does not define your ability. Take a short pause, then choose one part to try again when you are ready.",
        "provenance": {
            "generator_kind": "procedural",
            "generator_id": "euthymia-readiness-offline-mock-v1",
            "prompt_version": "everyday-puzzle-fixture-v1",
            "generated_at": config.generated_at,
            "seed": config.seed
        },
        "contract_version": "V0.1.0",
        "is_negative_fixture": false,
        "verification": null
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Deny before config reads or any output writes. No approval bypass flag.
    ensure!(
        cli.purpose == Purpose::Smoke,
        "training/export blocked: no reviewed teacher rights, human approval or RM-2588 release manifest"
    );
    let (config, request_bytes) = if let Some(path) = &cli.request {
        handoff::load_request(path)?
    } else {
        let config: Config = match &cli.config {
            Some(path) => serde_json::from_slice(&handoff::read_bounded(path)?)?,
            None => Config::default(),
        };
        config.validate()?;
        let bytes = serde_json::to_vec_pretty(&handoff::request(&config))?;
        (config, bytes)
    };
    if cli.check_config {
        println!(
            "configuration syntax valid; live generation disabled; rights and human approval UNREVIEWED"
        );
        return Ok(());
    }
    let response_bytes = if cli.mock_response {
        Some(serde_json::to_vec_pretty(&handoff::Response::mock(
            &config,
            &request_bytes,
        )?)?)
    } else if let Some(path) = &cli.response {
        Some(handoff::read_bounded(path)?)
    } else {
        None
    };
    let mut record = candidate(&config);
    if let Some(bytes) = &response_bytes {
        let response: handoff::Response = serde_json::from_slice(bytes)?;
        response.validate(&config, &request_bytes)?;
        record = handoff::replay_candidate(&config, &response);
    } else if !cli.emit_request {
        ensure!(
            config.provider == Provider::Mock,
            "live generation disabled: use --emit-request, then --request and --response for offline replay"
        );
    }
    let line = serde_json::to_string(&record)?;
    let errors = RecordValidator::new(RecordKind::Sft)?.validate_line(&line);
    ensure!(
        errors.is_empty(),
        "replayed candidate failed structural validation"
    );
    ensure!(
        !cli.batch.is_empty()
            && cli.batch.len() <= 80
            && cli
                .batch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "batch must be 1..80 ASCII letters, digits, underscores or hyphens"
    );
    ensure!(
        fs::read("schemas/sft_record.v1.json")?
            == include_bytes!("../../../schemas/sft_record.v1.json"),
        "run from the matching euthymia repository root"
    );
    for path in ["data", "data/raw"] {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_dir() && !metadata.is_symlink(),
            "output parent must be a real directory"
        );
    }
    let root = PathBuf::from("data/raw").join(&cli.batch);
    fs::create_dir(&root).context("batch destination must be new; choose another --batch")?;
    fs::write(root.join("request.json"), &request_bytes)?;
    if cli.emit_request {
        println!(
            "request prepared: {}; no generation authorized or performed",
            root.join("request.json").display()
        );
        return Ok(());
    }
    if let Some(bytes) = &response_bytes {
        fs::write(root.join("response.json"), bytes)?;
    }
    let config_bytes = serde_json::to_vec_pretty(&config)?;
    let candidates = format!("{line}\n");
    fs::write(root.join("config.json"), &config_bytes)?;
    fs::write(root.join("candidates.jsonl"), &candidates)?;
    // No invented reviewer score: the real admission gate must reject unscored rows.
    fs::write(root.join("scores.jsonl"), "")?;
    let scorer = FileScorer::load(&root.join("scores.jsonl"))?;
    let report_path = root.join("admission_report.json");
    let outputs = admit_batch(
        &[root.join("candidates.jsonl")],
        &scorer,
        &AdmissionConfig {
            batch: if response_bytes.is_some() {
                "replay"
            } else {
                "mock"
            },
            output_root: &root,
            report_path: &report_path,
        },
    )?;
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&outputs.report)?)?;
    ensure!(
        report["counts"]["input_rows"] == 1
            && report["counts"]["admitted"] == 0
            && report["counts"]["rejected"] == 1
            && fs::read(&outputs.admitted)?.is_empty(),
        "unexpected admission outcome; smoke failed"
    );
    // Path/clock independent provenance. Admission diagnostics retain parent behavior.
    let provenance = json!({
        "purpose": if response_bytes.is_some() { "offline_recorded_response_validation" } else { "offline_structural_smoke_only" },
        "generator_version": env!("CARGO_PKG_VERSION"),
        "generator_source_sha256": hash(include_bytes!("main.rs")),
        "handoff_source_sha256": hash(include_bytes!("handoff.rs")),
        "request_sha256": hash(&request_bytes),
        "response_sha256": response_bytes.as_ref().map(|bytes| hash(bytes)),
        "response_evidence": if response_bytes.is_some() { "unverified_recorded_envelope" } else { "built_in_fixture" },
        "admission_source_sha256": hash(include_bytes!("../../euthymia-verify/src/lib.rs")),
        "validator_source_sha256": hash(include_bytes!("../../euthymia-schema/src/validator.rs")),
        "cross_field_source_sha256": hash(include_bytes!("../../euthymia-schema/src/validate.rs")),
        "config_sha256": hash(&config_bytes),
        "candidate_line_sha256": hash(line.as_bytes()),
        "candidate_file_sha256": hash(candidates.as_bytes()),
        "contract_sha256": hash(include_bytes!("../../../contract/BEHAVIORAL_CONTRACT.md")),
        "schema_sha256": hash(include_bytes!("../../../schemas/sft_record.v1.json")),
        "taxonomy_sha256": hash(include_bytes!("../../../taxonomy/coverage-matrix.csv")),
        "dependency_lock_sha256": hash(include_bytes!("../../../Cargo.lock")),
        "teacher_called_by_this_command": false,
        "teacher_rights": "unreviewed",
        "human_review": "pending",
        "training_allowed": false,
        "export_allowed": false,
        "structural_validation": "passed",
        "admitted": 0,
        "rejected": 1
    });
    fs::write(
        root.join("provenance.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    println!(
        "offline validation passed: 1 valid candidate, 0 admitted, 1 rejection ({})",
        report["rows"]["eu-fal-001"]["rule"]
    );
    println!("outputs: {}; training/export BLOCKED", root.display());
    Ok(())
}
