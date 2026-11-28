//! File handoff only. Hermes owns any future provider transport and credentials.
use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Config, Provider, candidate, hash};

pub fn request(config: &Config) -> Value {
    let mut request = json!({
        "kind": "euthymia.teacher_request",
        "version": 1,
        "scope": "fictional_everyday_puzzle_candidate_only",
        "config": config,
        "contract_sha256": hash(include_bytes!("../../../contract/BEHAVIORAL_CONTRACT.md")),
        "schema_sha256": hash(include_bytes!("../../../schemas/sft_record.v1.json")),
        "generation_authorization": "not_assessed",
        "request_body": {
            "model": config.model,
            "messages": [
                {"role":"system", "content":"Draft one short, non-clinical everyday emotional-resilience response to the fictional puzzle scenario. Use only ASCII text. No real people, personal stories, private data, diagnoses, clinical advice, harassment, shame, or dependence on AI. Return only the response text. This is an unreviewed candidate, never an approval or training release."},
                {"role":"user", "content": candidate(config)["user_message"]}
            ],
            "max_tokens": 512
        }
    });
    if config.provider == Provider::Deepseek {
        request["transport"] = json!("openrouter");
        request["base_url"] = json!(crate::OPENROUTER_BASE_URL);
        request["http_method"] = json!("POST");
        request["request_body"]["provider"] = json!({
            "only": ["deepseek"],
            "allow_fallbacks": false
        });
    }
    request
}

/// Size bound applies before parsing. Never reads environment variables or URLs.
pub fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(65_537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "handoff file exceeds 64 KiB");
    Ok(bytes)
}

pub fn load_request(path: &Path) -> Result<(Config, Vec<u8>)> {
    let bytes = read_bounded(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let config: Config = serde_json::from_value(value["config"].clone())?;
    config.validate()?;
    // Exact v1 structure/prompt/pins, rather than accepting an arbitrary prompt.
    ensure!(
        value == request(&config),
        "request version, scope, prompt or source pins mismatch"
    );
    Ok((config, bytes))
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    SyntheticFixture,
    RecordedTeacher,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u8,
    pub request_sha256: String,
    pub provider: Provider,
    pub model: String,
    pub serving_provider: String,
    pub origin: Origin,
    pub content: String,
}

impl Response {
    pub fn mock(config: &Config, request_bytes: &[u8]) -> Result<Self> {
        ensure!(
            config.provider == Provider::Mock,
            "mock response requires a mock request"
        );
        Ok(Self {
            version: 1,
            request_sha256: hash(request_bytes),
            provider: config.provider,
            model: config.model.clone(),
            serving_provider: "offline-fixture".into(),
            origin: Origin::SyntheticFixture,
            content: candidate(config)["response"]
                .as_str()
                .context("fixture response")?
                .into(),
        })
    }

    pub fn validate(&self, config: &Config, request_bytes: &[u8]) -> Result<()> {
        ensure!(self.version == 1, "unsupported response version");
        ensure!(
            self.request_sha256 == hash(request_bytes),
            "response request hash mismatch"
        );
        ensure!(
            self.provider == config.provider && self.model == config.model,
            "response teacher identity mismatch"
        );
        ensure!(
            valid_serving_identity(&self.serving_provider),
            "serving_provider identity evidence required"
        );
        ensure!(
            config.provider != Provider::Mock || self.origin == Origin::SyntheticFixture,
            "mock cannot claim teacher origin"
        );
        ensure!(
            config.provider != Provider::Deepseek || self.serving_provider == "deepseek",
            "OpenRouter pilot requires serving_provider deepseek; missing or alternate route evidence is rejected"
        );
        // Parent #32 currently has a UTF-8 slicing finding. Quarantine instead of
        // changing another lane's admission logic or risking a worker panic.
        ensure!(
            valid_response_text(&self.content),
            "response must be 1..4096 bytes of nonempty printable ASCII text"
        );
        Ok(())
    }
}

pub fn replay_candidate(config: &Config, response: &Response) -> Value {
    let mut record = candidate(config);
    record["response"] = json!(response.content);
    if response.origin == Origin::RecordedTeacher {
        record["provenance"]["generator_kind"] = json!("teacher");
        record["provenance"]["generator_id"] = json!(format!(
            "{}@openrouter/{}",
            config.model, response.serving_provider
        ));
    } else {
        record["provenance"]["generator_id"] = json!("euthymia-recorded-synthetic-fixture-v1");
    }
    record
}

fn valid_serving_identity(identity: &str) -> bool {
    !identity.is_empty()
        && identity.len() <= 128
        && identity
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
}

fn valid_response_text(content: &str) -> bool {
    !content.trim().is_empty()
        && content.len() <= 4096
        && content
            .bytes()
            .all(|b| b.is_ascii_graphic() || matches!(b, b' ' | b'\n' | b'\t'))
}
