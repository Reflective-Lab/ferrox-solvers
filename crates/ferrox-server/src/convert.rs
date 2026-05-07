#![allow(clippy::result_large_err)]

use ferrox::cp::problem::{ConstraintKind, CpSatPlan, CpSatRequest, CpTerm, CpVariable};
use ferrox::lp::problem::{LpConstraint, LpObjective, LpPlan, LpRequest, LpTerm, LpVariable};
use ferrox::mip::problem::{
    MipConstraint, MipObjective, MipPlan, MipRequest, MipTerm, MipVariable, VarKind,
};
use tonic::Status;

use crate::proto::ferrox::v1 as p;

// ─── CP-SAT ──────────────────────────────────────────────────────────────────

pub fn cp_req_from_proto(r: p::SolveCpRequest) -> Result<CpSatRequest, Status> {
    let variables = r
        .variables
        .into_iter()
        .map(|v| CpVariable {
            name: v.name,
            lb: v.lb,
            ub: v.ub,
            is_bool: false,
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
        interval_vars: vec![],
        optional_interval_vars: vec![],
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
    }
}

fn cp_term_from_proto(t: p::CpTerm) -> CpTerm {
    CpTerm {
        var: t.var,
        coeff: t.coeff,
    }
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
    }
}

// ─── MIP ─────────────────────────────────────────────────────────────────────

pub fn mip_req_from_proto(r: p::SolveMipRequest) -> Result<MipRequest, Status> {
    let objective = r
        .objective
        .ok_or_else(|| Status::invalid_argument("missing MipObjective"))?;

    Ok(MipRequest {
        id: r.id,
        variables: r
            .variables
            .into_iter()
            .map(|v| MipVariable {
                name: v.name,
                lb: v.lb,
                ub: v.ub,
                kind: match v.kind {
                    1 => VarKind::Integer,
                    2 => VarKind::Binary,
                    _ => VarKind::Continuous,
                },
            })
            .collect(),
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
    fn cp_request_with_each_constraint_kind() {
        let req = p::SolveCpRequest {
            id: "r".into(),
            variables: vec![
                p::CpVariable {
                    name: "a".into(),
                    lb: 0,
                    ub: 5,
                },
                p::CpVariable {
                    name: "b".into(),
                    lb: 0,
                    ub: 5,
                },
            ],
            constraints: vec![
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearLe(p::CpLinear {
                        terms: vec![cp_term_proto("a", 1)],
                        rhs: 5,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearGe(p::CpLinear {
                        terms: vec![cp_term_proto("a", 1)],
                        rhs: 0,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::LinearEq(p::CpLinear {
                        terms: vec![cp_term_proto("a", 1), cp_term_proto("b", 1)],
                        rhs: 5,
                    })),
                },
                p::CpConstraint {
                    kind: Some(p::cp_constraint::Kind::AllDifferent(p::CpAllDifferent {
                        vars: vec!["a".into(), "b".into()],
                    })),
                },
            ],
            objective_terms: vec![cp_term_proto("a", 1), cp_term_proto("b", 1)],
            minimize: true,
            time_limit_seconds: Some(1.0),
        };
        let out = cp_req_from_proto(req).expect("conversion");
        assert_eq!(out.id, "r");
        assert_eq!(out.variables.len(), 2);
        assert_eq!(out.constraints.len(), 4);
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
        };
        let err = cp_req_from_proto(req).expect_err("must reject missing kind");
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
        };
        let resp = lp_resp_to_proto(plan);
        assert_eq!(resp.values.len(), 1);
        assert_eq!(resp.values[0].name, "x");
        assert_eq!(resp.values[0].value, 1.5);
        assert_eq!(resp.objective_value, 3.0);
        assert_eq!(resp.solver, "glop");
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
                p::MipVariable {
                    name: "u".into(),
                    lb: 0.0,
                    ub: 1.0,
                    kind: 99, // unknown → continuous fallback
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
        assert!(matches!(out.variables[3].kind, VarKind::Continuous));
        assert!(!out.objective.maximize);
        assert_eq!(out.mip_gap_tolerance, Some(0.01));
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
        };
        let resp = mip_resp_to_proto(plan);
        assert_eq!(resp.values.len(), 2);
        assert_eq!(resp.objective_value, 7.0);
        assert_eq!(resp.mip_gap, 0.05);
        assert_eq!(resp.solver, "highs");
    }
}
