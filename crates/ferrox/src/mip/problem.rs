use serde::{Deserialize, Serialize};

use converge_pack::{ExecutionIdentity, FactPayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarKind {
    Continuous,
    Integer,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipVariable {
    pub name: String,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub lb: f64,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub ub: f64,
    pub kind: VarKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipTerm {
    pub var: String,
    pub coeff: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipConstraint {
    pub name: String,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub lb: f64,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub ub: f64,
    pub terms: Vec<MipTerm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipObjective {
    pub terms: Vec<MipTerm>,
    pub maximize: bool,
}

/// Seeded into `ContextKey::Seeds` with id prefix `"mip-request:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipRequest {
    pub id: String,
    pub variables: Vec<MipVariable>,
    pub constraints: Vec<MipConstraint>,
    pub objective: MipObjective,
    pub time_limit_seconds: Option<f64>,
    pub mip_gap_tolerance: Option<f64>,
}

impl FactPayload for MipRequest {
    const FAMILY: &'static str = "ferrox.mip.request";
    const VERSION: u16 = 1;
}

/// Written to `ContextKey::Strategies` with id prefix `"mip-plan:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MipPlan {
    pub request_id: String,
    pub status: String,
    pub values: Vec<(String, f64)>,
    pub objective_value: f64,
    pub mip_gap: f64,
    pub solver: String,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for MipPlan {
    const FAMILY: &'static str = "ferrox.mip.plan";
    const VERSION: u16 = 1;
}
