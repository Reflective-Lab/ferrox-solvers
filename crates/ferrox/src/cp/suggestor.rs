use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, ProvenanceSource, Suggestor,
};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::{CpModel, OrtoolsError};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::provenance::FERROX_PROVENANCE;
use crate::solver_identity::cp_sat_solver_identity;

use super::problem::{ConstraintKind, CpBoolLiteral, CpSatPlan, CpSatRequest, CpSolveStatus, CpTerm};

const REQUEST_PREFIX: &str = "cpsat-request:";
const PLAN_PREFIX: &str = "cpsat-plan:";

pub struct CpSatSuggestor;

#[async_trait]
impl Suggestor for CpSatSuggestor {
    fn name(&self) -> &'static str {
        "CpSatSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("NP-hard in general; CP-SAT DPLL+propagation+LNS; practical for n<=500 vars")
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

            match fact.require_payload::<CpSatRequest>() {
                Ok(req) => {
                    let plan = solve_cp(req);
                    let confidence = match plan.status {
                        CpSolveStatus::Optimal => 1.0,
                        CpSolveStatus::Feasible => 0.7,
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
                    warn!(id = %fact.id(), error = %e, "unexpected cpsat-request payload");
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
pub fn solve_cp(req: &CpSatRequest) -> CpSatPlan {
    if let Err(reason) = validate_cp_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid cpsat-request");
        return invalid_plan(req);
    }

    match solve_cp_checked(req) {
        Ok(plan) => plan,
        Err(OrtoolsError::InvalidInput(reason)) => {
            warn!(request_id = %req.id, reason = %reason, "invalid cpsat-request");
            invalid_plan(req)
        }
        Err(error) => {
            warn!(request_id = %req.id, error = %error, "CP-SAT native solve failed");
            error_plan(req)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn solve_cp_checked(req: &CpSatRequest) -> Result<CpSatPlan, OrtoolsError> {
    let mut model = CpModel::try_new()?;
    let mut name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut bool_name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut interval_name_to_idx: HashMap<String, i32> = HashMap::new();

    for var in &req.variables {
        let idx = if var.is_bool {
            let i = model.try_new_bool_var(&var.name)?;
            bool_name_to_idx.insert(var.name.clone(), i);
            i
        } else {
            model.try_new_int_var(var.lb, var.ub, &var.name)?
        };
        name_to_idx.insert(var.name.clone(), idx);
    }

    for ivd in &req.interval_vars {
        let s = cp_var_index(&name_to_idx, &ivd.start_var)?;
        let e = cp_var_index(&name_to_idx, &ivd.end_var)?;
        let idx = model.try_new_fixed_interval_var(s, ivd.duration, e, &ivd.name)?;
        interval_name_to_idx.insert(ivd.name.clone(), idx);
    }

    for ovd in &req.optional_interval_vars {
        let s = cp_var_index(&name_to_idx, &ovd.start_var)?;
        let e = cp_var_index(&name_to_idx, &ovd.end_var)?;
        let lit = cp_var_index(&bool_name_to_idx, &ovd.lit_var)?;
        let idx = model.try_new_optional_interval_var(s, ovd.duration, e, lit, &ovd.name)?;
        interval_name_to_idx.insert(ovd.name.clone(), idx);
    }

    for constraint in &req.constraints {
        match constraint {
            ConstraintKind::LinearLe { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx)?;
                model.try_add_linear_le(&vars, &coeffs, *rhs)?;
            }
            ConstraintKind::LinearGe { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx)?;
                model.try_add_linear_ge(&vars, &coeffs, *rhs)?;
            }
            ConstraintKind::LinearEq { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx)?;
                model.try_add_linear_eq(&vars, &coeffs, *rhs)?;
            }
            ConstraintKind::AllDifferent { vars } => {
                let idxs = vars_to_idxs(vars, &name_to_idx)?;
                model.try_add_all_different(&idxs)?;
            }
            ConstraintKind::BoolOr { literals } => {
                let lits = bool_literals_to_refs(literals, &bool_name_to_idx)?;
                model.try_add_bool_or(&lits)?;
            }
            ConstraintKind::BoolAnd { literals } => {
                let lits = bool_literals_to_refs(literals, &bool_name_to_idx)?;
                model.try_add_bool_and(&lits)?;
            }
            ConstraintKind::BoolXor { literals } => {
                let lits = bool_literals_to_refs(literals, &bool_name_to_idx)?;
                model.try_add_bool_xor(&lits)?;
            }
            ConstraintKind::Implication {
                antecedent,
                consequent,
            } => {
                let lhs = bool_literal_to_ref(antecedent, &bool_name_to_idx)?;
                let rhs = bool_literal_to_ref(consequent, &bool_name_to_idx)?;
                model.try_add_implication(lhs, rhs)?;
            }
            ConstraintKind::AtMostOne { literals } => {
                let lits = bool_literals_to_refs(literals, &bool_name_to_idx)?;
                model.try_add_at_most_one(&lits)?;
            }
            ConstraintKind::ExactlyOne { literals } => {
                let lits = bool_literals_to_refs(literals, &bool_name_to_idx)?;
                model.try_add_exactly_one(&lits)?;
            }
            ConstraintKind::AllowedAssignments { vars, tuples } => {
                let idxs = vars_to_idxs(vars, &name_to_idx)?;
                model.try_add_allowed_assignments(&idxs, tuples)?;
            }
            ConstraintKind::NoOverlap { intervals } => {
                let idxs = vars_to_idxs(intervals, &interval_name_to_idx)?;
                model.try_add_no_overlap(&idxs)?;
            }
            ConstraintKind::Cumulative { demands, capacity } => {
                let mut interval_idxs = Vec::with_capacity(demands.len());
                let mut demand_values = Vec::with_capacity(demands.len());
                for demand in demands {
                    interval_idxs.push(cp_var_index(&interval_name_to_idx, &demand.interval)?);
                    demand_values.push(demand.demand);
                }
                model.try_add_cumulative(&interval_idxs, &demand_values, *capacity)?;
            }
            ConstraintKind::NoOverlap2D { rectangles } => {
                let mut x_intervals = Vec::with_capacity(rectangles.len());
                let mut y_intervals = Vec::with_capacity(rectangles.len());
                for rectangle in rectangles {
                    x_intervals.push(cp_var_index(&interval_name_to_idx, &rectangle.x_interval)?);
                    y_intervals.push(cp_var_index(&interval_name_to_idx, &rectangle.y_interval)?);
                }
                model.try_add_no_overlap_2d(&x_intervals, &y_intervals)?;
            }
        }
    }

    if let Some(obj_terms) = &req.objective_terms {
        let (vars, coeffs) = terms_to_vecs(obj_terms, &name_to_idx)?;
        if req.minimize {
            model.try_minimize(&vars, &coeffs)?;
        } else {
            model.try_maximize(&vars, &coeffs)?;
        }
    }

    let time_limit = req.time_limit_seconds.unwrap_or(60.0);
    let solution = model.try_solve(time_limit)?;

    let status = match solution.status() {
        OrtoolsStatus::Optimal => CpSolveStatus::Optimal,
        OrtoolsStatus::Feasible => CpSolveStatus::Feasible,
        OrtoolsStatus::Infeasible => CpSolveStatus::Infeasible,
        OrtoolsStatus::Unbounded => CpSolveStatus::Unbounded,
        _ => CpSolveStatus::Error,
    };

    let assignments = if solution.status().is_success() {
        req.variables
            .iter()
            .map(|v| {
                let idx = cp_var_index(&name_to_idx, &v.name)?;
                Ok((v.name.clone(), solution.try_value(idx)?))
            })
            .collect::<Result<Vec<_>, OrtoolsError>>()?
    } else {
        vec![]
    };

    let objective_value = if solution.status().is_success() && req.objective_terms.is_some() {
        Some(solution.try_objective_value()?)
    } else {
        None
    };

    Ok(CpSatPlan {
        request_id: req.id.clone(),
        status,
        assignments,
        objective_value,
        wall_time_seconds: solution.wall_time(),
        solver: "cp-sat-v9.15".to_string(),
        execution_identity: cp_identity(req),
    })
}

fn invalid_plan(req: &CpSatRequest) -> CpSatPlan {
    empty_plan(req, CpSolveStatus::Invalid)
}

fn error_plan(req: &CpSatRequest) -> CpSatPlan {
    empty_plan(req, CpSolveStatus::Error)
}

fn empty_plan(req: &CpSatRequest, status: CpSolveStatus) -> CpSatPlan {
    CpSatPlan {
        request_id: req.id.clone(),
        status,
        assignments: Vec::new(),
        objective_value: None,
        wall_time_seconds: 0.0,
        solver: "cp-sat-v9.15".to_string(),
        execution_identity: cp_identity(req),
    }
}

fn cp_identity(req: &CpSatRequest) -> ExecutionIdentity {
    cp_sat_solver_identity(format!(
        "time_limit_seconds={}; search_workers=hardware_concurrency; minimize={}",
        req.time_limit_seconds.unwrap_or(60.0),
        req.minimize
    ))
}

fn validate_cp_request(req: &CpSatRequest) -> Result<(), String> {
    validate_name(&req.id, "request id")?;
    if let Some(time_limit) = req.time_limit_seconds
        && (!time_limit.is_finite() || time_limit <= 0.0)
    {
        return Err("time_limit_seconds must be finite and positive".to_string());
    }
    let (vars, bools) = validate_cp_variables(req)?;
    let intervals = validate_cp_intervals(req, &vars, &bools)?;
    validate_cp_constraints(&req.constraints, &vars, &bools, &intervals)?;
    if let Some(objective_terms) = &req.objective_terms {
        validate_terms(objective_terms, &vars)?;
    }

    Ok(())
}

fn validate_cp_variables(req: &CpSatRequest) -> Result<(HashSet<&str>, HashSet<&str>), String> {
    let mut vars = HashSet::new();
    let mut bools = HashSet::new();

    for var in &req.variables {
        validate_name(&var.name, "variable name")?;
        if !vars.insert(var.name.as_str()) {
            return Err(format!("duplicate variable '{}'", var.name));
        }
        if var.lb > var.ub {
            return Err(format!("variable '{}' has lb > ub", var.name));
        }
        if var.is_bool {
            if var.lb != 0 || var.ub != 1 {
                return Err(format!(
                    "bool variable '{}' must have domain 0..1",
                    var.name
                ));
            }
            bools.insert(var.name.as_str());
        }
    }

    Ok((vars, bools))
}

fn validate_cp_intervals<'a>(
    req: &'a CpSatRequest,
    vars: &HashSet<&str>,
    bools: &HashSet<&str>,
) -> Result<HashSet<&'a str>, String> {
    let mut intervals = HashSet::new();
    for interval in &req.interval_vars {
        validate_interval_name(&mut intervals, &interval.name)?;
        validate_var_ref(vars, &interval.start_var, "interval start")?;
        validate_var_ref(vars, &interval.end_var, "interval end")?;
        validate_duration(interval.duration, &interval.name)?;
    }
    for interval in &req.optional_interval_vars {
        validate_interval_name(&mut intervals, &interval.name)?;
        validate_var_ref(vars, &interval.start_var, "optional interval start")?;
        validate_var_ref(vars, &interval.end_var, "optional interval end")?;
        validate_var_ref(bools, &interval.lit_var, "optional interval literal")?;
        validate_duration(interval.duration, &interval.name)?;
    }

    Ok(intervals)
}

fn validate_cp_constraints(
    constraints: &[ConstraintKind],
    vars: &HashSet<&str>,
    bools: &HashSet<&str>,
    intervals: &HashSet<&str>,
) -> Result<(), String> {
    for constraint in constraints {
        validate_cp_constraint(constraint, vars, bools, intervals)?;
    }
    Ok(())
}

fn validate_cp_constraint(
    constraint: &ConstraintKind,
    vars: &HashSet<&str>,
    bools: &HashSet<&str>,
    intervals: &HashSet<&str>,
) -> Result<(), String> {
    match constraint {
        ConstraintKind::LinearLe { terms, .. }
        | ConstraintKind::LinearGe { terms, .. }
        | ConstraintKind::LinearEq { terms, .. } => validate_terms(terms, vars),
        ConstraintKind::AllDifferent {
            vars: constraint_vars,
        } => validate_var_refs(vars, constraint_vars, "AllDifferent"),
        ConstraintKind::BoolOr { literals }
        | ConstraintKind::BoolAnd { literals }
        | ConstraintKind::BoolXor { literals }
        | ConstraintKind::AtMostOne { literals }
        | ConstraintKind::ExactlyOne { literals } => validate_literals(literals, bools),
        ConstraintKind::Implication {
            antecedent,
            consequent,
        } => {
            validate_literal(antecedent, bools)?;
            validate_literal(consequent, bools)
        }
        ConstraintKind::AllowedAssignments {
            vars: constraint_vars,
            tuples,
        } => validate_allowed_assignments(vars, constraint_vars, tuples),
        ConstraintKind::NoOverlap {
            intervals: constraint_intervals,
        } => validate_var_refs(intervals, constraint_intervals, "NoOverlap"),
        ConstraintKind::Cumulative { demands, capacity } => {
            validate_cumulative(intervals, demands, *capacity)
        }
        ConstraintKind::NoOverlap2D { rectangles } => {
            for rectangle in rectangles {
                validate_var_ref(intervals, &rectangle.x_interval, "NoOverlap2D x interval")?;
                validate_var_ref(intervals, &rectangle.y_interval, "NoOverlap2D y interval")?;
            }
            Ok(())
        }
    }
}

fn validate_allowed_assignments(
    vars: &HashSet<&str>,
    constraint_vars: &[String],
    tuples: &[Vec<i64>],
) -> Result<(), String> {
    validate_var_refs(vars, constraint_vars, "AllowedAssignments")?;
    if let Some(tuple) = tuples
        .iter()
        .find(|tuple| tuple.len() != constraint_vars.len())
    {
        return Err(format!(
            "AllowedAssignments tuple arity {} does not match variable count {}",
            tuple.len(),
            constraint_vars.len()
        ));
    }
    Ok(())
}

fn validate_cumulative(
    intervals: &HashSet<&str>,
    demands: &[super::problem::CumulativeDemand],
    capacity: i64,
) -> Result<(), String> {
    if capacity < 0 {
        return Err("Cumulative capacity must be non-negative".to_string());
    }
    for demand in demands {
        validate_var_ref(intervals, &demand.interval, "Cumulative interval")?;
        if demand.demand < 0 {
            return Err(format!(
                "Cumulative demand for '{}' must be non-negative",
                demand.interval
            ));
        }
    }
    Ok(())
}

fn validate_interval_name<'a>(
    intervals: &mut HashSet<&'a str>,
    name: &'a str,
) -> Result<(), String> {
    validate_name(name, "interval name")?;
    if intervals.insert(name) {
        Ok(())
    } else {
        Err(format!("duplicate interval '{name}'"))
    }
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

fn validate_duration(duration: i64, name: &str) -> Result<(), String> {
    if duration < 0 {
        Err(format!("interval '{name}' has negative duration"))
    } else {
        Ok(())
    }
}

fn validate_terms(terms: &[CpTerm], vars: &HashSet<&str>) -> Result<(), String> {
    for term in terms {
        validate_var_ref(vars, &term.var, "linear term")?;
    }
    Ok(())
}

fn validate_literals(literals: &[CpBoolLiteral], bools: &HashSet<&str>) -> Result<(), String> {
    for literal in literals {
        validate_literal(literal, bools)?;
    }
    Ok(())
}

fn validate_literal(literal: &CpBoolLiteral, bools: &HashSet<&str>) -> Result<(), String> {
    validate_var_ref(bools, &literal.var, "bool literal")
}

fn validate_var_refs(
    known: &HashSet<&str>,
    refs: &[String],
    label: &'static str,
) -> Result<(), String> {
    for name in refs {
        validate_var_ref(known, name, label)?;
    }
    Ok(())
}

fn validate_var_ref(known: &HashSet<&str>, name: &str, label: &'static str) -> Result<(), String> {
    if known.contains(name) {
        Ok(())
    } else {
        Err(format!("{label} references unknown name '{name}'"))
    }
}

fn terms_to_vecs(
    terms: &[CpTerm],
    name_to_idx: &HashMap<String, i32>,
) -> Result<(Vec<i32>, Vec<i64>), OrtoolsError> {
    let mut vars = Vec::with_capacity(terms.len());
    let mut coeffs = Vec::with_capacity(terms.len());
    for term in terms {
        vars.push(cp_var_index(name_to_idx, &term.var)?);
        coeffs.push(term.coeff);
    }
    Ok((vars, coeffs))
}

fn vars_to_idxs(
    vars: &[String],
    name_to_idx: &HashMap<String, i32>,
) -> Result<Vec<i32>, OrtoolsError> {
    let mut idxs = Vec::with_capacity(vars.len());
    for var in vars {
        idxs.push(cp_var_index(name_to_idx, var)?);
    }
    Ok(idxs)
}

fn cp_var_index(name_to_idx: &HashMap<String, i32>, name: &str) -> Result<i32, OrtoolsError> {
    name_to_idx.get(name).copied().ok_or_else(|| {
        OrtoolsError::InvalidInput(format!("CP-SAT request references unknown name '{name}'"))
    })
}

fn bool_literals_to_refs(
    literals: &[CpBoolLiteral],
    bool_name_to_idx: &HashMap<String, i32>,
) -> Result<Vec<i32>, OrtoolsError> {
    let mut refs = Vec::with_capacity(literals.len());
    for literal in literals {
        let lit_ref = bool_literal_to_ref(literal, bool_name_to_idx)?;
        refs.push(lit_ref);
    }
    Ok(refs)
}

fn bool_literal_to_ref(
    literal: &CpBoolLiteral,
    bool_name_to_idx: &HashMap<String, i32>,
) -> Result<i32, OrtoolsError> {
    let idx = cp_var_index(bool_name_to_idx, &literal.var)?;
    Ok(if literal.negated { -idx - 1 } else { idx })
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_wrap,
    clippy::mistyped_literal_suffixes,
    clippy::unreadable_literal,
    clippy::similar_names
)]
mod tests {
    use super::*;
    use crate::cp::problem::{
        CpBoolLiteral, CpSolveStatus, CpTerm, CpVariable, CumulativeDemand, IntervalVarDef,
        NoOverlap2DRectangle, OptionalIntervalVarDef,
    };
    use crate::test_support::MockContext;
    use converge_pack::TextPayload;
    use proptest::prelude::*;

    fn var(name: &str, lb: i64, ub: i64) -> CpVariable {
        CpVariable {
            name: name.into(),
            lb,
            ub,
            is_bool: false,
        }
    }

    fn bool_var(name: &str) -> CpVariable {
        CpVariable {
            name: name.into(),
            lb: 0,
            ub: 1,
            is_bool: true,
        }
    }

    fn term(var: &str, coeff: i64) -> CpTerm {
        CpTerm {
            var: var.into(),
            coeff,
        }
    }

    fn lit(var: &str) -> CpBoolLiteral {
        CpBoolLiteral {
            var: var.into(),
            negated: false,
        }
    }

    fn not_lit(var: &str) -> CpBoolLiteral {
        CpBoolLiteral {
            var: var.into(),
            negated: true,
        }
    }

    fn fixed_bool(var: &str, value: bool) -> ConstraintKind {
        ConstraintKind::BoolAnd {
            literals: vec![if value { lit(var) } else { not_lit(var) }],
        }
    }

    #[test]
    fn solves_n_queens_4() {
        // 4-queens via AllDifferent on row+col diagonals; ensures AllDifferent path covered.
        let n: i64 = 4;
        let mut variables = Vec::new();
        for i in 0..n {
            variables.push(var(&format!("q{i}"), 0, n - 1));
        }
        let constraints = vec![ConstraintKind::AllDifferent {
            vars: (0..n).map(|i| format!("q{i}")).collect(),
        }];
        let req = CpSatRequest {
            id: "queens".into(),
            variables,
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints,
            objective_terms: None,
            minimize: true,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert!(plan.status.is_successful());
        assert_eq!(plan.assignments.len(), 4);
        assert_eq!(plan.solver, "cp-sat-v9.15");
        assert_eq!(plan.execution_identity.backend, "cp-sat-v9.15");
        assert!(
            plan.execution_identity
                .native_identity
                .as_ref()
                .is_some_and(|native| native.backend.contains("OR-Tools"))
        );
    }

    #[test]
    fn maximizes_simple_linear() {
        let req = CpSatRequest {
            id: "max".into(),
            variables: vec![var("x", 0, 10), var("y", 0, 10)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::LinearLe {
                terms: vec![term("x", 1), term("y", 1)],
                rhs: 8,
            }],
            objective_terms: Some(vec![term("x", 3), term("y", 2)]),
            minimize: false,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        // optimal: maximize 3x+2y s.t. x+y<=8, x,y in [0,10] -> x=8, y=0, obj=24
        assert_eq!(plan.objective_value, Some(24));
    }

    #[test]
    fn detects_infeasible() {
        let req = CpSatRequest {
            id: "inf".into(),
            variables: vec![var("x", 0, 5)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![
                ConstraintKind::LinearGe {
                    terms: vec![term("x", 1)],
                    rhs: 10,
                },
                ConstraintKind::LinearLe {
                    terms: vec![term("x", 1)],
                    rhs: 5,
                },
            ],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Infeasible);
        assert_eq!(plan.assignments.len(), 0);
    }

    #[test]
    fn linear_eq_constraint() {
        let req = CpSatRequest {
            id: "eq".into(),
            variables: vec![var("x", 0, 10), var("y", 0, 10)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::LinearEq {
                terms: vec![term("x", 1), term("y", 1)],
                rhs: 7,
            }],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_cp(&req);
        assert!(plan.status.is_successful());
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        assert_eq!(map["x"] + map["y"], 7);
    }

    #[test]
    fn boolean_logic_primitives() {
        let req = CpSatRequest {
            id: "bool".into(),
            variables: vec![
                bool_var("a"),
                bool_var("b"),
                bool_var("c"),
                bool_var("d"),
                bool_var("e"),
            ],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![
                ConstraintKind::ExactlyOne {
                    literals: vec![lit("a"), lit("b")],
                },
                ConstraintKind::Implication {
                    antecedent: lit("a"),
                    consequent: lit("c"),
                },
                ConstraintKind::BoolAnd {
                    literals: vec![lit("c")],
                },
                ConstraintKind::BoolXor {
                    literals: vec![lit("a"), lit("b"), lit("d")],
                },
                ConstraintKind::BoolOr {
                    literals: vec![not_lit("d"), lit("e")],
                },
                ConstraintKind::AtMostOne {
                    literals: vec![lit("a"), lit("e")],
                },
            ],
            objective_terms: Some(vec![term("a", 1)]),
            minimize: false,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        assert_eq!(map["a"], 1);
        assert_eq!(map["b"], 0);
        assert_eq!(map["c"], 1);
        assert_eq!(map["d"], 0);
        assert_eq!(map["e"], 0);
    }

    #[test]
    fn conflicting_boolean_constraints_are_infeasible() {
        let req = CpSatRequest {
            id: "bool-negative".into(),
            variables: vec![bool_var("a"), bool_var("b")],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![
                ConstraintKind::ExactlyOne {
                    literals: vec![lit("a"), lit("b")],
                },
                ConstraintKind::BoolAnd {
                    literals: vec![not_lit("a"), not_lit("b")],
                },
            ],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Infeasible);
        assert!(plan.assignments.is_empty());
    }

    proptest! {
        #[test]
        fn implication_matches_truth_table(a_value in any::<bool>(), b_value in any::<bool>()) {
            let req = CpSatRequest {
                id: format!("implication-{a_value}-{b_value}"),
                variables: vec![bool_var("a"), bool_var("b")],
                interval_vars: vec![],
                optional_interval_vars: vec![],
                constraints: vec![
                    ConstraintKind::Implication {
                        antecedent: lit("a"),
                        consequent: lit("b"),
                    },
                    fixed_bool("a", a_value),
                    fixed_bool("b", b_value),
                ],
                objective_terms: None,
                minimize: false,
                time_limit_seconds: Some(2.0),
            };
            let plan = solve_cp(&req);
            let feasible = plan.status.is_successful();
            let should_be_feasible = !a_value || b_value;
            assert_eq!(feasible, should_be_feasible, "a={a_value}, b={b_value}");
        }
    }

    #[test]
    fn allowed_assignments_table() {
        let req = CpSatRequest {
            id: "table".into(),
            variables: vec![var("x", 0, 2), var("y", 0, 2)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::AllowedAssignments {
                vars: vec!["x".into(), "y".into()],
                tuples: vec![vec![0, 2], vec![2, 0]],
            }],
            objective_terms: Some(vec![term("x", 1)]),
            minimize: true,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        assert_eq!((map["x"], map["y"]), (0, 2));
    }

    #[test]
    fn allowed_assignments_rejects_unlisted_tuple() {
        let req = CpSatRequest {
            id: "table-negative".into(),
            variables: vec![var("x", 0, 2), var("y", 0, 2)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![
                ConstraintKind::AllowedAssignments {
                    vars: vec!["x".into(), "y".into()],
                    tuples: vec![vec![0, 0]],
                },
                ConstraintKind::LinearEq {
                    terms: vec![term("x", 1)],
                    rhs: 1,
                },
            ],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Infeasible);
    }

    #[test]
    fn fixed_interval_no_overlap() {
        // Two fixed intervals on one machine; NoOverlap forces serial scheduling.
        let req = CpSatRequest {
            id: "iv".into(),
            variables: vec![
                var("s1", 0, 100),
                var("e1", 0, 100),
                var("s2", 0, 100),
                var("e2", 0, 100),
            ],
            interval_vars: vec![
                IntervalVarDef {
                    name: "iv1".into(),
                    start_var: "s1".into(),
                    duration: 5,
                    end_var: "e1".into(),
                },
                IntervalVarDef {
                    name: "iv2".into(),
                    start_var: "s2".into(),
                    duration: 7,
                    end_var: "e2".into(),
                },
            ],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::NoOverlap {
                intervals: vec!["iv1".into(), "iv2".into()],
            }],
            objective_terms: Some(vec![term("e2", 1)]),
            minimize: true,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        // Minimising e2 alone: iv2 schedules first (0..7), iv1 after (7..12). e2 = 7.
        assert_eq!(plan.objective_value, Some(7));
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        // Both intervals must be scheduled and non-overlapping.
        let (s1, e1, s2, e2) = (map["s1"], map["e1"], map["s2"], map["e2"]);
        assert_eq!(e1 - s1, 5);
        assert_eq!(e2 - s2, 7);
        assert!(e1 <= s2 || e2 <= s1, "intervals must not overlap");
    }

    #[test]
    fn cumulative_resource_limits_overlap() {
        let req = CpSatRequest {
            id: "cumulative".into(),
            variables: vec![
                var("s1", 0, 100),
                var("e1", 0, 100),
                var("s2", 0, 100),
                var("e2", 0, 100),
            ],
            interval_vars: vec![
                IntervalVarDef {
                    name: "iv1".into(),
                    start_var: "s1".into(),
                    duration: 5,
                    end_var: "e1".into(),
                },
                IntervalVarDef {
                    name: "iv2".into(),
                    start_var: "s2".into(),
                    duration: 5,
                    end_var: "e2".into(),
                },
            ],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::Cumulative {
                demands: vec![
                    CumulativeDemand {
                        interval: "iv1".into(),
                        demand: 2,
                    },
                    CumulativeDemand {
                        interval: "iv2".into(),
                        demand: 2,
                    },
                ],
                capacity: 3,
            }],
            objective_terms: Some(vec![term("e2", 1)]),
            minimize: true,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        let (s1, e1, s2, e2) = (map["s1"], map["e1"], map["s2"], map["e2"]);
        assert_eq!(e1 - s1, 5);
        assert_eq!(e2 - s2, 5);
        assert!(e1 <= s2 || e2 <= s1, "capacity forces serial tasks");
    }

    #[test]
    fn no_overlap_2d_prevents_rectangle_overlap() {
        let req = CpSatRequest {
            id: "no-overlap-2d".into(),
            variables: vec![
                var("x1s", 0, 10),
                var("x1e", 0, 10),
                var("y1s", 0, 0),
                var("y1e", 2, 2),
                var("x2s", 0, 10),
                var("x2e", 0, 10),
                var("y2s", 0, 0),
                var("y2e", 2, 2),
            ],
            interval_vars: vec![
                IntervalVarDef {
                    name: "x1".into(),
                    start_var: "x1s".into(),
                    duration: 4,
                    end_var: "x1e".into(),
                },
                IntervalVarDef {
                    name: "y1".into(),
                    start_var: "y1s".into(),
                    duration: 2,
                    end_var: "y1e".into(),
                },
                IntervalVarDef {
                    name: "x2".into(),
                    start_var: "x2s".into(),
                    duration: 4,
                    end_var: "x2e".into(),
                },
                IntervalVarDef {
                    name: "y2".into(),
                    start_var: "y2s".into(),
                    duration: 2,
                    end_var: "y2e".into(),
                },
            ],
            optional_interval_vars: vec![],
            constraints: vec![ConstraintKind::NoOverlap2D {
                rectangles: vec![
                    NoOverlap2DRectangle {
                        x_interval: "x1".into(),
                        y_interval: "y1".into(),
                    },
                    NoOverlap2DRectangle {
                        x_interval: "x2".into(),
                        y_interval: "y2".into(),
                    },
                ],
            }],
            objective_terms: Some(vec![term("x2e", 1)]),
            minimize: true,
            time_limit_seconds: Some(2.0),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Optimal);
        let map: HashMap<_, _> = plan.assignments.iter().cloned().collect();
        assert!(map["x1e"] <= map["x2s"] || map["x2e"] <= map["x1s"]);
    }

    #[test]
    fn optional_interval_uses_lit() {
        // Bool literal toggles whether the interval is active.
        let req = CpSatRequest {
            id: "ov".into(),
            variables: vec![bool_var("lit"), var("s", 0, 10), var("e", 0, 10)],
            interval_vars: vec![],
            optional_interval_vars: vec![OptionalIntervalVarDef {
                name: "ov1".into(),
                start_var: "s".into(),
                duration: 3,
                end_var: "e".into(),
                lit_var: "lit".into(),
            }],
            constraints: vec![],
            objective_terms: Some(vec![term("lit", 1)]),
            minimize: false,
            time_limit_seconds: Some(1.0),
        };
        let plan = solve_cp(&req);
        assert!(plan.status.is_successful());
    }

    #[test]
    fn rejects_unknown_var_references_as_invalid() {
        let req = CpSatRequest {
            id: "u".into(),
            variables: vec![var("s", 0, 10)],
            interval_vars: vec![IntervalVarDef {
                name: "iv".into(),
                start_var: "missing-start".into(),
                duration: 3,
                end_var: "missing-end".into(),
            }],
            optional_interval_vars: vec![OptionalIntervalVarDef {
                name: "ov".into(),
                start_var: "s".into(),
                duration: 3,
                end_var: "missing-end".into(),
                lit_var: "missing-lit".into(),
            }],
            constraints: vec![],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(0.5),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Invalid);
        assert!(plan.assignments.is_empty());
    }

    #[test]
    fn rejects_unknown_objective_reference_as_invalid() {
        let req = CpSatRequest {
            id: "bad-objective".into(),
            variables: vec![var("x", 0, 10)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![],
            objective_terms: Some(vec![term("missing", 1)]),
            minimize: true,
            time_limit_seconds: Some(0.5),
        };
        let plan = solve_cp(&req);
        assert_eq!(plan.status, CpSolveStatus::Invalid);
        assert!(plan.objective_value.is_none());
    }

    #[test]
    fn no_objective_yields_no_objective_value() {
        let req = CpSatRequest {
            id: "nobj".into(),
            variables: vec![var("x", 0, 5)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(0.5),
        };
        let plan = solve_cp(&req);
        assert!(plan.status.is_successful());
        assert!(plan.objective_value.is_none());
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let req = CpSatRequest {
            id: "s1".into(),
            variables: vec![var("x", 0, 5)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(0.5),
        };
        let ctx = MockContext::empty().with_seed("cpsat-request:s1", req);
        let s = CpSatSuggestor;
        assert_eq!(s.name(), "CpSatSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let req = CpSatRequest {
            id: "s2".into(),
            variables: vec![var("x", 0, 5)],
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints: vec![],
            objective_terms: None,
            minimize: false,
            time_limit_seconds: Some(0.5),
        };
        let ctx = MockContext::empty()
            .with_seed("cpsat-request:s2", req)
            .with_strategy("cpsat-plan:s2", TextPayload::new("existing"));
        let s = CpSatSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty()
            .with_seed("cpsat-request:bad", TextPayload::new("not a cpsat request"));
        let s = CpSatSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: 30-second budget on a hard 2D bin-packing-style assignment.
    /// 500 boolean items x 8 capacity rows with correlated coefficients:
    /// CP-SAT cannot prune via LP relaxation, so it explores the full tree.
    /// Designed to actually consume the 30 s budget.
    #[test]
    fn stress_30s_correlated_multi_knapsack() {
        let n: usize = 500;
        let bins: usize = 8;
        let mut state: u64 = 0xC0FFEE_BADCAFE;
        let step = |s: &mut u64| -> u64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *s
        };
        // Strongly-correlated knapsack: value = weight + small constant. This defeats
        // greedy/LP-relax bounds and forces deep CP-SAT search.
        let weights: Vec<i64> = (0..n)
            .map(|_| ((step(&mut state) >> 33) & 0xFF) as i64 + 50)
            .collect();
        let values: Vec<i64> = weights.iter().map(|&w| w + 10).collect();

        let variables: Vec<CpVariable> = (0..n).map(|i| bool_var(&format!("x{i}"))).collect();
        let mut constraints: Vec<ConstraintKind> = Vec::new();
        let total: i64 = weights.iter().sum();
        for b in 0..bins {
            // Per-bin capacity covers ~1/bins of the items but with overlapping
            // rotated weight subsets, creating cross-bin contention.
            let cap = total / (bins as i64) - 5;
            let terms: Vec<CpTerm> = (0..n)
                .map(|i| {
                    let w = weights[(i + b * 17) % n];
                    term(&format!("x{i}"), w)
                })
                .collect();
            constraints.push(ConstraintKind::LinearLe { terms, rhs: cap });
        }

        let obj: Vec<CpTerm> = (0..n).map(|i| term(&format!("x{i}"), values[i])).collect();

        let req = CpSatRequest {
            id: "stress".into(),
            variables,
            interval_vars: vec![],
            optional_interval_vars: vec![],
            constraints,
            objective_terms: Some(obj),
            minimize: false,
            time_limit_seconds: Some(30.0),
        };
        let started = std::time::Instant::now();
        let plan = solve_cp(&req);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            plan.status.is_successful(),
            "stress should yield a feasible solution, got {elapsed:.1}s"
        );
        assert_eq!(plan.assignments.len(), n);
        assert!(plan.objective_value.unwrap() > 0);
    }
}
