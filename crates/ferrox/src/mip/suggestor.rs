use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, Suggestor};
use ferrox_highs_sys::HighsModelStatus;
use ferrox_highs_sys::safe::HighsSolver;
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};

use super::problem::{MipPlan, MipRequest, MipTerm, VarKind};

const REQUEST_PREFIX: &str = "mip-request:";
const PLAN_PREFIX: &str = "mip-plan:";

pub struct HighsMipSuggestor;

#[async_trait]
impl Suggestor for HighsMipSuggestor {
    fn name(&self) -> &'static str {
        "HighsMipSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("MIP branch-and-cut via HiGHS v1.14; NP-hard in general")
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

            match serde_json::from_str::<MipRequest>(fact.content()) {
                Ok(req) => {
                    let plan = solve_mip(&req);
                    let confidence = match plan.status.as_str() {
                        "optimal" => 1.0,
                        "feasible" => 0.6 + (1.0 - plan.mip_gap.min(1.0)) * 0.3,
                        _ => 0.0,
                    };
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
                    warn!(id = %fact.id(), error = %e, "malformed mip-request");
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

pub fn solve_mip(req: &MipRequest) -> MipPlan {
    if let Err(reason) = validate_mip_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid mip-request");
        return invalid_plan(req);
    }

    let mut solver = HighsSolver::new();

    // HiGHS cost sign: we pass costs directly; HiGHS minimizes by default.
    // For maximize, negate all cost coefficients.
    let sign = if req.objective.maximize { -1.0 } else { 1.0 };

    // Build a cost vector indexed by variable position
    let mut costs = vec![0.0f64; req.variables.len()];
    let name_to_pos: HashMap<&str, usize> = req
        .variables
        .iter()
        .enumerate()
        .map(|(i, v)| (v.name.as_str(), i))
        .collect();

    for term in &req.objective.terms {
        if let Some(&pos) = name_to_pos.get(term.var.as_str()) {
            costs[pos] = term.coeff * sign;
        }
    }

    // Add columns
    let col_indices: Vec<i32> = req
        .variables
        .iter()
        .enumerate()
        .map(|(i, var)| match var.kind {
            VarKind::Continuous => solver.add_col(costs[i], var.lb, var.ub),
            VarKind::Integer => solver.add_int_col(costs[i], var.lb, var.ub),
            VarKind::Binary => solver.add_bin_col(costs[i]),
        })
        .collect();

    if let Some(tl) = req.time_limit_seconds {
        solver.set_time_limit(tl);
    }
    if let Some(gap) = req.mip_gap_tolerance {
        solver.set_mip_rel_gap(gap);
    }

    // Add rows
    for con in &req.constraints {
        let mut indices = Vec::new();
        let mut vals = Vec::new();
        for term in &con.terms {
            if let Some(&pos) = name_to_pos.get(term.var.as_str()) {
                indices.push(col_indices[pos]);
                vals.push(term.coeff);
            }
        }
        solver.add_row(con.lb, con.ub, &indices, &vals);
    }

    let status = solver.run();

    let status_str = match status {
        HighsModelStatus::Optimal => "optimal",
        HighsModelStatus::SolutionLimit | HighsModelStatus::TimeLimit => "feasible",
        HighsModelStatus::Infeasible => "infeasible",
        HighsModelStatus::Unbounded => "unbounded",
        _ => "error",
    };

    let (values, objective_value, mip_gap) = if status.is_success() {
        let vals: Vec<(String, f64)> = req
            .variables
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name.clone(), solver.col_value(col_indices[i])))
            .collect();
        let obj = solver.objective_value() * sign;
        let gap = solver.mip_gap();
        (vals, obj, gap)
    } else {
        (vec![], 0.0, f64::INFINITY)
    };

    MipPlan {
        request_id: req.id.clone(),
        status: status_str.to_string(),
        values,
        objective_value,
        mip_gap,
        solver: "highs-v1.14.0".to_string(),
    }
}

fn invalid_plan(req: &MipRequest) -> MipPlan {
    MipPlan {
        request_id: req.id.clone(),
        status: "invalid".to_string(),
        values: Vec::new(),
        objective_value: 0.0,
        mip_gap: f64::INFINITY,
        solver: "highs-v1.14.0".to_string(),
    }
}

fn validate_mip_request(req: &MipRequest) -> Result<(), String> {
    let mut vars = HashSet::new();
    for var in &req.variables {
        if var.name.trim().is_empty() {
            return Err("variable name must not be empty".to_string());
        }
        if !vars.insert(var.name.as_str()) {
            return Err(format!("duplicate variable '{}'", var.name));
        }
        if var.lb > var.ub {
            return Err(format!("variable '{}' has lb > ub", var.name));
        }
    }

    validate_mip_terms(&req.objective.terms, &vars, "objective")?;

    for constraint in &req.constraints {
        if constraint.lb > constraint.ub {
            return Err(format!("constraint '{}' has lb > ub", constraint.name));
        }
        validate_mip_terms(&constraint.terms, &vars, "constraint")?;
    }

    Ok(())
}

fn validate_mip_terms(
    terms: &[MipTerm],
    vars: &HashSet<&str>,
    label: &'static str,
) -> Result<(), String> {
    for term in terms {
        if !vars.contains(term.var.as_str()) {
            return Err(format!(
                "{label} references unknown variable '{}'",
                term.var
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::doc_markdown,
    clippy::mistyped_literal_suffixes,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;
    use crate::mip::problem::{MipConstraint, MipObjective, MipTerm, MipVariable, VarKind};
    use crate::test_support::MockContext;

    fn binary(name: &str) -> MipVariable {
        MipVariable {
            name: name.into(),
            lb: 0.0,
            ub: 1.0,
            kind: VarKind::Binary,
        }
    }

    fn cont(name: &str, lb: f64, ub: f64) -> MipVariable {
        MipVariable {
            name: name.into(),
            lb,
            ub,
            kind: VarKind::Continuous,
        }
    }

    fn term(var: &str, coeff: f64) -> MipTerm {
        MipTerm {
            var: var.into(),
            coeff,
        }
    }

    fn knapsack(n: usize, capacity: f64, weights: &[f64], values: &[f64]) -> MipRequest {
        let variables: Vec<_> = (0..n).map(|i| binary(&format!("x{i}"))).collect();
        let weight_terms: Vec<_> = (0..n).map(|i| term(&format!("x{i}"), weights[i])).collect();
        let value_terms: Vec<_> = (0..n).map(|i| term(&format!("x{i}"), values[i])).collect();

        MipRequest {
            id: "kp".into(),
            variables,
            constraints: vec![MipConstraint {
                name: "cap".into(),
                lb: f64::NEG_INFINITY,
                ub: capacity,
                terms: weight_terms,
            }],
            objective: MipObjective {
                terms: value_terms,
                maximize: true,
            },
            time_limit_seconds: Some(2.0),
            mip_gap_tolerance: None,
        }
    }

    #[test]
    fn solves_small_knapsack_optimally() {
        let req = knapsack(4, 10.0, &[2.0, 3.0, 4.0, 5.0], &[3.0, 4.0, 5.0, 6.0]);
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "optimal");
        assert!(plan.objective_value > 0.0);
        assert_eq!(plan.values.len(), 4);
    }

    #[test]
    fn continuous_lp_via_mip_path() {
        // pure-continuous problem: max x s.t. x + y <= 4, x,y in [0, 5]
        let req = MipRequest {
            id: "lp".into(),
            variables: vec![cont("x", 0.0, 5.0), cont("y", 0.0, 5.0)],
            constraints: vec![MipConstraint {
                name: "c".into(),
                lb: f64::NEG_INFINITY,
                ub: 4.0,
                terms: vec![term("x", 1.0), term("y", 1.0)],
            }],
            objective: MipObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: None,
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "optimal");
        assert!((plan.objective_value - 4.0).abs() < 1e-6);
    }

    #[test]
    fn integer_var_path() {
        // integer variable with explicit kind exercises add_int_col.
        let req = MipRequest {
            id: "int".into(),
            variables: vec![MipVariable {
                name: "x".into(),
                lb: 0.0,
                ub: 10.0,
                kind: VarKind::Integer,
            }],
            constraints: vec![MipConstraint {
                name: "c".into(),
                lb: f64::NEG_INFINITY,
                ub: 7.5,
                terms: vec![term("x", 1.0)],
            }],
            objective: MipObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: Some(0.001),
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "optimal");
        let map: HashMap<_, _> = plan.values.iter().cloned().collect();
        assert!((map["x"] - 7.0).abs() < 1e-6);
    }

    #[test]
    fn detects_infeasible_mip() {
        let req = MipRequest {
            id: "inf".into(),
            variables: vec![binary("x")],
            constraints: vec![MipConstraint {
                name: "low".into(),
                lb: 2.0,
                ub: f64::INFINITY,
                terms: vec![term("x", 1.0)],
            }],
            objective: MipObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: None,
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "infeasible");
        assert!(plan.mip_gap.is_infinite());
        assert_eq!(plan.values.len(), 0);
    }

    #[test]
    fn rejects_unknown_constraint_reference_as_invalid() {
        let req = MipRequest {
            id: "bad-constraint".into(),
            variables: vec![binary("x")],
            constraints: vec![MipConstraint {
                name: "bad".into(),
                lb: f64::NEG_INFINITY,
                ub: 1.0,
                terms: vec![term("missing", 1.0)],
            }],
            objective: MipObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: None,
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "invalid");
        assert!(plan.values.is_empty());
    }

    #[test]
    fn rejects_unknown_objective_reference_as_invalid() {
        let req = MipRequest {
            id: "bad-objective".into(),
            variables: vec![binary("x")],
            constraints: vec![],
            objective: MipObjective {
                terms: vec![term("missing", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: None,
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "invalid");
        assert!(plan.values.is_empty());
    }

    #[test]
    fn minimize_path() {
        // min x s.t. x >= 3, x in [0,10]; optimal x=3, obj=3.
        let req = MipRequest {
            id: "min".into(),
            variables: vec![cont("x", 0.0, 10.0)],
            constraints: vec![MipConstraint {
                name: "c".into(),
                lb: 3.0,
                ub: f64::INFINITY,
                terms: vec![term("x", 1.0)],
            }],
            objective: MipObjective {
                terms: vec![term("x", 1.0)],
                maximize: false,
            },
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: None,
        };
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "optimal");
        assert!((plan.objective_value - 3.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let req = knapsack(3, 5.0, &[1.0, 2.0, 3.0], &[5.0, 4.0, 3.0]);
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty().with_seed("mip-request:kp", &body);
        let s = HighsMipSuggestor;
        assert_eq!(s.name(), "HighsMipSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let req = knapsack(2, 5.0, &[1.0, 2.0], &[5.0, 4.0]);
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty()
            .with_seed("mip-request:kp", &body)
            .with_strategy("mip-plan:kp", "{}");
        let s = HighsMipSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed("mip-request:bad", "not json");
        let s = HighsMipSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: 30-second budget on a strongly-correlated 0/1 knapsack with
    /// 1000 items. Pisinger-style hard instances require value = weight + R
    /// to defeat the LP relaxation bound; HiGHS's branch-and-cut must explore
    /// a substantial part of the tree to prove optimality.
    #[test]
    fn stress_30s_correlated_knapsack_1000() {
        let n: usize = 1_000;
        let mut state: u64 = 0xFEEDFACE_DEADBEEF;
        let step = |s: &mut u64| -> u64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *s
        };
        // Strongly correlated: weights uniform in [50, 250]; values = weight + 10.
        let weights: Vec<f64> = (0..n)
            .map(|_| f64::from(((step(&mut state) >> 33) & 0xFF) as u32) + 50.0)
            .collect();
        let values: Vec<f64> = weights.iter().map(|w| w + 10.0).collect();
        // Tight capacity: 30% of total weight forces hard packing decisions.
        let capacity: f64 = weights.iter().sum::<f64>() * 0.30;

        let mut req = knapsack(n, capacity, &weights, &values);
        req.id = "stress".into();
        req.time_limit_seconds = Some(30.0);
        req.mip_gap_tolerance = Some(0.0); // demand provable optimality

        let started = std::time::Instant::now();
        let plan = solve_mip(&req);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible MIP solution, got {} in {elapsed:.1}s",
            plan.status
        );
        assert_eq!(plan.values.len(), n);
        assert!(plan.objective_value > 0.0);
    }
}
