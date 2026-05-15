use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, ProvenanceSource, Suggestor,
};
use ferrox_highs_sys::HighsModelStatus;
use ferrox_highs_sys::safe::{HighsError, HighsSolver};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::provenance::FERROX_PROVENANCE;
use crate::solver_identity::highs_solver_identity;

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

    fn provenance(&self) -> &'static str {
        FERROX_PROVENANCE.as_str()
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
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

            match fact.require_payload::<MipRequest>() {
                Ok(req) => {
                    let plan = solve_mip(req);
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
                                plan,
                            )
                            .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "unexpected mip-request payload");
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
    let mut col_indices = Vec::with_capacity(req.variables.len());
    for (i, var) in req.variables.iter().enumerate() {
        let col = match var.kind {
            VarKind::Continuous => solver.try_add_col(costs[i], var.lb, var.ub),
            VarKind::Integer => solver.try_add_int_col(costs[i], var.lb, var.ub),
            VarKind::Binary => solver.try_add_bin_col(costs[i]),
        };
        match col {
            Ok(col) => col_indices.push(col),
            Err(err) => return solver_error_plan(req, &err),
        }
    }

    if let Some(tl) = req.time_limit_seconds
        && let Err(err) = solver.try_set_time_limit(tl)
    {
        return solver_error_plan(req, &err);
    }
    if let Some(gap) = req.mip_gap_tolerance
        && let Err(err) = solver.try_set_mip_rel_gap(gap)
    {
        return solver_error_plan(req, &err);
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
        if let Err(err) = solver.try_add_row(con.lb, con.ub, &indices, &vals) {
            return solver_error_plan(req, &err);
        }
    }

    let status = match solver.try_run() {
        Ok(status) => status,
        Err(err) => return solver_error_plan(req, &err),
    };
    let has_solution = solver.has_solution();

    let status_str = match status {
        HighsModelStatus::Optimal => "optimal",
        HighsModelStatus::SolutionLimit | HighsModelStatus::TimeLimit if has_solution => "feasible",
        HighsModelStatus::TimeLimit => "timeout",
        HighsModelStatus::Infeasible => "infeasible",
        HighsModelStatus::Unbounded => "unbounded",
        _ => "error",
    };

    let solution_usable = status.is_optimal()
        || (status.may_have_incumbent() && has_solution && status_str == "feasible");

    let (values, objective_value, mip_gap) = if solution_usable {
        let mut vals = Vec::with_capacity(req.variables.len());
        for (i, v) in req.variables.iter().enumerate() {
            let value = match solver.try_col_value(col_indices[i]) {
                Ok(value) => value,
                Err(err) => return solver_error_plan(req, &err),
            };
            vals.push((v.name.clone(), value));
        }
        let obj = solver.objective_value() * sign;
        let gap = match solver.try_mip_gap() {
            Ok(gap) => gap,
            Err(err) => return solver_error_plan(req, &err),
        };
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
        execution_identity: mip_identity(req),
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
        execution_identity: mip_identity(req),
    }
}

fn solver_error_plan(req: &MipRequest, err: &HighsError) -> MipPlan {
    warn!(request_id = %req.id, error = %err, "mip solver failed");
    MipPlan {
        request_id: req.id.clone(),
        status: "error".to_string(),
        values: Vec::new(),
        objective_value: 0.0,
        mip_gap: f64::INFINITY,
        solver: "highs-v1.14.0".to_string(),
        execution_identity: mip_identity(req),
    }
}

fn mip_identity(req: &MipRequest) -> ExecutionIdentity {
    let time_limit = req
        .time_limit_seconds
        .map_or_else(|| "none".to_string(), |limit| limit.to_string());
    let mip_gap_tolerance = req
        .mip_gap_tolerance
        .map_or_else(|| "none".to_string(), |gap| gap.to_string());
    highs_solver_identity(format!(
        "time_limit_seconds={time_limit}; mip_gap_tolerance={mip_gap_tolerance}; maximize={}",
        req.objective.maximize
    ))
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
        validate_bound(var.lb, "variable lower bound")?;
        validate_bound(var.ub, "variable upper bound")?;
        if var.lb > var.ub {
            return Err(format!("variable '{}' has lb > ub", var.name));
        }
    }

    validate_mip_terms(&req.objective.terms, &vars, "objective")?;

    for constraint in &req.constraints {
        validate_bound(constraint.lb, "constraint lower bound")?;
        validate_bound(constraint.ub, "constraint upper bound")?;
        if constraint.lb > constraint.ub {
            return Err(format!("constraint '{}' has lb > ub", constraint.name));
        }
        validate_mip_terms(&constraint.terms, &vars, "constraint")?;
    }

    if let Some(time_limit) = req.time_limit_seconds
        && (!time_limit.is_finite() || time_limit <= 0.0)
    {
        return Err("time_limit_seconds must be finite and > 0".to_string());
    }
    if let Some(gap) = req.mip_gap_tolerance
        && (!gap.is_finite() || gap < 0.0)
    {
        return Err("mip_gap_tolerance must be finite and >= 0".to_string());
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
        if !term.coeff.is_finite() {
            return Err(format!(
                "{label} term for '{}' has non-finite coefficient",
                term.var
            ));
        }
    }
    Ok(())
}

fn validate_bound(value: f64, label: &'static str) -> Result<(), String> {
    if value.is_nan() {
        Err(format!("{label} must not be NaN"))
    } else {
        Ok(())
    }
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
    use converge_pack::TextPayload;

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
        assert_eq!(plan.execution_identity.backend, "highs-v1.14.0");
        assert_eq!(
            plan.execution_identity
                .native_identity
                .as_ref()
                .map(|native| native.backend.as_str()),
            Some("HiGHS")
        );
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
    fn rejects_non_finite_coefficients_as_invalid() {
        let req = MipRequest {
            id: "bad-coeff".into(),
            variables: vec![binary("x")],
            constraints: vec![],
            objective: MipObjective {
                terms: vec![term("x", f64::NAN)],
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
    fn rejects_invalid_solver_options_as_invalid() {
        let mut req = knapsack(1, 1.0, &[1.0], &[1.0]);
        req.time_limit_seconds = Some(0.0);
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "invalid");

        let mut req = knapsack(1, 1.0, &[1.0], &[1.0]);
        req.mip_gap_tolerance = Some(f64::NAN);
        let plan = solve_mip(&req);
        assert_eq!(plan.status, "invalid");
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
        let ctx = MockContext::empty().with_seed("mip-request:kp", req);
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
        let ctx = MockContext::empty()
            .with_seed("mip-request:kp", req)
            .with_strategy("mip-plan:kp", TextPayload::new("existing"));
        let s = HighsMipSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty()
            .with_seed("mip-request:bad", TextPayload::new("not a mip request"));
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
