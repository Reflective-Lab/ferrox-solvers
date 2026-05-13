use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, Suggestor};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::CpModel;
use std::collections::HashMap;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};

use super::problem::{ConstraintKind, CpBoolLiteral, CpSatPlan, CpSatRequest, CpTerm};

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

            match serde_json::from_str::<CpSatRequest>(fact.content()) {
                Ok(req) => {
                    let plan = solve_cp(&req);
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
                                serde_json::to_string(&plan).unwrap_or_default(),
                            )
                            .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "malformed cpsat-request");
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
    let mut model = CpModel::new();
    let mut name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut bool_name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut interval_name_to_idx: HashMap<String, i32> = HashMap::new();

    for var in &req.variables {
        let idx = if var.is_bool {
            let i = model.new_bool_var(&var.name);
            bool_name_to_idx.insert(var.name.clone(), i);
            i
        } else {
            model.new_int_var(var.lb, var.ub, &var.name)
        };
        name_to_idx.insert(var.name.clone(), idx);
    }

    for ivd in &req.interval_vars {
        if let (Some(&s), Some(&e)) = (
            name_to_idx.get(&ivd.start_var),
            name_to_idx.get(&ivd.end_var),
        ) {
            let idx = model.new_fixed_interval_var(s, ivd.duration, e, &ivd.name);
            interval_name_to_idx.insert(ivd.name.clone(), idx);
        } else {
            warn!(name = %ivd.name, "interval_var references unknown start/end variable");
        }
    }

    for ovd in &req.optional_interval_vars {
        match (
            name_to_idx.get(&ovd.start_var),
            name_to_idx.get(&ovd.end_var),
            bool_name_to_idx.get(&ovd.lit_var),
        ) {
            (Some(&s), Some(&e), Some(&lit)) => {
                let idx = model.new_optional_interval_var(s, ovd.duration, e, lit, &ovd.name);
                interval_name_to_idx.insert(ovd.name.clone(), idx);
            }
            _ => {
                warn!(name = %ovd.name, "optional_interval_var references unknown variable(s)");
            }
        }
    }

    for constraint in &req.constraints {
        match constraint {
            ConstraintKind::LinearLe { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx);
                model.add_linear_le(&vars, &coeffs, *rhs);
            }
            ConstraintKind::LinearGe { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx);
                model.add_linear_ge(&vars, &coeffs, *rhs);
            }
            ConstraintKind::LinearEq { terms, rhs } => {
                let (vars, coeffs) = terms_to_vecs(terms, &name_to_idx);
                model.add_linear_eq(&vars, &coeffs, *rhs);
            }
            ConstraintKind::AllDifferent { vars } => {
                let idxs: Vec<i32> = vars
                    .iter()
                    .filter_map(|v| name_to_idx.get(v).copied())
                    .collect();
                model.add_all_different(&idxs);
            }
            ConstraintKind::BoolOr { literals } => {
                if let Some(lits) = bool_literals_to_refs(literals, &bool_name_to_idx, "BoolOr") {
                    model.add_bool_or(&lits);
                }
            }
            ConstraintKind::BoolAnd { literals } => {
                if let Some(lits) = bool_literals_to_refs(literals, &bool_name_to_idx, "BoolAnd") {
                    model.add_bool_and(&lits);
                }
            }
            ConstraintKind::BoolXor { literals } => {
                if let Some(lits) = bool_literals_to_refs(literals, &bool_name_to_idx, "BoolXor") {
                    model.add_bool_xor(&lits);
                }
            }
            ConstraintKind::Implication {
                antecedent,
                consequent,
            } => {
                if let (Some(lhs), Some(rhs)) = (
                    bool_literal_to_ref(antecedent, &bool_name_to_idx, "Implication"),
                    bool_literal_to_ref(consequent, &bool_name_to_idx, "Implication"),
                ) {
                    model.add_implication(lhs, rhs);
                } else {
                    warn!("Implication references unknown bool variable");
                }
            }
            ConstraintKind::AtMostOne { literals } => {
                if let Some(lits) = bool_literals_to_refs(literals, &bool_name_to_idx, "AtMostOne")
                {
                    model.add_at_most_one(&lits);
                }
            }
            ConstraintKind::ExactlyOne { literals } => {
                if let Some(lits) = bool_literals_to_refs(literals, &bool_name_to_idx, "ExactlyOne")
                {
                    model.add_exactly_one(&lits);
                }
            }
            ConstraintKind::AllowedAssignments { vars, tuples } => {
                if tuples.iter().all(|tuple| tuple.len() == vars.len()) {
                    if let Some(idxs) = vars_to_idxs(vars, &name_to_idx, "AllowedAssignments") {
                        model.add_allowed_assignments(&idxs, tuples);
                    }
                } else {
                    warn!("AllowedAssignments tuple arity does not match vars");
                }
            }
            ConstraintKind::NoOverlap { intervals } => {
                let idxs: Vec<i32> = intervals
                    .iter()
                    .filter_map(|n| interval_name_to_idx.get(n).copied())
                    .collect();
                model.add_no_overlap(&idxs);
            }
            ConstraintKind::Cumulative { demands, capacity } => {
                let mut interval_idxs = Vec::with_capacity(demands.len());
                let mut demand_values = Vec::with_capacity(demands.len());
                let mut missing = false;
                for demand in demands {
                    if let Some(&idx) = interval_name_to_idx.get(&demand.interval) {
                        interval_idxs.push(idx);
                        demand_values.push(demand.demand);
                    } else {
                        missing = true;
                        warn!(
                            interval = %demand.interval,
                            "Cumulative references unknown interval"
                        );
                    }
                }
                if !missing {
                    model.add_cumulative(&interval_idxs, &demand_values, *capacity);
                }
            }
            ConstraintKind::NoOverlap2D { rectangles } => {
                let mut x_intervals = Vec::with_capacity(rectangles.len());
                let mut y_intervals = Vec::with_capacity(rectangles.len());
                let mut missing = false;
                for rectangle in rectangles {
                    if let (Some(&x), Some(&y)) = (
                        interval_name_to_idx.get(&rectangle.x_interval),
                        interval_name_to_idx.get(&rectangle.y_interval),
                    ) {
                        x_intervals.push(x);
                        y_intervals.push(y);
                    } else {
                        missing = true;
                        warn!("NoOverlap2D references unknown interval");
                    }
                }
                if !missing {
                    model.add_no_overlap_2d(&x_intervals, &y_intervals);
                }
            }
        }
    }

    if let Some(obj_terms) = &req.objective_terms {
        let (vars, coeffs) = terms_to_vecs(obj_terms, &name_to_idx);
        if req.minimize {
            model.minimize(&vars, &coeffs);
        } else {
            model.maximize(&vars, &coeffs);
        }
    }

    let time_limit = req.time_limit_seconds.unwrap_or(60.0);
    let solution = model.solve(time_limit);

    let status = match solution.status() {
        OrtoolsStatus::Optimal => "optimal",
        OrtoolsStatus::Feasible => "feasible",
        OrtoolsStatus::Infeasible => "infeasible",
        OrtoolsStatus::Unbounded => "unbounded",
        _ => "error",
    };

    let assignments = if solution.status().is_success() {
        req.variables
            .iter()
            .filter_map(|v| {
                name_to_idx
                    .get(&v.name)
                    .map(|&idx| (v.name.clone(), solution.value(idx)))
            })
            .collect()
    } else {
        vec![]
    };

    let objective_value = if solution.status().is_success() && req.objective_terms.is_some() {
        Some(solution.objective_value())
    } else {
        None
    };

    CpSatPlan {
        request_id: req.id.clone(),
        status: status.to_string(),
        assignments,
        objective_value,
        wall_time_seconds: solution.wall_time(),
        solver: "cp-sat-v9.15".to_string(),
    }
}

fn terms_to_vecs(terms: &[CpTerm], name_to_idx: &HashMap<String, i32>) -> (Vec<i32>, Vec<i64>) {
    terms
        .iter()
        .filter_map(|t| name_to_idx.get(&t.var).map(|&idx| (idx, t.coeff)))
        .unzip()
}

fn vars_to_idxs(
    vars: &[String],
    name_to_idx: &HashMap<String, i32>,
    constraint: &'static str,
) -> Option<Vec<i32>> {
    let mut idxs = Vec::with_capacity(vars.len());
    for var in vars {
        let Some(&idx) = name_to_idx.get(var) else {
            warn!(constraint, var = %var, "constraint references unknown variable");
            return None;
        };
        idxs.push(idx);
    }
    Some(idxs)
}

fn bool_literals_to_refs(
    literals: &[CpBoolLiteral],
    bool_name_to_idx: &HashMap<String, i32>,
    constraint: &'static str,
) -> Option<Vec<i32>> {
    let mut refs = Vec::with_capacity(literals.len());
    for literal in literals {
        let lit_ref = bool_literal_to_ref(literal, bool_name_to_idx, constraint)?;
        refs.push(lit_ref);
    }
    Some(refs)
}

fn bool_literal_to_ref(
    literal: &CpBoolLiteral,
    bool_name_to_idx: &HashMap<String, i32>,
    constraint: &'static str,
) -> Option<i32> {
    let Some(&idx) = bool_name_to_idx.get(&literal.var) else {
        warn!(
            constraint,
            var = %literal.var,
            "constraint references unknown bool variable"
        );
        return None;
    };
    Some(if literal.negated { -idx - 1 } else { idx })
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
        CpBoolLiteral, CpTerm, CpVariable, CumulativeDemand, IntervalVarDef, NoOverlap2DRectangle,
        OptionalIntervalVarDef,
    };
    use crate::test_support::MockContext;
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
        assert!(matches!(plan.status.as_str(), "optimal" | "feasible"));
        assert_eq!(plan.assignments.len(), 4);
        assert_eq!(plan.solver, "cp-sat-v9.15");
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
        assert_eq!(plan.status, "optimal");
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
        assert_eq!(plan.status, "infeasible");
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
        assert!(matches!(plan.status.as_str(), "optimal" | "feasible"));
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
        assert_eq!(plan.status, "optimal");
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
        assert_eq!(plan.status, "infeasible");
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
            let feasible = matches!(plan.status.as_str(), "optimal" | "feasible");
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
        assert_eq!(plan.status, "optimal");
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
        assert_eq!(plan.status, "infeasible");
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
        assert_eq!(plan.status, "optimal");
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
        assert_eq!(plan.status, "optimal");
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
        assert_eq!(plan.status, "optimal");
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
        assert!(matches!(plan.status.as_str(), "optimal" | "feasible"));
    }

    #[test]
    fn ignores_unknown_var_references() {
        // interval_var references unknown start/end: warning, no panic.
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
        // Solver still runs on the orphan var; no panic.
        assert!(matches!(
            plan.status.as_str(),
            "optimal" | "feasible" | "error"
        ));
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
        assert!(matches!(plan.status.as_str(), "optimal" | "feasible"));
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
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty().with_seed("cpsat-request:s1", &body);
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
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty()
            .with_seed("cpsat-request:s2", &body)
            .with_strategy("cpsat-plan:s2", "{}");
        let s = CpSatSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed("cpsat-request:bad", "not json");
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
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible solution, got {} in {elapsed:.1}s",
            plan.status
        );
        assert_eq!(plan.assignments.len(), n);
        assert!(plan.objective_value.unwrap() > 0);
    }
}
