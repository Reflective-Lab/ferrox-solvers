#![allow(clippy::result_large_err)]

use ferrox::cp::problem::{
    ConstraintKind, CpBoolLiteral, CpSatPlan, CpSatRequest, CpTerm, CpVariable, CumulativeDemand,
    IntervalVarDef, NoOverlap2DRectangle, OptionalIntervalVarDef,
};
use ferrox::lp::problem::{LpConstraint, LpObjective, LpPlan, LpRequest, LpTerm, LpVariable};
use ferrox::mip::problem::{
    MipConstraint, MipObjective, MipPlan, MipRequest, MipTerm, MipVariable, VarKind,
};
use tonic::Status;

use crate::proto::ferrox::v1 as p;

fn solver_identity_to_proto(identity: ferrox::ExecutionIdentity) -> p::SolverIdentity {
    let native_identity = identity.native_identity.as_ref();
    let native_backend =
        native_identity.map_or_else(|| "none".to_string(), |native| native.backend.clone());
    let native_version = native_identity.map_or_else(
        || identity.backend_version.clone(),
        |native| native.version.clone(),
    );
    let native_source_url = native_identity.map_or_else(
        || "not_applicable".to_string(),
        |native| native.source_url.clone(),
    );
    let native_expected_commit = native_identity.map_or_else(
        || "not_applicable".to_string(),
        |native| native.expected_commit.clone(),
    );
    let native_actual_commit = native_identity.map_or_else(
        || "not_applicable".to_string(),
        |native| native.actual_commit.clone(),
    );
    let native_source_mode = native_identity.map_or_else(
        || "not_applicable".to_string(),
        |native| native.source_mode.clone(),
    );

    p::SolverIdentity {
        solver: identity.backend,
        native_backend,
        native_version,
        native_source_url,
        native_expected_commit,
        native_actual_commit,
        native_source_mode,
        build_config: identity.build_identity,
        runtime_config: identity.runtime_config,
        rust_crate: identity.producer.name,
        rust_crate_version: identity.producer.version,
    }
}

// ─── CP-SAT ──────────────────────────────────────────────────────────────────

pub fn cp_req_from_proto(r: p::SolveCpRequest) -> Result<CpSatRequest, Status> {
    let variables = r
        .variables
        .into_iter()
        .map(|v| CpVariable {
            name: v.name,
            lb: v.lb,
            ub: v.ub,
            is_bool: v.is_bool,
        })
        .collect();

    let constraints = r
        .constraints
        .into_iter()
        .map(cp_constraint_from_proto)
        .collect::<Result<Vec<_>, _>>()?;

    let objective_terms = if r.objective_terms.is_empty() {
        None
    } else {
        Some(
            r.objective_terms
                .into_iter()
                .map(cp_term_from_proto)
                .collect(),
        )
    };

    Ok(CpSatRequest {
        id: r.id,
        variables,
        interval_vars: r
            .interval_vars
            .into_iter()
            .map(|v| IntervalVarDef {
                name: v.name,
                start_var: v.start_var,
                duration: v.duration,
                end_var: v.end_var,
            })
            .collect(),
        optional_interval_vars: r
            .optional_interval_vars
            .into_iter()
            .map(|v| OptionalIntervalVarDef {
                name: v.name,
                start_var: v.start_var,
                duration: v.duration,
                end_var: v.end_var,
                lit_var: v.lit_var,
            })
            .collect(),
        constraints,
        objective_terms,
        minimize: r.minimize,
        time_limit_seconds: r.time_limit_seconds,
    })
}

fn cp_constraint_from_proto(c: p::CpConstraint) -> Result<ConstraintKind, Status> {
    use p::cp_constraint::Kind;
    match c
        .kind
        .ok_or_else(|| Status::invalid_argument("missing CpConstraint.kind"))?
    {
        Kind::LinearLe(l) => Ok(ConstraintKind::LinearLe {
            terms: l.terms.into_iter().map(cp_term_from_proto).collect(),
            rhs: l.rhs,
        }),
        Kind::LinearGe(l) => Ok(ConstraintKind::LinearGe {
            terms: l.terms.into_iter().map(cp_term_from_proto).collect(),
            rhs: l.rhs,
        }),
        Kind::LinearEq(l) => Ok(ConstraintKind::LinearEq {
            terms: l.terms.into_iter().map(cp_term_from_proto).collect(),
            rhs: l.rhs,
        }),
        Kind::AllDifferent(a) => Ok(ConstraintKind::AllDifferent { vars: a.vars }),
        Kind::BoolOr(b) => Ok(ConstraintKind::BoolOr {
            literals: cp_bool_literals_from_proto(b),
        }),
        Kind::BoolAnd(b) => Ok(ConstraintKind::BoolAnd {
            literals: cp_bool_literals_from_proto(b),
        }),
        Kind::BoolXor(b) => Ok(ConstraintKind::BoolXor {
            literals: cp_bool_literals_from_proto(b),
        }),
        Kind::Implication(i) => Ok(ConstraintKind::Implication {
            antecedent: cp_required_bool_literal(i.antecedent, "CpImplication.antecedent")?,
            consequent: cp_required_bool_literal(i.consequent, "CpImplication.consequent")?,
        }),
        Kind::AtMostOne(b) => Ok(ConstraintKind::AtMostOne {
            literals: cp_bool_literals_from_proto(b),
        }),
        Kind::ExactlyOne(b) => Ok(ConstraintKind::ExactlyOne {
            literals: cp_bool_literals_from_proto(b),
        }),
        Kind::AllowedAssignments(a) => Ok(ConstraintKind::AllowedAssignments {
            vars: a.vars,
            tuples: a.tuples.into_iter().map(|t| t.values).collect(),
        }),
        Kind::NoOverlap(n) => Ok(ConstraintKind::NoOverlap {
            intervals: n.intervals,
        }),
        Kind::Cumulative(c) => Ok(ConstraintKind::Cumulative {
            demands: c
                .demands
                .into_iter()
                .map(|d| CumulativeDemand {
                    interval: d.interval,
                    demand: d.demand,
                })
                .collect(),
            capacity: c.capacity,
        }),
        Kind::NoOverlap2d(n) => Ok(ConstraintKind::NoOverlap2D {
            rectangles: n
                .rectangles
                .into_iter()
                .map(|r| NoOverlap2DRectangle {
                    x_interval: r.x_interval,
                    y_interval: r.y_interval,
                })
                .collect(),
        }),
    }
}

fn cp_term_from_proto(t: p::CpTerm) -> CpTerm {
    CpTerm {
        var: t.var,
        coeff: t.coeff,
    }
}

fn cp_bool_literals_from_proto(literals: p::CpBoolLiterals) -> Vec<CpBoolLiteral> {
    literals
        .literals
        .into_iter()
        .map(cp_bool_literal_from_proto)
        .collect()
}

fn cp_bool_literal_from_proto(l: p::CpBoolLiteral) -> CpBoolLiteral {
    CpBoolLiteral {
        var: l.var,
        negated: l.negated,
    }
}

fn cp_required_bool_literal(
    literal: Option<p::CpBoolLiteral>,
    field: &'static str,
) -> Result<CpBoolLiteral, Status> {
    literal
        .map(cp_bool_literal_from_proto)
        .ok_or_else(|| Status::invalid_argument(format!("missing {field}")))
}

pub fn cp_resp_to_proto(p: CpSatPlan) -> p::SolveCpResponse {
    p::SolveCpResponse {
        request_id: p.request_id,
        status: p.status,
        assignments: p
            .assignments
            .into_iter()
            .map(|(name, value)| p::StringI64 { name, value })
            .collect(),
        objective_value: p.objective_value,
        wall_time_seconds: p.wall_time_seconds,
        solver: p.solver,
        solver_identity: Some(solver_identity_to_proto(p.execution_identity)),
    }
}

// ─── LP ──────────────────────────────────────────────────────────────────────

pub fn lp_req_from_proto(r: p::SolveLpRequest) -> Result<LpRequest, Status> {
    let objective = r
        .objective
        .ok_or_else(|| Status::invalid_argument("missing LpObjective"))?;

    Ok(LpRequest {
        id: r.id,
        variables: r
            .variables
            .into_iter()
            .map(|v| LpVariable {
                name: v.name,
                lb: v.lb,
                ub: v.ub,
            })
            .collect(),
        constraints: r
            .constraints
            .into_iter()
            .map(|c| LpConstraint {
                name: c.name,
                lb: c.lb,
                ub: c.ub,
                terms: c
                    .terms
                    .into_iter()
                    .map(|t| LpTerm {
                        var: t.var,
                        coeff: t.coeff,
                    })
                    .collect(),
            })
            .collect(),
        objective: LpObjective {
            terms: objective
                .terms
                .into_iter()
                .map(|t| LpTerm {
                    var: t.var,
                    coeff: t.coeff,
                })
                .collect(),
            maximize: objective.maximize,
        },
        time_limit_seconds: r.time_limit_seconds,
    })
}

pub fn lp_resp_to_proto(p: LpPlan) -> p::SolveLpResponse {
    p::SolveLpResponse {
        request_id: p.request_id,
        status: p.status,
        values: p
            .values
            .into_iter()
            .map(|(name, value)| p::StringF64 { name, value })
            .collect(),
        objective_value: p.objective_value,
        solver: p.solver,
        solver_identity: Some(solver_identity_to_proto(p.execution_identity)),
    }
}

// ─── MIP ─────────────────────────────────────────────────────────────────────

pub fn mip_req_from_proto(r: p::SolveMipRequest) -> Result<MipRequest, Status> {
    let objective = r
        .objective
        .ok_or_else(|| Status::invalid_argument("missing MipObjective"))?;

    let variables = r
        .variables
        .into_iter()
        .map(|v| {
            let kind = match v.kind {
                0 => VarKind::Continuous,
                1 => VarKind::Integer,
                2 => VarKind::Binary,
                other => {
                    return Err(Status::invalid_argument(format!(
                        "unknown MipVariable.kind {other} for '{}'",
                        v.name
                    )));
                }
            };

            Ok(MipVariable {
                name: v.name,
                lb: v.lb,
                ub: v.ub,
                kind,
            })
        })
        .collect::<Result<Vec<_>, Status>>()?;

    Ok(MipRequest {
        id: r.id,
        variables,
        constraints: r
            .constraints
            .into_iter()
            .map(|c| MipConstraint {
                name: c.name,
                lb: c.lb,
                ub: c.ub,
                terms: c
                    .terms
                    .into_iter()
                    .map(|t| MipTerm {
                        var: t.var,
                        coeff: t.coeff,
                    })
                    .collect(),
            })
            .collect(),
        objective: MipObjective {
            terms: objective
                .terms
                .into_iter()
                .map(|t| MipTerm {
                    var: t.var,
                    coeff: t.coeff,
                })
                .collect(),
            maximize: objective.maximize,
        },
        time_limit_seconds: r.time_limit_seconds,
        mip_gap_tolerance: r.mip_gap_tolerance,
    })
}

pub fn mip_resp_to_proto(p: MipPlan) -> p::SolveMipResponse {
    p::SolveMipResponse {
        request_id: p.request_id,
        status: p.status,
        values: p
            .values
            .into_iter()
            .map(|(name, value)| p::StringF64 { name, value })
            .collect(),
        objective_value: p.objective_value,
        mip_gap: p.mip_gap,
        solver: p.solver,
        solver_identity: Some(solver_identity_to_proto(p.execution_identity)),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::proto::ferrox::v1 as p;

    fn cp_term_proto(var: &str, coeff: i64) -> p::CpTerm {
        p::CpTerm {
            var: var.into(),
            coeff,
        }
    }

    fn cp_var_proto(name: &str, lb: i64, ub: i64, is_bool: bool) -> p::CpVariable {
        p::CpVariable {
            name: name.into(),
            lb,
            ub,
            is_bool,
        }
    }

    fn cp_lit_proto(var: &str, negated: bool) -> p::CpBoolLiteral {
        p::CpBoolLiteral {
            var: var.into(),
            negated,
        }
    }

    fn cp_lits_proto(vars: &[&str]) -> p::CpBoolLiterals {
        p::CpBoolLiterals {
            literals: vars.iter().map(|var| cp_lit_proto(var, false)).collect(),
        }
    }

    fn lp_term(var: &str, coeff: f64) -> p::LpTerm {
        p::LpTerm {
            var: var.into(),
            coeff,
        }
    }

    fn mip_term(var: &str, coeff: f64) -> p::MipTerm {
        p::MipTerm {
            var: var.into(),
            coeff,
        }
    }

    // ─── CP-SAT ──────────────────────────────────────────────────────────────

    #[test]
    #[allow(clippy::too_many_lines)]
    fn cp_request_with_each_constraint_kind() {
        let req = p::SolveCpRequest {
            id: "r".into(),
            variables: vec![
                cp_var_proto("a", 0, 1, true),
                cp_var_proto("b", 0, 1, true),
                cp_var_proto("x", 0, 5, false),
                cp_var_proto("y", 0, 5, false),
            ],
            constraints: vec![
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearLe(p::CpLinear {
                        terms: vec![cp_term_proto("x", 1)],
                        rhs: 5,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearGe(p::CpLinear {
                        terms: vec![cp_term_proto("x", 1)],
                        rhs: 0,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearEq(p::CpLinear {
                        terms: vec![cp_term_proto("x", 1), cp_term_proto("y", 1)],
                        rhs: 5,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::AllDifferent(p::CpAllDifferent {
                        vars: vec!["x".into(), "y".into()],
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::BoolOr(cp_lits_proto(&["a", "b"]))),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::BoolAnd(cp_lits_proto(&["a"]))),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::BoolXor(cp_lits_proto(&["a", "b"]))),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::Implication(p::CpImplication {
                        antecedent: Some(cp_lit_proto("a", false)),
                        consequent: Some(cp_lit_proto("b", true)),
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::AtMostOne(cp_lits_proto(&[
                        "a", "b",
                    ]))),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::ExactlyOne(cp_lits_proto(&[
                        "a", "b",
                    ]))),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::AllowedAssignments(
                        p::CpAllowedAssignments {
                            vars: vec!["x".into(), "y".into()],
                            tuples: vec![p::CpTuple { values: vec![0, 1] }],
                        },
                    )),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::NoOverlap(p::CpNoOverlap {
                        intervals: vec!["iv1".into(), "iv2".into()],
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::Cumulative(p::CpCumulative {
                        demands: vec![p::CpCumulativeDemand {
                            interval: "iv1".into(),
                            demand: 2,
                        }],
                        capacity: 3,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::NoOverlap2d(p::CpNoOverlap2D {
                        rectangles: vec![p::CpNoOverlap2DRectangle {
                            x_interval: "iv1".into(),
                            y_interval: "iv2".into(),
                        }],
                    })),
                },
            ],
            objective_terms: vec![cp_term_proto("x", 1), cp_term_proto("y", 1)],
            minimize: true,
            time_limit_seconds: Some(1.0),
            interval_vars: vec![
                p::CpIntervalVar {
                    name: "iv1".into(),
                    start_var: "x".into(),
                    duration: 1,
                    end_var: "y".into(),
                },
                p::CpIntervalVar {
                    name: "iv2".into(),
                    start_var: "x".into(),
                    duration: 1,
                    end_var: "y".into(),
                },
            ],
            optional_interval_vars: vec![p::CpOptionalIntervalVar {
                name: "oiv".into(),
                start_var: "x".into(),
                duration: 1,
                end_var: "y".into(),
                lit_var: "a".into(),
            }],
        };
        let out = cp_req_from_proto(req).expect("conversion");
        assert_eq!(out.id, "r");
        assert_eq!(out.variables.len(), 4);
        assert!(out.variables[0].is_bool);
        assert_eq!(out.interval_vars.len(), 2);
        assert_eq!(out.optional_interval_vars.len(), 1);
        assert_eq!(out.constraints.len(), 14);
        assert!(matches!(
            out.constraints[0],
            ferrox::cp::problem::ConstraintKind::LinearLe { rhs: 5, .. }
        ));
        assert!(matches!(
            out.constraints[1],
            ferrox::cp::problem::ConstraintKind::LinearGe { .. }
        ));
        assert!(matches!(
            out.constraints[2],
            ferrox::cp::problem::ConstraintKind::LinearEq { .. }
        ));
        assert!(matches!(
            out.constraints[3],
            ferrox::cp::problem::ConstraintKind::AllDifferent { .. }
        ));
        assert!(matches!(
            out.constraints[4],
            ferrox::cp::problem::ConstraintKind::BoolOr { .. }
        ));
        assert!(matches!(
            out.constraints[5],
            ferrox::cp::problem::ConstraintKind::BoolAnd { .. }
        ));
        assert!(matches!(
            out.constraints[6],
            ferrox::cp::problem::ConstraintKind::BoolXor { .. }
        ));
        assert!(matches!(
            out.constraints[7],
            ferrox::cp::problem::ConstraintKind::Implication { .. }
        ));
        assert!(matches!(
            out.constraints[8],
            ferrox::cp::problem::ConstraintKind::AtMostOne { .. }
        ));
        assert!(matches!(
            out.constraints[9],
            ferrox::cp::problem::ConstraintKind::ExactlyOne { .. }
        ));
        assert!(matches!(
            out.constraints[10],
            ferrox::cp::problem::ConstraintKind::AllowedAssignments { .. }
        ));
        assert!(matches!(
            out.constraints[11],
            ferrox::cp::problem::ConstraintKind::NoOverlap { .. }
        ));
        assert!(matches!(
            out.constraints[12],
            ferrox::cp::problem::ConstraintKind::Cumulative { .. }
        ));
        assert!(matches!(
            out.constraints[13],
            ferrox::cp::problem::ConstraintKind::NoOverlap2D { .. }
        ));
        assert!(out.minimize);
        assert!(out.objective_terms.is_some());
        assert_eq!(out.time_limit_seconds, Some(1.0));
    }

    #[test]
    fn cp_request_empty_objective_yields_none() {
        let req = p::SolveCpRequest {
            id: "r".into(),
            variables: vec![],
            constraints: vec![],
            objective_terms: vec![],
            minimize: false,
            time_limit_seconds: None,
            interval_vars: vec![],
            optional_interval_vars: vec![],
        };
        let out = cp_req_from_proto(req).expect("conversion");
        assert!(out.objective_terms.is_none());
        assert!(!out.minimize);
    }

    #[test]
    fn cp_constraint_missing_kind_returns_invalid_argument() {
        let req = p::SolveCpRequest {
            id: "r".into(),
            variables: vec![],
            constraints: vec![p::CpConstraint { kind: None }],
            objective_terms: vec![],
            minimize: false,
            time_limit_seconds: None,
            interval_vars: vec![],
            optional_interval_vars: vec![],
        };
        let err = cp_req_from_proto(req).expect_err("must reject missing kind");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn cp_implication_missing_literal_returns_invalid_argument() {
        let req = p::SolveCpRequest {
            id: "r".into(),
            variables: vec![],
            constraints: vec![p::CpConstraint {
                kind: Some(p::cp_constraint::Kind::Implication(p::CpImplication {
                    antecedent: None,
                    consequent: Some(cp_lit_proto("b", false)),
                })),
            }],
            objective_terms: vec![],
            minimize: false,
            time_limit_seconds: None,
            interval_vars: vec![],
            optional_interval_vars: vec![],
        };
        let err = cp_req_from_proto(req).expect_err("must reject missing implication literal");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn cp_response_round_trip() {
        let plan = ferrox::cp::problem::CpSatPlan {
            request_id: "r".into(),
            status: "optimal".into(),
            assignments: vec![("x".into(), 3), ("y".into(), 7)],
            objective_value: Some(10),
            wall_time_seconds: 0.5,
            solver: "cp-sat-v9.15".into(),
            execution_identity: ferrox::solver_identity::non_native_solver_identity(
                "cp-sat-v9.15",
                "test",
            ),
        };
        let resp = cp_resp_to_proto(plan);
        assert_eq!(resp.request_id, "r");
        assert_eq!(resp.status, "optimal");
        assert_eq!(resp.assignments.len(), 2);
        assert_eq!(resp.assignments[0].name, "x");
        assert_eq!(resp.assignments[0].value, 3);
        assert_eq!(resp.objective_value, Some(10));
        assert_eq!(resp.wall_time_seconds, 0.5);
        assert_eq!(resp.solver, "cp-sat-v9.15");
        assert_eq!(resp.solver_identity.unwrap().solver, "cp-sat-v9.15");
    }

    // ─── LP ──────────────────────────────────────────────────────────────────

    #[test]
    fn lp_request_round_trip() {
        let req = p::SolveLpRequest {
            id: "lp".into(),
            variables: vec![p::LpVariable {
                name: "x".into(),
                lb: 0.0,
                ub: 10.0,
            }],
            constraints: vec![p::LpConstraint {
                name: "c".into(),
                lb: 0.0,
                ub: 5.0,
                terms: vec![lp_term("x", 1.0)],
            }],
            objective: Some(p::LpObjective {
                terms: vec![lp_term("x", 2.0)],
                maximize: true,
            }),
            time_limit_seconds: Some(2.0),
        };
        let out = lp_req_from_proto(req).expect("conversion");
        assert_eq!(out.id, "lp");
        assert_eq!(out.variables.len(), 1);
        assert_eq!(out.variables[0].lb, 0.0);
        assert_eq!(out.variables[0].ub, 10.0);
        assert_eq!(out.constraints.len(), 1);
        assert_eq!(out.constraints[0].terms.len(), 1);
        assert_eq!(out.objective.terms[0].coeff, 2.0);
        assert!(out.objective.maximize);
        assert_eq!(out.time_limit_seconds, Some(2.0));
    }

    #[test]
    fn lp_request_missing_objective_returns_invalid_argument() {
        let req = p::SolveLpRequest {
            id: "lp".into(),
            variables: vec![],
            constraints: vec![],
            objective: None,
            time_limit_seconds: None,
        };
        let err = lp_req_from_proto(req).expect_err("must reject missing objective");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn lp_response_round_trip() {
        let plan = ferrox::lp::problem::LpPlan {
            request_id: "r".into(),
            status: "optimal".into(),
            values: vec![("x".into(), 1.5)],
            objective_value: 3.0,
            solver: "glop".into(),
            execution_identity: ferrox::solver_identity::non_native_solver_identity("glop", "test"),
        };
        let resp = lp_resp_to_proto(plan);
        assert_eq!(resp.values.len(), 1);
        assert_eq!(resp.values[0].name, "x");
        assert_eq!(resp.values[0].value, 1.5);
        assert_eq!(resp.objective_value, 3.0);
        assert_eq!(resp.solver, "glop");
        assert_eq!(resp.solver_identity.unwrap().solver, "glop");
    }

    // ─── MIP ─────────────────────────────────────────────────────────────────

    #[test]
    fn mip_request_each_var_kind() {
        use ferrox::mip::problem::VarKind;
        let req = p::SolveMipRequest {
            id: "mip".into(),
            variables: vec![
                p::MipVariable {
                    name: "c".into(),
                    lb: 0.0,
                    ub: 1.0,
                    kind: 0,
                },
                p::MipVariable {
                    name: "i".into(),
                    lb: 0.0,
                    ub: 5.0,
                    kind: 1,
                },
                p::MipVariable {
                    name: "b".into(),
                    lb: 0.0,
                    ub: 1.0,
                    kind: 2,
                },
            ],
            constraints: vec![p::MipConstraint {
                name: "c1".into(),
                lb: f64::NEG_INFINITY,
                ub: 5.0,
                terms: vec![mip_term("i", 1.0)],
            }],
            objective: Some(p::MipObjective {
                terms: vec![mip_term("c", 1.0), mip_term("b", 2.0)],
                maximize: false,
            }),
            time_limit_seconds: Some(1.0),
            mip_gap_tolerance: Some(0.01),
        };
        let out = mip_req_from_proto(req).expect("conversion");
        assert!(matches!(out.variables[0].kind, VarKind::Continuous));
        assert!(matches!(out.variables[1].kind, VarKind::Integer));
        assert!(matches!(out.variables[2].kind, VarKind::Binary));
        assert!(!out.objective.maximize);
        assert_eq!(out.mip_gap_tolerance, Some(0.01));
    }

    #[test]
    fn mip_request_unknown_var_kind_rejected() {
        let req = p::SolveMipRequest {
            id: "mip".into(),
            variables: vec![p::MipVariable {
                name: "x".into(),
                lb: 0.0,
                ub: 1.0,
                kind: 99,
            }],
            constraints: vec![],
            objective: Some(p::MipObjective {
                terms: vec![mip_term("x", 1.0)],
                maximize: false,
            }),
            time_limit_seconds: None,
            mip_gap_tolerance: None,
        };

        let err = mip_req_from_proto(req).expect_err("unknown kind must fail closed");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn mip_request_missing_objective_rejected() {
        let req = p::SolveMipRequest {
            id: "mip".into(),
            variables: vec![],
            constraints: vec![],
            objective: None,
            time_limit_seconds: None,
            mip_gap_tolerance: None,
        };
        let err = mip_req_from_proto(req).expect_err("must reject missing objective");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn mip_response_round_trip() {
        let plan = ferrox::mip::problem::MipPlan {
            request_id: "r".into(),
            status: "feasible".into(),
            values: vec![("x".into(), 2.0), ("y".into(), 1.0)],
            objective_value: 7.0,
            mip_gap: 0.05,
            solver: "highs".into(),
            execution_identity: ferrox::solver_identity::non_native_solver_identity(
                "highs", "test",
            ),
        };
        let resp = mip_resp_to_proto(plan);
        assert_eq!(resp.values.len(), 2);
        assert_eq!(resp.objective_value, 7.0);
        assert_eq!(resp.mip_gap, 0.05);
        assert_eq!(resp.solver, "highs");
        assert_eq!(resp.solver_identity.unwrap().solver, "highs");
    }
}
