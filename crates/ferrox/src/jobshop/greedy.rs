use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, ProposedFact, Suggestor};
use std::time::Instant;
use tracing::warn;

use super::problem::{JobShopPlan, JobShopRequest, ScheduledOp};

pub(super) const REQUEST_PREFIX: &str = "jspbench-request:";
const PLAN_PREFIX: &str = "jspbench-plan-greedy:";

/// Schedules a Job Shop instance via List Scheduling with Shortest Processing Time
/// (SPT) dispatching.
///
/// **Algorithm:** O(n·m²) where n = jobs, m = machines.  At each scheduling
/// event (a machine becomes free), all operations waiting for that machine are
/// ranked by duration; the shortest is dispatched first.
///
/// **Confidence:** capped at 0.60 — SPT can be arbitrarily bad vs. optimal in
/// the worst case.  Use alongside [`CpSatJobShopSuggestor`] in a Formation.
///
/// [`CpSatJobShopSuggestor`]: super::cpsat::CpSatJobShopSuggestor
pub struct GreedyJobShopSuggestor;

#[async_trait]
impl Suggestor for GreedyJobShopSuggestor {
    fn name(&self) -> &'static str {
        "GreedyJobShopSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("O(n·m²) — SPT list scheduling; sub-ms for n ≤ 1 000 jobs × m ≤ 100 machines")
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds).iter().any(|f| {
            f.id().starts_with(REQUEST_PREFIX) && !own_plan_exists(ctx, request_id(f.id()))
        })
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

            match serde_json::from_str::<JobShopRequest>(fact.content()) {
                Ok(req) => {
                    let plan = solve_greedy(&req);
                    // SPT dispatching: bounded confidence.
                    let confidence = 0.55_f64;
                    proposals.push(
                        ProposedFact::new(
                            ContextKey::Strategies,
                            format!("{PLAN_PREFIX}{rid}"),
                            serde_json::to_string(&plan).unwrap_or_default(),
                            self.name(),
                        )
                        .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "malformed jspbench-request");
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

/// SPT list scheduling: repeatedly dispatch the shortest ready operation per machine.
#[allow(clippy::needless_range_loop)]
pub fn solve_greedy(req: &JobShopRequest) -> JobShopPlan {
    let t0 = Instant::now();
    let n = req.jobs.len();
    let m = req.num_machines;

    // next_op[j] = index of the next operation to schedule for job j
    let mut next_op = vec![0usize; n];
    // job_free[j]  = earliest time job j's next op can start (after predecessor)
    let mut job_free = vec![0i64; n];
    // machine_free[m] = earliest time machine m is free
    let mut machine_free = vec![0i64; m];

    let mut schedule: Vec<ScheduledOp> = Vec::new();
    let total_ops: usize = req.jobs.iter().map(|j| j.operations.len()).sum();

    while schedule.len() < total_ops {
        // For each machine, find all operations currently ready for it.
        // "Ready" = job's predecessor op is done AND this op's machine matches.
        let mut dispatched_any = false;

        for mach in 0..m {
            // Collect candidates: ready operations for this machine.
            let mut candidates: Vec<(usize, usize, i64, i64)> = Vec::new(); // (job, op_idx, earliest_start, duration)
            for (j, job) in req.jobs.iter().enumerate() {
                let k = next_op[j];
                if k >= job.operations.len() {
                    continue;
                }
                let op = &job.operations[k];
                if op.machine_id != mach {
                    continue;
                }
                let earliest = job_free[j].max(machine_free[mach]);
                candidates.push((j, k, earliest, op.duration));
            }

            if candidates.is_empty() {
                continue;
            }

            // SPT: choose the candidate with shortest duration; break ties by job id.
            candidates.sort_by_key(|&(j, _, _, dur)| (dur, j));
            let (j, k, earliest, dur) = candidates[0];

            let start = earliest;
            let end = start + dur;
            machine_free[mach] = end;
            job_free[j] = end;
            next_op[j] = k + 1;

            schedule.push(ScheduledOp {
                job_id: j,
                job_name: req.jobs[j].name.clone(),
                machine_id: mach,
                op_index: k,
                start,
                end,
            });
            dispatched_any = true;
        }

        if !dispatched_any {
            // Advance time to the next machine-free event.
            let next_t = machine_free
                .iter()
                .copied()
                .filter(|&t| {
                    // Only machines that still have pending ops.
                    (0..n).any(|j| {
                        let k = next_op[j];
                        k < req.jobs[j].operations.len()
                            && req.jobs[j].operations[k].machine_id
                                == machine_free
                                    .iter()
                                    .position(|&f| f == t)
                                    .unwrap_or(usize::MAX)
                    })
                })
                .min();

            if next_t.is_none() {
                // Deadlock guard: just advance the global minimum.
                let min_t = machine_free.iter().copied().min().unwrap_or(0);
                for mf in &mut machine_free {
                    if *mf == min_t {
                        *mf = 0; // reset to allow re-dispatch
                    }
                }
            }
        }
    }

    schedule.sort_by_key(|s| (s.machine_id, s.start));
    let makespan = schedule.iter().map(|s| s.end).max().unwrap_or(0);

    JobShopPlan {
        request_id: req.id.clone(),
        schedule,
        makespan,
        lower_bound: None,
        solver: "greedy-spt".to_string(),
        status: "feasible".to_string(),
        wall_time_seconds: t0.elapsed().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobshop::problem::{Job, Operation};
    use crate::test_support::MockContext;
    use converge_pack::Suggestor;

    fn op(machine: usize, dur: i64) -> Operation {
        Operation {
            machine_id: machine,
            duration: dur,
        }
    }

    fn job(id: usize, name: &str, ops: Vec<Operation>) -> Job {
        Job {
            id,
            name: name.into(),
            operations: ops,
        }
    }

    fn req(jobs: Vec<Job>, m: usize) -> JobShopRequest {
        JobShopRequest {
            id: "j1".into(),
            jobs,
            num_machines: m,
            time_limit_seconds: 1.0,
        }
    }

    #[test]
    fn empty_request_returns_zero_makespan() {
        let r = req(vec![], 1);
        let plan = solve_greedy(&r);
        assert_eq!(plan.makespan, 0);
        assert_eq!(plan.schedule.len(), 0);
        assert_eq!(plan.solver, "greedy-spt");
    }

    #[test]
    fn single_job_two_ops_serial() {
        let r = req(vec![job(0, "j0", vec![op(0, 5), op(1, 3)])], 2);
        let plan = solve_greedy(&r);
        assert_eq!(plan.schedule.len(), 2);
        assert_eq!(plan.makespan, 8);
        assert_eq!(plan.schedule[0].start, 0);
        assert_eq!(plan.schedule[0].end, 5);
        assert_eq!(plan.schedule[1].start, 5);
        assert_eq!(plan.schedule[1].end, 8);
    }

    #[test]
    fn spt_prefers_shorter_op_on_same_machine() {
        let r = req(
            vec![
                job(0, "long", vec![op(0, 10)]),
                job(1, "short", vec![op(0, 2)]),
            ],
            1,
        );
        let plan = solve_greedy(&r);
        assert_eq!(plan.schedule.len(), 2);
        // SPT: shorter goes first
        assert_eq!(plan.schedule[0].job_id, 1);
        assert_eq!(plan.schedule[1].job_id, 0);
    }

    #[test]
    fn parallel_machines_run_concurrently() {
        let r = req(
            vec![job(0, "ja", vec![op(0, 4)]), job(1, "jb", vec![op(1, 3)])],
            2,
        );
        let plan = solve_greedy(&r);
        assert_eq!(plan.makespan, 4);
    }

    #[test]
    fn horizon_sums_all_durations() {
        let r = req(
            vec![
                job(0, "j", vec![op(0, 3), op(1, 4)]),
                job(1, "k", vec![op(0, 2)]),
            ],
            2,
        );
        assert_eq!(r.horizon(), 9);
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let r = req(vec![job(0, "j0", vec![op(0, 5)])], 1);
        let body = serde_json::to_string(&r).unwrap();
        let ctx = MockContext::empty().with_seed(&format!("{REQUEST_PREFIX}j1"), &body);
        let s = GreedyJobShopSuggestor;
        assert_eq!(s.name(), "GreedyJobShopSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let r = req(vec![job(0, "j0", vec![op(0, 5)])], 1);
        let body = serde_json::to_string(&r).unwrap();
        let ctx = MockContext::empty()
            .with_seed(&format!("{REQUEST_PREFIX}j1"), &body)
            .with_strategy("jspbench-plan-greedy:j1", "{}");
        let s = GreedyJobShopSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_skips_malformed_seed() {
        let ctx = MockContext::empty().with_seed(&format!("{REQUEST_PREFIX}bad"), "{not json");
        let s = GreedyJobShopSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }
}
