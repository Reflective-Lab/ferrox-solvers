use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, ProvenanceSource, Suggestor};
use std::time::Instant;
use tracing::warn;

use crate::provenance::FERROX_PROVENANCE;
use crate::solver_identity::non_native_solver_identity;

use crate::domain_types::Minutes;

use super::problem::{SchedulingPlan, SchedulingRequest, SchedulingSolveStatus, TaskAssignment};

pub(super) const REQUEST_PREFIX: &str = "scheduling-request:";
const PLAN_PREFIX: &str = "scheduling-plan-greedy:";

/// Schedules tasks via Earliest-Deadline-First + earliest-available skilled agent.
///
/// **Algorithm:** O(n·m·log n) where n = tasks, m = agents.
///
/// **When to use:** latency-sensitive pipelines where a good schedule is needed
/// in microseconds. Use alongside [`CpSatSchedulerSuggestor`] in a Formation;
/// the Engine will emit both plans and the highest-confidence one (CP-SAT optimal)
/// will be accepted for execution.
///
/// **Confidence:** capped at 0.65 — greedy cannot prove optimality.
///
/// [`CpSatSchedulerSuggestor`]: super::cpsat::CpSatSchedulerSuggestor
pub struct GreedySchedulerSuggestor;

#[async_trait]
impl Suggestor for GreedySchedulerSuggestor {
    fn name(&self) -> &'static str {
        "GreedySchedulerSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("O(n·m·log n) — EDF + earliest-available; deterministic, sub-ms for n ≤ 10 000 tasks")
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

            match fact.require_payload::<SchedulingRequest>() {
                Ok(req) => {
                    let plan = solve_greedy(req);
                    // Greedy is fast but can't prove optimality — cap confidence at 0.65.
                    let confidence = (plan.throughput_ratio() * 0.65).min(0.65);
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
                    warn!(id = %fact.id(), error = %e, "unexpected scheduling-request payload");
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

/// Pure EDF + earliest-available scheduling.  No OR-Tools dependency.
pub fn solve_greedy(req: &SchedulingRequest) -> SchedulingPlan {
    let t0 = Instant::now();

    // Sort tasks by earliest deadline first.
    let mut ordered: Vec<_> = req.tasks.iter().collect();
    ordered.sort_by_key(|t| t.deadline_min);

    let mut next_free = vec![0i64; req.agents.len()];
    let mut assignments: Vec<TaskAssignment> = Vec::new();

    for task in &ordered {
        // Find capable agent whose earliest available time after release is smallest.
        let best = req
            .agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.capabilities.contains(&task.required_capability))
            .map(|(idx, a)| (idx, a, next_free[idx].max(task.release_min.0)))
            .min_by_key(|(_, a, start)| (*start, a.id.0));

        if let Some((idx, agent, start)) = best {
            let end = start + task.duration_min.0;
            if end <= task.deadline_min.0 {
                next_free[idx] = end;
                assignments.push(TaskAssignment {
                    task_id: task.id,
                    task_name: task.name.clone(),
                    agent_id: agent.id,
                    agent_name: agent.name.clone(),
                    start_min: Minutes(start),
                    end_min: Minutes(end),
                });
            }
        }
    }

    assignments.sort_by_key(|a| a.start_min);
    let makespan = assignments
        .iter()
        .map(|a| a.end_min)
        .max()
        .unwrap_or(Minutes(0));
    let scheduled = assignments.len();

    SchedulingPlan {
        request_id: req.id.clone(),
        assignments,
        tasks_total: req.tasks.len(),
        tasks_scheduled: scheduled,
        makespan_min: makespan,
        solver: "greedy-edf".to_string(),
        execution_identity: non_native_solver_identity(
            "greedy-edf",
            "algorithm=edf_earliest_available",
        ),
        status: SchedulingSolveStatus::Feasible,
        wall_time_seconds: t0.elapsed().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_types::{AgentId, Minutes, TaskId};
    use crate::scheduling::problem::{SchedulingAgent, SchedulingTask};
    use crate::test_support::MockContext;
    use converge_pack::Suggestor;
    use converge_pack::TextPayload;

    fn agent(id: usize, name: &str, caps: &[&str]) -> SchedulingAgent {
        SchedulingAgent {
            id: AgentId(id),
            name: name.into(),
            capabilities: caps.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn task(
        id: usize,
        name: &str,
        cap: &str,
        duration: i64,
        release: i64,
        deadline: i64,
    ) -> SchedulingTask {
        SchedulingTask {
            id: TaskId(id),
            name: name.into(),
            required_capability: cap.into(),
            duration_min: Minutes(duration),
            release_min: Minutes(release),
            deadline_min: Minutes(deadline),
        }
    }

    fn req_with(tasks: Vec<SchedulingTask>, agents: Vec<SchedulingAgent>) -> SchedulingRequest {
        SchedulingRequest {
            id: "r1".into(),
            agents,
            tasks,
            horizon_min: 480,
            time_limit_seconds: 1.0,
        }
    }

    #[test]
    fn empty_request_yields_empty_plan() {
        let req = req_with(vec![], vec![]);
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_total, 0);
        assert_eq!(plan.tasks_scheduled, 0);
        assert_eq!(plan.makespan_min, Minutes(0));
        assert!((plan.throughput_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_capable_agent_schedules_in_order() {
        let req = req_with(
            vec![
                task(1, "t1", "py", 30, 0, 60),
                task(2, "t2", "py", 30, 0, 120),
            ],
            vec![agent(0, "alice", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_scheduled, 2);
        assert_eq!(plan.makespan_min, Minutes(60));
        assert_eq!(plan.assignments[0].task_id, TaskId(1));
        assert!((plan.throughput_ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn drops_task_with_unsatisfiable_deadline() {
        let req = req_with(
            vec![task(1, "tight", "py", 100, 0, 30)],
            vec![agent(0, "alice", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_scheduled, 0);
        assert_eq!(plan.tasks_total, 1);
    }

    #[test]
    fn skips_task_when_no_capable_agent() {
        let req = req_with(
            vec![task(1, "rs-only", "rust", 30, 0, 120)],
            vec![agent(0, "alice", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_scheduled, 0);
    }

    #[test]
    fn picks_earliest_available_agent() {
        let req = req_with(
            vec![
                task(1, "t1", "py", 60, 0, 120),
                task(2, "t2", "py", 30, 0, 120),
            ],
            vec![agent(0, "alice", &["py"]), agent(1, "bob", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_scheduled, 2);
        let agents: std::collections::HashSet<_> =
            plan.assignments.iter().map(|a| a.agent_id).collect();
        assert_eq!(agents.len(), 2, "should distribute across both agents");
    }

    #[test]
    fn supports_non_dense_agent_ids() {
        let req = req_with(
            vec![
                task(1, "t1", "py", 30, 0, 90),
                task(2, "t2", "py", 30, 0, 90),
            ],
            vec![agent(10, "alice", &["py"]), agent(20, "bob", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.tasks_scheduled, 2);
        assert_eq!(plan.assignments[0].agent_id, AgentId(10));
    }

    #[test]
    fn release_time_delays_start() {
        let req = req_with(
            vec![task(1, "t1", "py", 30, 100, 200)],
            vec![agent(0, "alice", &["py"])],
        );
        let plan = solve_greedy(&req);
        assert_eq!(plan.assignments[0].start_min, Minutes(100));
        assert_eq!(plan.assignments[0].end_min, Minutes(130));
    }

    #[tokio::test]
    async fn suggestor_emits_proposal_for_seed() {
        let req = req_with(
            vec![task(1, "t1", "py", 30, 0, 60)],
            vec![agent(0, "alice", &["py"])],
        );
        let ctx = MockContext::empty().with_seed(&format!("{REQUEST_PREFIX}r1"), req);
        let s = GreedySchedulerSuggestor;
        assert_eq!(s.name(), "GreedySchedulerSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let effect = s.execute(&ctx).await;
        assert_eq!(effect.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_if_plan_already_present() {
        let req = req_with(
            vec![task(1, "t1", "py", 30, 0, 60)],
            vec![agent(0, "alice", &["py"])],
        );
        let ctx = MockContext::empty()
            .with_seed(&format!("{REQUEST_PREFIX}r1"), req)
            .with_strategy("scheduling-plan-greedy:r1", TextPayload::new("existing"));
        let s = GreedySchedulerSuggestor;
        assert!(!s.accepts(&ctx));
        let effect = s.execute(&ctx).await;
        assert_eq!(effect.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed(
            &format!("{REQUEST_PREFIX}bad"),
            TextPayload::new("not a scheduling request"),
        );
        let s = GreedySchedulerSuggestor;
        let effect = s.execute(&ctx).await;
        assert_eq!(effect.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_ignores_other_seed_prefixes() {
        let ctx = MockContext::empty().with_seed("unrelated:r1", TextPayload::new("unrelated"));
        let s = GreedySchedulerSuggestor;
        assert!(!s.accepts(&ctx));
    }
}
