//! Canonical record types — the source of truth for the generated JSON
//! Schemas (D1, D4–D6). Field sets are locked by the 2026-11-20 decision
//! record on rmems/euthymia#13.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;

use crate::enums::*;

/// Current record schema version (D2). A record carrying any other
/// `schema_version` fails to deserialize under this build.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Contract version every record cites (D4/D5): contract/BEHAVIORAL_CONTRACT.md.
pub const CONTRACT_VERSION: &str = "V0.1.0";

/// `scenario_id` pattern — `eu-<family code>-<3 digits>` (D8).
pub const SCENARIO_ID_PATTERN: &str = r"^eu-(brk|ang|dis|rec|jel|bnd|fal)-[0-9]{3}$";

/// `pair_id` pattern for DPO records (D8).
pub const PAIR_ID_PATTERN: &str = r"^eu-dpo-[0-9]{3}$";

/// ISO calendar date (`YYYY-MM-DD`) for provenance/verification timestamps.
const ISO_DATE_PATTERN: &str = r"^\d{4}-\d{2}-\d{2}$";

/// `schema_version` field: serializes as [`CURRENT_SCHEMA_VERSION`], rejects
/// every other value on deserialize, and appears as `{"const": 1}` in the
/// generated schema (D2). A newtype so an out-of-version record cannot be
/// constructed in Rust either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SchemaVersion;

impl SchemaVersion {
    /// The version this field always carries.
    pub fn get(self) -> u32 {
        CURRENT_SCHEMA_VERSION
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(CURRENT_SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        if value == CURRENT_SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(D::Error::custom(format!(
                "unsupported schema_version {value}; expected {CURRENT_SCHEMA_VERSION}"
            )))
        }
    }
}

impl JsonSchema for SchemaVersion {
    fn schema_name() -> Cow<'static, str> {
        "SchemaVersion".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json!({ "const": CURRENT_SCHEMA_VERSION })
            .as_object()
            .expect("object literal")
            .clone()
            .into()
    }

    fn inline_schema() -> bool {
        true
    }
}

/// `contract_version` field: the pinned string [`CONTRACT_VERSION`], enforced
/// the same way as [`SchemaVersion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ContractVersion;

impl ContractVersion {
    /// The contract version string this field always carries.
    pub fn get(self) -> &'static str {
        CONTRACT_VERSION
    }
}

impl Serialize for ContractVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(CONTRACT_VERSION)
    }
}

impl<'de> Deserialize<'de> for ContractVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == CONTRACT_VERSION {
            Ok(Self)
        } else {
            Err(D::Error::custom(format!(
                "unsupported contract_version {value:?}; expected {CONTRACT_VERSION:?}"
            )))
        }
    }
}

impl JsonSchema for ContractVersion {
    fn schema_name() -> Cow<'static, str> {
        "ContractVersion".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json!({ "const": CONTRACT_VERSION })
            .as_object()
            .expect("object literal")
            .clone()
            .into()
    }

    fn inline_schema() -> bool {
        true
    }
}

/// Where a record came from (D6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub generator_kind: GeneratorKind,
    /// Model name or template-set id.
    pub generator_id: String,
    pub prompt_version: String,
    /// ISO date (`YYYY-MM-DD`).
    #[schemars(regex(pattern = ISO_DATE_PATTERN))]
    pub generated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

/// Shape of a verification result (D6). This schema carries the shape only —
/// scoring logic is issue #17.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 5))]
    pub rubric_score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub critical_failure: Option<CriticalFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_pass: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
    /// ISO date (`YYYY-MM-DD`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = ISO_DATE_PATTERN))]
    pub verified_at: Option<String>,
}

/// One SFT training row (D4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SftRecord {
    pub schema_version: SchemaVersion,
    #[schemars(regex(pattern = SCENARIO_ID_PATTERN))]
    pub scenario_id: String,
    pub family: Family,
    pub primary_competency: Competency,
    /// Zero to two competencies; must be unique and never equal the primary.
    /// JSON Schema enforces the cap; uniqueness and ≠ primary are cross-field
    /// rules checked by `euthymia-validate` (D9).
    #[schemars(length(max = 2))]
    pub secondary_competencies: Vec<Competency>,
    pub difficulty: Difficulty,
    pub life_stage: LifeStage,
    pub relationship_context: RelationshipContext,
    pub setting: Setting,
    pub user_message: String,
    pub response: String,
    pub provenance: Provenance,
    pub contract_version: ContractVersion,
    /// §3 negative-fixture label — the only permitted carrier of a critical
    /// failure, machine-readable end-to-end (D7).
    pub is_negative_fixture: bool,
    /// The rule expected to reject this record; present iff
    /// `is_negative_fixture` is true (D7). Enforced as a cross-field rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_rejection: Option<String>,
    /// Null until verification (#17) runs.
    pub verification: Option<VerificationResult>,
}

/// One DPO preference pair (D5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DpoPairRecord {
    pub schema_version: SchemaVersion,
    #[schemars(regex(pattern = PAIR_ID_PATTERN))]
    pub pair_id: String,
    /// The SFT scenario this pair derives from.
    #[schemars(regex(pattern = SCENARIO_ID_PATTERN))]
    pub scenario_id: String,
    pub family: Family,
    pub primary_competency: Competency,
    pub difficulty: Difficulty,
    pub prompt: String,
    /// The contract-compliant response. Never a negative fixture (D5).
    pub chosen: String,
    /// The rejected response, tagged with `rejected_failure_mode`.
    pub rejected: String,
    pub rejected_failure_mode: FailureMode,
    pub provenance: Provenance,
    pub contract_version: ContractVersion,
    /// Labels a pair built around a §3-violating rejected side (D5).
    pub is_negative_fixture: bool,
    /// Null until verification (#17) runs.
    pub verification: Option<VerificationResult>,
}
