use async_trait::async_trait;
use converge_model::formation::{FormationPlan, FormationRequest, ProfileSnapshot, RoleAssignment};
use converge_pack::{AgentEffect, Context, ContextKey, Suggestor};
use ferrox_ortools_sys::safe::CpModel;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};

// Uses a distinct prefix from converge-optimization's FormationAssemblySuggestor
// so both can coexist in the same engine.
const REQUEST_PREFIX: &str = "cpsat-formation-request:";
const PLAN_PREFIX: &str = "cpsat-formation-plan:";

const W_LATENCY: i64 = 200;
const W_COST: i64 = 100;

/// Assembles a formation using CP-SAT weighted assignment.
///
/// Unlike the bipartite-matching approach in converge-optimization, this
/// maximizes total confidence score while respecting role/capability constraints.
/// Prefer this when you have multiple suggestors filling the same role and want
/// the highest-confidence assignment rather than an arbitrary maximum matching.
pub struct CpSatFormationSuggestor {
    catalog: Vec<ProfileSnapshot>,
}

impl CpSatFormationSuggestor {
    pub fn new(catalog: Vec<ProfileSnapshot>) -> Self {
        Self { catalog }
    }
}

#[async_trait]
impl Suggestor for CpSatFormationSuggestor {
    fn name(&self) -> &'static str {
        "CpSatFormationSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("weighted bipartite assignment via CP-SAT v9.15; O(roles * catalog) variables")
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|f| f.id().starts_with(REQUEST_PREFIX) && !plan_exists(ctx, request_id(f.id())))
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let _span = suggestor_span(
            self.name(),
            ContextKey::Seeds,
            ContextKey::Strategies,
            ctx.count(ContextKey::Seeds),
        )
        .entered();
        let mut proposals = Vec::new();

        for fact in ctx
            .get(ContextKey::Seeds)
            .iter()
            .filter(|f| f.id().starts_with(REQUEST_PREFIX))
        {
            let rid = request_id(fact.id());
            if plan_exists(ctx, rid) {
                continue;
            }

            match serde_json::from_str::<FormationRequest>(fact.content()) {
                Ok(req) => {
                    let plan = assemble_cp(&req, &self.catalog);
                    let confidence = plan.coverage_ratio;
                    proposals.push(
                        FERROX_PROVENANCE
                            .proposed_fact(
                                ContextKey::Strategies,
                                format!("{PLAN_PREFIX}{}", plan.request_id),
                                serde_json::to_string(&plan).unwrap_or_default(),
                            )
                            .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "malformed cpsat-formation-request");
                }
            }
        }

        if proposals.is_empty() {
            AgentEffect::empty()
        } else {
            AgentEffect::with_proposals(proposals)
        }
    }
}

fn request_id(fact_id: &str) -> &str {
    fact_id.trim_start_matches(REQUEST_PREFIX)
}

fn plan_exists(ctx: &dyn Context, request_id: &str) -> bool {
    let plan_id = format!("{PLAN_PREFIX}{request_id}");
    ctx.get(ContextKey::Strategies)
        .iter()
        .any(|f| f.id() == plan_id.as_str())
}

#[allow(clippy::too_many_lines)]
fn assemble_cp(req: &FormationRequest, catalog: &[ProfileSnapshot]) -> FormationPlan {
    if req.required_roles.is_empty() {
        return FormationPlan {
            request_id: req.id.clone(),
            assignments: vec![],
            unmatched_roles: vec![],
            coverage_ratio: 1.0,
        };
    }

    // Filter eligible catalog entries (must satisfy all required_capabilities)
    let eligible: Vec<&ProfileSnapshot> = catalog
        .iter()
        .filter(|s| {
            req.required_capabilities
                .iter()
                .all(|cap| s.capabilities.contains(cap))
        })
        .collect();

    let n_roles = req.required_roles.len();
    let n_cands = eligible.len();

    if n_cands == 0 {
        return FormationPlan {
            request_id: req.id.clone(),
            assignments: vec![],
            unmatched_roles: req.required_roles.clone(),
            coverage_ratio: 0.0,
        };
    }

    let mut model = CpModel::new();

    // x[i][j] = 1 if eligible[j] is assigned to role slot i
    // Value 0 in the grid means "not a valid assignment" (role mismatch)
    let mut x: Vec<Vec<i32>> = vec![vec![-1i32; n_cands]; n_roles];
    for (i, role) in req.required_roles.iter().enumerate() {
        for (j, cand) in eligible.iter().enumerate() {
            if cand.role == *role {
                x[i][j] = model.new_bool_var(&format!("x_{i}_{j}"));
            }
        }
    }

    // Each role slot filled at most once
    for row in &x {
        let vars: Vec<i32> = row.iter().copied().filter(|&v| v != -1).collect();
        if vars.len() > 1 {
            let ones = vec![1i64; vars.len()];
            model.add_linear_le(&vars, &ones, 1);
        }
    }

    // Each suggestor used at most once (no double-booking)
    for j in 0..n_cands {
        let vars: Vec<i32> = x.iter().map(|row| row[j]).filter(|&v| v != -1).collect();
        if vars.len() > 1 {
            let ones = vec![1i64; vars.len()];
            model.add_linear_le(&vars, &ones, 1);
        }
    }

    // Objective: maximize weighted score
    let mut obj_vars = Vec::new();
    let mut obj_coeffs = Vec::new();
    for row in &x {
        for (j, &var_idx) in row.iter().enumerate() {
            if var_idx != -1 {
                obj_vars.push(var_idx);
                obj_coeffs.push(score(eligible[j]));
            }
        }
    }

    if !obj_vars.is_empty() {
        model.maximize(&obj_vars, &obj_coeffs);
    }

    let solution = model.solve(10.0);

    if !solution.status().is_success() {
        return FormationPlan {
            request_id: req.id.clone(),
            assignments: vec![],
            unmatched_roles: req.required_roles.clone(),
            coverage_ratio: 0.0,
        };
    }

    let mut assignments = Vec::new();
    let mut assigned = vec![false; n_roles];

    for (i, row) in x.iter().enumerate() {
        for (j, &var_idx) in row.iter().enumerate() {
            if var_idx != -1 && solution.value(var_idx) == 1 {
                assignments.push(RoleAssignment {
                    role: req.required_roles[i],
                    suggestor: eligible[j].name.clone(),
                });
                assigned[i] = true;
                break;
            }
        }
    }

    let unmatched_roles: Vec<_> = req
        .required_roles
        .iter()
        .enumerate()
        .filter(|(i, _)| !assigned[*i])
        .map(|(_, r)| *r)
        .collect();

    #[allow(clippy::cast_precision_loss)]
    let coverage_ratio = assignments.len() as f64 / n_roles as f64;

    FormationPlan {
        request_id: req.id.clone(),
        assignments,
        unmatched_roles,
        coverage_ratio,
    }
}

fn score(snap: &ProfileSnapshot) -> i64 {
    use converge_provider::{CostClass, LatencyClass};

    #[allow(clippy::cast_possible_truncation)]
    let base = (f64::from(snap.confidence_max) * 1000.0) as i64;

    let latency_bonus = match snap.latency_hint {
        LatencyClass::Realtime => W_LATENCY,
        LatencyClass::Interactive => W_LATENCY / 2,
        _ => 0,
    };

    let cost_bonus = match snap.cost_hint {
        CostClass::Low => W_COST,
        CostClass::Medium => W_COST / 2,
        _ => 0,
    };

    base + latency_bonus + cost_bonus
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use converge_model::formation::{SuggestorCapability, SuggestorRole};
    use converge_provider::{CostClass, LatencyClass};

    fn snap(name: &str, role: SuggestorRole, conf_max: f32) -> ProfileSnapshot {
        ProfileSnapshot {
            name: name.to_string(),
            role,
            output_keys: vec![ContextKey::Strategies],
            cost_hint: CostClass::Medium,
            latency_hint: LatencyClass::Interactive,
            capabilities: vec![],
            confidence_min: 0.5,
            confidence_max: conf_max,
        }
    }

    #[test]
    fn prefers_higher_confidence_over_lower() {
        let catalog = vec![
            snap("a-low", SuggestorRole::Analysis, 0.6),
            snap("a-high", SuggestorRole::Analysis, 0.95),
        ];
        let req = FormationRequest {
            id: "r1".to_string(),
            required_roles: vec![SuggestorRole::Analysis],
            required_capabilities: vec![],
        };

        let plan = assemble_cp(&req, &catalog);
        assert_eq!(plan.assignments.len(), 1);
        assert_eq!(plan.assignments[0].suggestor, "a-high");
    }

    #[test]
    fn no_double_booking() {
        let catalog = vec![
            snap("a1", SuggestorRole::Analysis, 0.9),
            snap("a2", SuggestorRole::Analysis, 0.8),
        ];
        let req = FormationRequest {
            id: "r2".to_string(),
            required_roles: vec![SuggestorRole::Analysis, SuggestorRole::Analysis],
            required_capabilities: vec![],
        };

        let plan = assemble_cp(&req, &catalog);
        assert_eq!(plan.assignments.len(), 2);
        let names: std::collections::HashSet<_> =
            plan.assignments.iter().map(|a| &a.suggestor).collect();
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn empty_catalog_yields_zero_coverage() {
        let req = FormationRequest {
            id: "r3".to_string(),
            required_roles: vec![SuggestorRole::Analysis],
            required_capabilities: vec![],
        };
        let plan = assemble_cp(&req, &[]);
        assert!(plan.coverage_ratio < f64::EPSILON);
    }

    #[test]
    fn capability_filter_respected() {
        let mut cap_snap = snap("optimizer", SuggestorRole::Analysis, 0.9);
        cap_snap.capabilities = vec![SuggestorCapability::Optimization];
        let catalog = vec![cap_snap, snap("plain", SuggestorRole::Analysis, 0.95)];
        let req = FormationRequest {
            id: "r4".to_string(),
            required_roles: vec![SuggestorRole::Analysis],
            required_capabilities: vec![SuggestorCapability::Optimization],
        };
        let plan = assemble_cp(&req, &catalog);
        assert_eq!(plan.assignments.len(), 1);
        assert_eq!(plan.assignments[0].suggestor, "optimizer");
    }
}
