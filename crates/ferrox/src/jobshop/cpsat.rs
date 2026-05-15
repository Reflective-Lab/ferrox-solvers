use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, ProvenanceSource, Suggestor,
};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::CpModel;
use std::time::Instant;
use tracing::warn;

use crate::provenance::FERROX_PROVENANCE;
use crate::solver_identity::cp_sat_solver_identity;

use super::greedy::REQUEST_PREFIX;
use super::problem::{JobShopPlan, JobShopRequest, ScheduledOp};

const PLAN_PREFIX: &str = "jspbench-plan-cpsat:";

/// Solves Job Shop Scheduling to optimality using CP-SAT interval variables.
///
/// **Model:**
/// - One fixed-duration interval per operation (start + duration = end enforced).
/// - `AddNoOverlap` per machine: no two operations on the same machine overlap.
/// - `LinearGe` precedence within each job: op k+1 starts after op k ends.
/// - Makespan variable `C_max`: minimised; bounded by the latest operation end.
///
/// **Confidence:**
/// - `optimal` → 1.0 (proven minimum makespan)
/// - `feasible` → 0.85 (time budget exhausted, solution may improve)
///
/// **Formation role:** CP-SAT is the quality anchor; [`GreedyJobShopSuggestor`]
/// provides an instant warm-start that CP-SAT verifies or beats.
///
/// [`GreedyJobShopSuggestor`]: super::greedy::GreedyJobShopSuggestor
pub struct CpSatJobShopSuggestor;

#[async_trait]
impl Suggestor for CpSatJobShopSuggestor {
    fn name(&self) -> &'static str {
        "CpSatJobShopSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some(concat!(
            "NP-hard; CP-SAT interval-NoOverlap formulation; ",
            "proves optimality for 15×10 instances in < 5 s on 10-core hardware"
        ))
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds).iter().any(|f| {
            f.id().starts_with(REQUEST_PREFIX) && !own_plan_exists(ctx, request_id(f.id()))
        })
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
            if own_plan_exists(ctx, rid) {
                continue;
            }

            match fact.require_payload::<JobShopRequest>() {
                Ok(req) => {
                    let plan = solve_cpsat_jsp(req);
                    let confidence = match plan.status.as_str() {
                        "optimal" => 1.0,
                        "feasible" => 0.85,
                        _ => 0.0,
                    };
                    proposals.push(
                        FERROX_PROVENANCE
                            .proposed_fact(
                                ContextKey::Strategies,
                                format!("{PLAN_PREFIX}{rid}"),
                                plan,
                            )
                            .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "unexpected jspbench-request payload");
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

fn own_plan_exists(ctx: &dyn Context, request_id: &str) -> bool {
    let plan_id = format!("{PLAN_PREFIX}{request_id}");
    ctx.get(ContextKey::Strategies)
        .iter()
        .any(|f| f.id() == plan_id.as_str())
}

// ── Solver ────────────────────────────────────────────────────────────────────

pub fn solve_cpsat_jsp(req: &JobShopRequest) -> JobShopPlan {
    let t0 = Instant::now();
    if let Err(reason) = validate_jobshop_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid jspbench-request");
        return empty_plan(req, "invalid", t0.elapsed().as_secs_f64());
    }

    let horizon = req.horizon();
    let mut model = CpModel::new();

    // start[j][k], end[j][k], interval[j][k]
    let mut starts: Vec<Vec<i32>> = Vec::new();
    let mut ends: Vec<Vec<i32>> = Vec::new();
    let mut intervals: Vec<Vec<i32>> = Vec::new();

    for job in &req.jobs {
        let mut js = Vec::new();
        let mut je = Vec::new();
        let mut ji = Vec::new();
        for (k, op) in job.operations.iter().enumerate() {
            let s = model.new_int_var(0, horizon, &format!("s_{}_{k}", job.id));
            let e = model.new_int_var(op.duration, horizon, &format!("e_{}_{k}", job.id));
            let iv = model.new_fixed_interval_var(s, op.duration, e, &format!("iv_{}_{k}", job.id));
            js.push(s);
            je.push(e);
            ji.push(iv);
        }
        starts.push(js);
        ends.push(je);
        intervals.push(ji);
    }

    // Makespan variable.
    let cmax = model.new_int_var(0, horizon, "cmax");

    // ── Per-job precedence constraints ────────────────────────────────────────
    // end[j][k] <= start[j][k+1]  →  start[j][k+1] - end[j][k] >= 0
    for (j, job) in req.jobs.iter().enumerate() {
        for k in 0..job.operations.len().saturating_sub(1) {
            model.add_linear_ge(&[starts[j][k + 1], ends[j][k]], &[1, -1], 0);
        }
    }

    // ── Per-machine NoOverlap ─────────────────────────────────────────────────
    let mut machine_intervals: Vec<Vec<i32>> = vec![Vec::new(); req.num_machines];
    for (j, job) in req.jobs.iter().enumerate() {
        for (k, op) in job.operations.iter().enumerate() {
            machine_intervals[op.machine_id].push(intervals[j][k]);
        }
    }
    for ivs in &machine_intervals {
        if ivs.len() > 1 {
            model.add_no_overlap(ivs);
        }
    }

    // ── Makespan: cmax >= end[j][last] for each job ───────────────────────────
    for (j, job) in req.jobs.iter().enumerate() {
        let last = job.operations.len() - 1;
        model.add_linear_ge(&[cmax, ends[j][last]], &[1, -1], 0);
    }

    // ── Minimise makespan ─────────────────────────────────────────────────────
    model.minimize(&[cmax], &[1]);

    let solution = model.solve(req.time_limit_seconds);
    let elapsed = t0.elapsed().as_secs_f64();

    let status = match solution.status() {
        OrtoolsStatus::Optimal => "optimal",
        OrtoolsStatus::Feasible => "feasible",
        OrtoolsStatus::Infeasible => "infeasible",
        _ => "error",
    };

    if !solution.status().is_success() {
        return empty_plan(req, status, elapsed);
    }

    let makespan = solution.value(cmax);

    let mut schedule: Vec<ScheduledOp> = Vec::new();
    for (j, job) in req.jobs.iter().enumerate() {
        for (k, op) in job.operations.iter().enumerate() {
            let start = solution.value(starts[j][k]);
            schedule.push(ScheduledOp {
                job_id: job.id,
                job_name: job.name.clone(),
                machine_id: op.machine_id,
                op_index: k,
                start,
                end: start + op.duration,
            });
        }
    }
    schedule.sort_by_key(|s| (s.machine_id, s.start));

    let lower_bound = if status == "optimal" {
        Some(makespan)
    } else {
        None
    };

    JobShopPlan {
        request_id: req.id.clone(),
        schedule,
        makespan,
        lower_bound,
        solver: "cp-sat-v9.15".to_string(),
        execution_identity: jobshop_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds: elapsed,
    }
}

fn validate_jobshop_request(req: &JobShopRequest) -> Result<(), String> {
    if req.id.trim().is_empty() {
        return Err("request id must not be empty".to_string());
    }
    if req.id.contains('\0') {
        return Err("request id contains an interior NUL byte".to_string());
    }
    if !req.time_limit_seconds.is_finite() || req.time_limit_seconds <= 0.0 {
        return Err("time_limit_seconds must be finite and positive".to_string());
    }

    let mut horizon = 0_i64;
    for job in &req.jobs {
        if job.operations.is_empty() {
            return Err(format!("job '{}' has no operations", job.id));
        }
        for op in &job.operations {
            if op.machine_id >= req.num_machines {
                return Err(format!(
                    "operation references machine {} outside 0..{}",
                    op.machine_id, req.num_machines
                ));
            }
            if op.duration < 0 {
                return Err(format!(
                    "operation on machine {} has negative duration",
                    op.machine_id
                ));
            }
            horizon = horizon
                .checked_add(op.duration)
                .ok_or_else(|| "job-shop horizon overflows i64".to_string())?;
        }
    }
    Ok(())
}

fn empty_plan(req: &JobShopRequest, status: &str, wall_time_seconds: f64) -> JobShopPlan {
    JobShopPlan {
        request_id: req.id.clone(),
        schedule: Vec::new(),
        makespan: 0,
        lower_bound: None,
        solver: "cp-sat-v9.15".to_string(),
        execution_identity: jobshop_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds,
    }
}

fn jobshop_cpsat_identity(req: &JobShopRequest) -> ExecutionIdentity {
    cp_sat_solver_identity(format!(
        "time_limit_seconds={}; search_workers=hardware_concurrency; objective=minimize_makespan",
        req.time_limit_seconds
    ))
}

#[cfg(test)]
#[allow(clippy::cast_possible_wrap, clippy::similar_names)]
mod tests {
    use super::*;
    use crate::jobshop::problem::{Job, Operation};
    use crate::test_support::MockContext;
    use converge_pack::TextPayload;

    fn op(machine: usize, dur: i64) -> Operation {
        Operation {
            machine_id: machine,
            duration: dur,
        }
    }

    fn job(id: usize, ops: Vec<Operation>) -> Job {
        Job {
            id,
            name: format!("j{id}"),
            operations: ops,
        }
    }

    fn req(jobs: Vec<Job>, m: usize, time_limit: f64) -> JobShopRequest {
        JobShopRequest {
            id: "j".into(),
            jobs,
            num_machines: m,
            time_limit_seconds: time_limit,
        }
    }

    #[test]
    fn small_instance_optimal() {
        let r = req(
            vec![
                job(0, vec![op(0, 3), op(1, 2)]),
                job(1, vec![op(1, 4), op(0, 1)]),
            ],
            2,
            5.0,
        );
        let plan = solve_cpsat_jsp(&r);
        assert_eq!(plan.status, "optimal");
        assert_eq!(plan.execution_identity.backend, "cp-sat-v9.15");
        assert!(
            plan.execution_identity
                .native_identity
                .as_ref()
                .is_some_and(|native| native.backend.contains("OR-Tools"))
        );
        assert!(plan.lower_bound.is_some());
        // Optimal makespan for this 2x2 instance is 6.
        assert_eq!(plan.makespan, 6);
        assert_eq!(plan.schedule.len(), 4);
    }

    #[test]
    fn rejects_invalid_operation_without_panic() {
        let r = req(vec![job(0, vec![op(2, 3)])], 1, 5.0);
        let plan = solve_cpsat_jsp(&r);
        assert_eq!(plan.status, "invalid");
        assert!(plan.schedule.is_empty());
    }

    #[test]
    fn cpsat_beats_or_matches_greedy_on_small_instance() {
        let jobs = vec![
            job(0, vec![op(0, 3), op(1, 2), op(2, 4)]),
            job(1, vec![op(1, 5), op(0, 3), op(2, 1)]),
            job(2, vec![op(2, 2), op(1, 4), op(0, 3)]),
        ];
        let r = req(jobs, 3, 10.0);
        let opt = solve_cpsat_jsp(&r);
        let g = crate::jobshop::greedy::solve_greedy(&r);
        assert!(opt.makespan <= g.makespan);
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let r = req(vec![job(0, vec![op(0, 3)])], 1, 1.0);
        let ctx = MockContext::empty().with_seed("jspbench-request:j", r);
        let s = CpSatJobShopSuggestor;
        assert_eq!(s.name(), "CpSatJobShopSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let r = req(vec![job(0, vec![op(0, 3)])], 1, 1.0);
        let ctx = MockContext::empty()
            .with_seed("jspbench-request:j", r)
            .with_strategy("jspbench-plan-cpsat:j", TextPayload::new("existing"));
        let s = CpSatJobShopSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed(
            "jspbench-request:bad",
            TextPayload::new("not a jobshop request"),
        );
        let s = CpSatJobShopSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: Taillard 20×15 — a notoriously hard JSP benchmark size.
    /// 300 operations with random machine permutations and durations in
    /// [10, 80]. CP-SAT typically uses the full 30 s budget and may not
    /// prove optimality; the test verifies feasibility only.
    #[test]
    fn stress_30s_jobshop_20x15() {
        let n_jobs = 20;
        let n_machines = 15;
        let mut state: u64 = 0x1234_5678_ABCD_EF01;
        let step = |s: &mut u64| -> u64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *s
        };
        let mut jobs: Vec<Job> = Vec::with_capacity(n_jobs);
        for j in 0..n_jobs {
            let mut perm: Vec<usize> = (0..n_machines).collect();
            for i in (1..n_machines).rev() {
                let r = (step(&mut state) >> 33) as usize % (i + 1);
                perm.swap(i, r);
            }
            let ops = perm
                .into_iter()
                .map(|m| {
                    let dur = ((step(&mut state) >> 33) & 0x7F) as i64 + 10;
                    op(m, dur)
                })
                .collect();
            jobs.push(job(j, ops));
        }
        let r = req(jobs, n_machines, 30.0);
        let started = std::time::Instant::now();
        let plan = solve_cpsat_jsp(&r);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible job-shop plan, got {} in {elapsed:.1}s",
            plan.status
        );
        assert_eq!(plan.schedule.len(), n_jobs * n_machines);
        assert!(plan.makespan > 0);
    }
}
