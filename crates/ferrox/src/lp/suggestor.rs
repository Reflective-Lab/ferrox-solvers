use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, ProvenanceSource, Suggestor,
};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::{LinearSolver, OrtoolsError};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::provenance::FERROX_PROVENANCE;
use crate::solver_identity::glop_solver_identity;

use super::problem::{LpPlan, LpRequest};

const REQUEST_PREFIX: &str = "glop-request:";
const PLAN_PREFIX: &str = "glop-plan:";

pub struct GlopLpSuggestor;

#[async_trait]
impl Suggestor for GlopLpSuggestor {
    fn name(&self) -> &'static str {
        "GlopLpSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("LP simplex; polynomial in practice; GLOP v9.15")
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

            match fact.require_payload::<LpRequest>() {
                Ok(req) => {
                    let plan = solve_lp(req);
                    let confidence = match plan.status.as_str() {
                        "optimal" => 1.0,
                        "feasible" => 0.7,
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
                    warn!(id = %fact.id(), error = %e, "unexpected glop-request payload");
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

pub fn solve_lp(req: &LpRequest) -> LpPlan {
    if let Err(reason) = validate_lp_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid glop-request");
        return empty_plan(req, "invalid");
    }

    match solve_lp_checked(req) {
        Ok(plan) => plan,
        Err(OrtoolsError::InvalidInput(reason)) => {
            warn!(request_id = %req.id, reason = %reason, "invalid glop-request");
            empty_plan(req, "invalid")
        }
        Err(error) => {
            warn!(request_id = %req.id, error = %error, "GLOP native solve failed");
            empty_plan(req, "error")
        }
    }
}

fn solve_lp_checked(req: &LpRequest) -> Result<LpPlan, OrtoolsError> {
    let mut solver = LinearSolver::try_new_glop(&req.id)?;
    let mut name_to_idx: HashMap<String, i32> = HashMap::new();

    for var in &req.variables {
        let idx = solver.try_num_var(var.lb, var.ub, &var.name)?;
        name_to_idx.insert(var.name.clone(), idx);
    }

    for con in &req.constraints {
        let ci = solver.try_add_constraint(con.lb, con.ub, &con.name)?;
        for term in &con.terms {
            let vi = variable_index(&name_to_idx, &term.var)?;
            solver.try_set_constraint_coeff(ci, vi, term.coeff)?;
        }
    }

    for term in &req.objective.terms {
        let vi = variable_index(&name_to_idx, &term.var)?;
        solver.try_set_objective_coeff(vi, term.coeff)?;
    }

    if req.objective.maximize {
        solver.maximize();
    } else {
        solver.minimize();
    }

    let solve_status = solver.try_solve()?;
    let status = match solve_status {
        OrtoolsStatus::Optimal => "optimal",
        OrtoolsStatus::Feasible => "feasible",
        OrtoolsStatus::Infeasible => "infeasible",
        OrtoolsStatus::Unbounded => "unbounded",
        _ => "error",
    };

    let values: Vec<(String, f64)> = if solve_status.is_success() {
        req.variables
            .iter()
            .map(|v| {
                let vi = variable_index(&name_to_idx, &v.name)?;
                Ok((v.name.clone(), solver.try_var_value(vi)?))
            })
            .collect::<Result<_, OrtoolsError>>()?
    } else {
        Vec::new()
    };

    let objective_value = if solve_status.is_success() {
        solver.try_objective_value()?
    } else {
        0.0
    };

    Ok(LpPlan {
        request_id: req.id.clone(),
        status: status.to_string(),
        values,
        objective_value,
        solver: "glop-v9.15".to_string(),
        execution_identity: lp_identity(req),
    })
}

fn variable_index(vars: &HashMap<String, i32>, name: &str) -> Result<i32, OrtoolsError> {
    vars.get(name).copied().ok_or_else(|| {
        OrtoolsError::InvalidInput(format!("LP term references unknown variable '{name}'"))
    })
}

fn validate_lp_request(req: &LpRequest) -> Result<(), String> {
    validate_name(&req.id, "request id")?;
    if let Some(time_limit) = req.time_limit_seconds
        && (!time_limit.is_finite() || time_limit <= 0.0)
    {
        return Err("time_limit_seconds must be finite and positive".to_string());
    }

    let mut vars = HashSet::new();
    for var in &req.variables {
        validate_name(&var.name, "variable name")?;
        if !vars.insert(var.name.as_str()) {
            return Err(format!("duplicate variable '{}'", var.name));
        }
        validate_bounds(var.lb, var.ub, "variable bounds")?;
    }

    for constraint in &req.constraints {
        validate_name(&constraint.name, "constraint name")?;
        validate_bounds(constraint.lb, constraint.ub, "constraint bounds")?;
        validate_terms(&constraint.terms, &vars)?;
    }
    validate_terms(&req.objective.terms, &vars)?;

    Ok(())
}

fn validate_name(name: &str, label: &'static str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if name.contains('\0') {
        return Err(format!("{label} contains an interior NUL byte"));
    }
    Ok(())
}

fn validate_bounds(lb: f64, ub: f64, label: &'static str) -> Result<(), String> {
    if lb.is_nan() || ub.is_nan() {
        return Err(format!("{label} must not be NaN"));
    }
    if lb > ub {
        return Err(format!("{label} lower bound exceeds upper bound"));
    }
    Ok(())
}

fn validate_terms(terms: &[super::problem::LpTerm], vars: &HashSet<&str>) -> Result<(), String> {
    for term in terms {
        if !term.coeff.is_finite() {
            return Err(format!(
                "term for variable '{}' has non-finite coefficient",
                term.var
            ));
        }
        if !vars.contains(term.var.as_str()) {
            return Err(format!("term references unknown variable '{}'", term.var));
        }
    }
    Ok(())
}

fn empty_plan(req: &LpRequest, status: &'static str) -> LpPlan {
    LpPlan {
        request_id: req.id.clone(),
        status: status.to_string(),
        values: Vec::new(),
        objective_value: 0.0,
        solver: "glop-v9.15".to_string(),
        execution_identity: lp_identity(req),
    }
}

fn lp_identity(req: &LpRequest) -> ExecutionIdentity {
    let time_limit = req
        .time_limit_seconds
        .map_or_else(|| "none".to_string(), |limit| limit.to_string());
    glop_solver_identity(format!(
        "time_limit_seconds={time_limit}; time_limit_applied=false; maximize={}",
        req.objective.maximize
    ))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::lp::problem::{LpConstraint, LpObjective, LpTerm, LpVariable};
    use crate::test_support::MockContext;
    use converge_pack::TextPayload;

    fn var(name: &str, lb: f64, ub: f64) -> LpVariable {
        LpVariable {
            name: name.into(),
            lb,
            ub,
        }
    }

    fn term(var: &str, coeff: f64) -> LpTerm {
        LpTerm {
            var: var.into(),
            coeff,
        }
    }

    #[test]
    fn maximize_simple_lp() {
        // max x + y s.t. x + 2y <= 4, 3x + y <= 6, x,y >= 0  → x=8/5, y=6/5, obj=14/5
        let req = LpRequest {
            id: "max".into(),
            variables: vec![var("x", 0.0, 100.0), var("y", 0.0, 100.0)],
            constraints: vec![
                LpConstraint {
                    name: "c1".into(),
                    lb: f64::NEG_INFINITY,
                    ub: 4.0,
                    terms: vec![term("x", 1.0), term("y", 2.0)],
                },
                LpConstraint {
                    name: "c2".into(),
                    lb: f64::NEG_INFINITY,
                    ub: 6.0,
                    terms: vec![term("x", 3.0), term("y", 1.0)],
                },
            ],
            objective: LpObjective {
                terms: vec![term("x", 1.0), term("y", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "optimal");
        assert!((plan.objective_value - 14.0 / 5.0).abs() < 1e-6);
        assert_eq!(plan.solver, "glop-v9.15");
        assert_eq!(plan.execution_identity.backend, "glop-v9.15");
        assert!(
            plan.execution_identity
                .native_identity
                .as_ref()
                .is_some_and(|native| native.backend.contains("OR-Tools"))
        );
    }

    #[test]
    fn minimize_path() {
        // min x s.t. x >= 2, x <= 5
        let req = LpRequest {
            id: "min".into(),
            variables: vec![var("x", 0.0, 5.0)],
            constraints: vec![LpConstraint {
                name: "c".into(),
                lb: 2.0,
                ub: f64::INFINITY,
                terms: vec![term("x", 1.0)],
            }],
            objective: LpObjective {
                terms: vec![term("x", 1.0)],
                maximize: false,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "optimal");
        assert!((plan.objective_value - 2.0).abs() < 1e-6);
    }

    #[test]
    fn detects_infeasible_lp() {
        // x >= 5 AND x <= 1 → infeasible
        let req = LpRequest {
            id: "inf".into(),
            variables: vec![var("x", 0.0, 1.0)],
            constraints: vec![LpConstraint {
                name: "c".into(),
                lb: 5.0,
                ub: f64::INFINITY,
                terms: vec![term("x", 1.0)],
            }],
            objective: LpObjective {
                terms: vec![term("x", 1.0)],
                maximize: false,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "infeasible");
        assert_eq!(plan.objective_value, 0.0);
    }

    #[test]
    fn rejects_unknown_constraint_variable() {
        let req = LpRequest {
            id: "bad-ref".into(),
            variables: vec![var("x", 0.0, 1.0)],
            constraints: vec![LpConstraint {
                name: "c".into(),
                lb: f64::NEG_INFINITY,
                ub: 1.0,
                terms: vec![term("missing", 1.0)],
            }],
            objective: LpObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "invalid");
        assert!(plan.values.is_empty());
    }

    #[test]
    fn rejects_unknown_objective_variable() {
        let req = LpRequest {
            id: "bad-obj".into(),
            variables: vec![var("x", 0.0, 1.0)],
            constraints: vec![],
            objective: LpObjective {
                terms: vec![term("missing", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "invalid");
    }

    #[test]
    fn rejects_non_finite_lp_coefficient() {
        let req = LpRequest {
            id: "bad-coeff".into(),
            variables: vec![var("x", 0.0, 1.0)],
            constraints: vec![],
            objective: LpObjective {
                terms: vec![term("x", f64::NAN)],
                maximize: true,
            },
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_lp(&req);
        assert_eq!(plan.status, "invalid");
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let req = LpRequest {
            id: "s".into(),
            variables: vec![var("x", 0.0, 10.0)],
            constraints: vec![],
            objective: LpObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: Some(0.5),
        };
        let ctx = MockContext::empty().with_seed("glop-request:s", req);
        let s = GlopLpSuggestor;
        assert_eq!(s.name(), "GlopLpSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let req = LpRequest {
            id: "s2".into(),
            variables: vec![var("x", 0.0, 10.0)],
            constraints: vec![],
            objective: LpObjective {
                terms: vec![term("x", 1.0)],
                maximize: true,
            },
            time_limit_seconds: None,
        };
        let ctx = MockContext::empty()
            .with_seed("glop-request:s2", req)
            .with_strategy("glop-plan:s2", TextPayload::new("existing"));
        let s = GlopLpSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty()
            .with_seed("glop-request:bad", TextPayload::new("not a glop request"));
        let s = GlopLpSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: large-scale dense LP (transportation-style) with a 30 s budget.
    /// 200×200 = 40 000 variables, dense constraints — GLOP's simplex
    /// substantially exercises its column-generation and pricing heuristics.
    #[test]
    fn stress_30s_large_lp() {
        let dim: usize = 200;
        let n_vars = dim * dim;

        let mut state: u64 = 0xABCD_1234_5678_9ABC;
        let step = |s: &mut u64| -> f64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            f64::from(((*s >> 33) & 0xFF) as u32) / 10.0 + 1.0
        };

        let mut variables = Vec::with_capacity(n_vars);
        for i in 0..dim {
            for j in 0..dim {
                variables.push(var(&format!("x_{i}_{j}"), 0.0, 100.0));
            }
        }

        let mut constraints = Vec::with_capacity(2 * dim);
        // Row supply constraints: Σ_j x_ij ≤ supply_i
        for i in 0..dim {
            let supply = 50.0 + step(&mut state);
            let terms: Vec<_> = (0..dim).map(|j| term(&format!("x_{i}_{j}"), 1.0)).collect();
            constraints.push(LpConstraint {
                name: format!("supply_{i}"),
                lb: f64::NEG_INFINITY,
                ub: supply,
                terms,
            });
        }
        // Column demand constraints: Σ_i x_ij ≥ demand_j
        for j in 0..dim {
            let demand = 10.0 + step(&mut state) * 0.5;
            let terms: Vec<_> = (0..dim).map(|i| term(&format!("x_{i}_{j}"), 1.0)).collect();
            constraints.push(LpConstraint {
                name: format!("demand_{j}"),
                lb: demand,
                ub: f64::INFINITY,
                terms,
            });
        }

        // Random transportation costs.
        let mut obj_terms = Vec::with_capacity(n_vars);
        for i in 0..dim {
            for j in 0..dim {
                obj_terms.push(term(&format!("x_{i}_{j}"), step(&mut state)));
            }
        }

        let req = LpRequest {
            id: "stress".into(),
            variables,
            constraints,
            objective: LpObjective {
                terms: obj_terms,
                maximize: false,
            },
            time_limit_seconds: Some(30.0),
        };
        let started = std::time::Instant::now();
        let plan = solve_lp(&req);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible/optimal LP, got {} in {elapsed:.1}s",
            plan.status
        );
        assert_eq!(plan.values.len(), n_vars);
    }
}
