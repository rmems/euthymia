//! Closed enum sets (D3). Every value is verbatim from the merged taxonomy
//! (rmems/euthymia@ca8b588, issue #12) or the behavioral contract V0.1.0.
//! Do not add, rename, or reorder values without a schema version bump.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Scenario family — taxonomy "Scenario families" (#12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Breakups,
    Anger,
    DisciplineMotivation,
    Recovery,
    Jealousy,
    Boundaries,
    FailureSetbacks,
}

impl Family {
    /// The family code used inside `scenario_id` / `pair_id` values (D8).
    pub fn id_code(self) -> &'static str {
        match self {
            Self::Breakups => "brk",
            Self::Anger => "ang",
            Self::DisciplineMotivation => "dis",
            Self::Recovery => "rec",
            Self::Jealousy => "jel",
            Self::Boundaries => "bnd",
            Self::FailureSetbacks => "fal",
        }
    }

    /// The allowed secondary-competency pool for this family — taxonomy
    /// "Secondary-competency pools" (#12). A record's secondaries must come
    /// only from this pool and never equal the primary.
    pub fn secondary_pool(self) -> &'static [Competency] {
        match self {
            Self::Breakups => &[
                Competency::CognitiveDiscipline,
                Competency::BehavioralControl,
            ],
            Self::Anger => &[
                Competency::PersonalAccountability,
                Competency::HealthyBoundaries,
                Competency::LongTermResilience,
            ],
            Self::DisciplineMotivation => &[
                Competency::EmotionalAwareness,
                Competency::HealthyBoundaries,
            ],
            Self::Recovery => &[
                Competency::CognitiveDiscipline,
                Competency::HealthyBoundaries,
            ],
            Self::Jealousy => &[
                Competency::PersonalAccountability,
                Competency::BehavioralControl,
                Competency::LongTermResilience,
            ],
            Self::Boundaries => &[
                Competency::EmotionalAwareness,
                Competency::LongTermResilience,
            ],
            Self::FailureSetbacks => &[Competency::HealthyBoundaries],
        }
    }
}

/// The six competencies — contract §1 Six Competencies @ V0.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Competency {
    EmotionalAwareness,
    CognitiveDiscipline,
    PersonalAccountability,
    BehavioralControl,
    HealthyBoundaries,
    LongTermResilience,
}

/// Scenario difficulty — taxonomy "Difficulty quotas" (#12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Everyday,
    Moderate,
    Acute,
}

/// Critical failure — contract §3 Critical Failures @ V0.1.0. Hard rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CriticalFailure {
    /// §3.1 Revenge or Harassment
    RevengeOrHarassment,
    /// §3.2 Shame-Based Discipline
    ShameBasedDiscipline,
    /// §3.3 Enabling Substance Use in Recovery
    EnablingSubstanceUse,
    /// §3.4 Fostering AI Dependence
    FosteringAiDependence,
}

/// Failure mode tagged on the rejected side of a DPO pair (D3, mapped from
/// #18's scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    Shaming,
    Enabling,
    RevengeEncouraging,
    BoundaryViolating,
    ValidationSeekingReinforcement,
    DependenceFostering,
}

/// Persona axis — taxonomy "Persona and setting axes" (#12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum LifeStage {
    #[serde(rename = "age_18_24")]
    Age18To24,
    #[serde(rename = "age_25_34")]
    Age25To34,
    #[serde(rename = "age_35_49")]
    Age35To49,
    #[serde(rename = "age_50_plus")]
    Age50Plus,
}

/// Persona axis — taxonomy "Persona and setting axes" (#12). Generalized
/// roles only, per contract §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipContext {
    Partner,
    ExPartner,
    Friend,
    FamilyMember,
    Coworker,
    AuthorityFigure,
    #[serde(rename = "self")]
    SelfContext,
}

/// Persona axis — taxonomy "Persona and setting axes" (#12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    Home,
    Work,
    Social,
    Online,
    Public,
}

/// Who or what produced the record — `Provenance::generator_kind` (D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeneratorKind {
    Procedural,
    Teacher,
    Human,
}

/// Verification outcome — `VerificationResult::status` (D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    Admitted,
    Rejected,
}
