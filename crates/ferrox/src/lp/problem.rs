use serde::{Deserialize, Serialize};

use converge_pack::{ExecutionIdentity, FactPayload};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpVariable {
    pub name: String,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub lb: f64,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub ub: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpTerm {
    pub var: String,
    pub coeff: f64,
}

/// A row constraint: lb <= sum(terms) <= ub
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpConstraint {
    pub name: String,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub lb: f64,
    #[serde(with = "crate::serde_util::f64_inf")]
    pub ub: f64,
    pub terms: Vec<LpTerm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpObjective {
    pub terms: Vec<LpTerm>,
    pub maximize: bool,
}

/// Seeded into `ContextKey::Seeds` with id prefix `"glop-request:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpRequest {
    pub id: String,
    pub variables: Vec<LpVariable>,
    pub constraints: Vec<LpConstraint>,
    pub objective: LpObjective,
    pub time_limit_seconds: Option<f64>,
}

impl FactPayload for LpRequest {
    const FAMILY: &'static str = "ferrox.lp.request";
    const VERSION: u16 = 1;
}

/// Written to `ContextKey::Strategies` with id prefix `"glop-plan:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LpPlan {
    pub request_id: String,
    pub status: String,
    pub values: Vec<(String, f64)>,
    pub objective_value: f64,
    pub solver: String,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for LpPlan {
    const FAMILY: &'static str = "ferrox.lp.plan";
    const VERSION: u16 = 1;
}
