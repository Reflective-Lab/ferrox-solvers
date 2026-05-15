use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, ExecutionIdentity, Suggestor};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::CpModel;
use std::collections::HashMap;
use std::time::Instant;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};
use crate::solver_identity::cp_sat_solver_identity;

use super::problem::{SchedulingPlan, SchedulingRequest, SchedulingTask, TaskAssignment};

const PLAN_PREFIX: &str = "scheduling-plan-cpsat:";

use super::greedy::REQUEST_PREFIX;

/// Schedules tasks to optimality using CP-SAT optional-interval variables and
/// per-agent `NoOverlap` constraints.
///
/// **Algorithm:** CP-SAT (DPLL + LNS + clause learning).  Explores the full
/// combinatorial space; guarantees optimality within the time budget.
///
/// **When to use:** batch planning, pre-flight checks, or any context where
/// schedule quality matters more than latency.  Pair with
/// [`GreedySchedulerSuggestor`] in a Formation: greedy provides an immediate
/// warm-start answer; CP-SAT proves (or improves) it before execution begins.
///
/// **Confidence:**
/// - `optimal` status + 100% throughput → 1.0
/// - `optimal` status + partial throughput → throughput ratio (resource-limited)
/// - `feasible` status → `throughput_ratio` × 0.85 (time budget exhausted)
/// - `infeasible` → 0.0
///
/// [`GreedySchedulerSuggestor`]: super::greedy::GreedySchedulerSuggestor
pub struct CpSatSchedulerSuggestor;

#[async_trait]
impl Suggestor for CpSatSchedulerSuggestor {
    fn name(&self) -> &'static str {
        "CpSatSchedulerSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some(concat!(
            "NP-hard in general; CP-SAT DPLL+LNS with optional-interval NoOverlap; ",
            "proves optimality for n ≤ 100 tasks within 30 s on 10-core hardware"
        ))
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds).iter().any(|f| {
            f.id().starts_with(REQUEST_PREFIX) && !own_plan_exists(ctx, request_id(f.id()))
        })
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
            if own_plan_exists(ctx, rid) {
                continue;
            }

            match fact.require_payload::<SchedulingRequest>() {
                Ok(req) => {
                    let plan = solve_cpsat(req);
                    let confidence = match plan.status.as_str() {
                        "optimal" => plan.throughput_ratio(),
                        "feasible" => plan.throughput_ratio() * 0.85,
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

// ── Solver ────────────────────────────────────────────────────────────────────

/// Maximise tasks scheduled subject to agent-capacity (`NoOverlap`) and
/// time-window constraints.  Returns a [`SchedulingPlan`] with full assignments.
#[allow(clippy::too_many_lines)]
pub fn solve_cpsat(req: &SchedulingRequest) -> SchedulingPlan {
    let t0 = Instant::now();
    if let Err(reason) = validate_scheduling_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid scheduling-request");
        return empty_plan(req, "invalid", t0.elapsed().as_secs_f64());
    }

    let mut model = CpModel::new();

    // Index helpers
    let mut name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut bool_name_to_idx: HashMap<String, i32> = HashMap::new();
    let mut interval_name_to_idx: HashMap<String, i32> = HashMap::new();

    // ── Per-task start / end variables ────────────────────────────────────────

    for task in &req.tasks {
        let Some((s_ub, e_lb)) = feasible_window(task) else {
            continue;
        };

        let s = model.new_int_var(task.release_min, s_ub, &start_name(task));
        let e = model.new_int_var(e_lb, task.deadline_min, &end_name(task));

        name_to_idx.insert(start_name(task), s);
        name_to_idx.insert(end_name(task), e);
    }

    // ── Per-(task, agent) assignment booleans + optional intervals ────────────

    // agent_id → list of optional interval var indices for that agent
    let mut agent_interval_idxs: Vec<Vec<i32>> = vec![Vec::new(); req.agents.len()];
    // task_index → list of x variable names for that task
    let mut task_assign_names: Vec<Vec<String>> = vec![Vec::new(); req.tasks.len()];

    for (ti, task) in req.tasks.iter().enumerate() {
        for (agent_idx, agent) in req
            .agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.capabilities.contains(&task.required_capability))
        {
            let x_name = x_var_name(task.id, agent.id);
            let ov_name = ov_var_name(task.id, agent.id);

            let (Some(&s_idx), Some(&e_idx)) = (
                name_to_idx.get(&start_name(task)),
                name_to_idx.get(&end_name(task)),
            ) else {
                continue;
            };

            let x_idx = model.new_bool_var(&x_name);
            bool_name_to_idx.insert(x_name.clone(), x_idx);
            name_to_idx.insert(x_name.clone(), x_idx);

            let ov_idx =
                model.new_optional_interval_var(s_idx, task.duration_min, e_idx, x_idx, &ov_name);
            interval_name_to_idx.insert(ov_name, ov_idx);

            agent_interval_idxs[agent_idx].push(ov_idx);
            task_assign_names[ti].push(x_name);
        }
    }

    // ── Constraints ───────────────────────────────────────────────────────────

    // 1. Each task assigned to at most one capable agent (optional scheduling).
    for names in &task_assign_names {
        if names.len() > 1 {
            let vars: Vec<i32> = names.iter().map(|n| name_to_idx[n]).collect();
            let ones = vec![1i64; vars.len()];
            model.add_linear_le(&vars, &ones, 1);
        }
    }

    // 2. No two tasks overlap on the same agent.
    for agent_ivs in &agent_interval_idxs {
        if agent_ivs.len() > 1 {
            model.add_no_overlap(agent_ivs);
        }
    }

    // ── Objective: maximise total tasks scheduled ─────────────────────────────

    let obj_vars: Vec<i32> = bool_name_to_idx.values().copied().collect();
    let obj_coeffs = vec![1i64; obj_vars.len()];
    model.maximize(&obj_vars, &obj_coeffs);

    let solution = model.solve(req.time_limit_seconds);
    let elapsed = t0.elapsed().as_secs_f64();

    let status = match solution.status() {
        OrtoolsStatus::Optimal => "optimal",
        OrtoolsStatus::Feasible => "feasible",
        OrtoolsStatus::Infeasible => "infeasible",
        OrtoolsStatus::Unbounded => "unbounded",
        _ => "error",
    };

    if !solution.status().is_success() {
        return empty_plan(req, status, elapsed);
    }

    // ── Extract assignments ───────────────────────────────────────────────────

    let mut assignments: Vec<TaskAssignment> = Vec::new();

    for task in &req.tasks {
        for agent in req
            .agents
            .iter()
            .filter(|a| a.capabilities.contains(&task.required_capability))
        {
            let x_name = x_var_name(task.id, agent.id);
            if let Some(&x_idx) = bool_name_to_idx.get(&x_name)
                && solution.value(x_idx) == 1
            {
                let s_idx = name_to_idx[&start_name(task)];
                let start = solution.value(s_idx);
                assignments.push(TaskAssignment {
                    task_id: task.id,
                    task_name: task.name.clone(),
                    agent_id: agent.id,
                    agent_name: agent.name.clone(),
                    start_min: start,
                    end_min: start + task.duration_min,
                });
                break;
            }
        }
    }

    assignments.sort_by_key(|a| a.start_min);
    let makespan = assignments.iter().map(|a| a.end_min).max().unwrap_or(0);
    let scheduled = assignments.len();

    SchedulingPlan {
        request_id: req.id.clone(),
        assignments,
        tasks_total: req.tasks.len(),
        tasks_scheduled: scheduled,
        makespan_min: makespan,
        solver: "cp-sat-v9.15".to_string(),
        solver_identity: scheduling_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds: elapsed,
    }
}

// ── Variable name helpers ─────────────────────────────────────────────────────

fn start_name(task: &SchedulingTask) -> String {
    format!("s_{}", task.id)
}
fn end_name(task: &SchedulingTask) -> String {
    format!("e_{}", task.id)
}
fn x_var_name(task_id: usize, agent_id: usize) -> String {
    format!("x_{task_id}_{agent_id}")
}
fn ov_var_name(task_id: usize, agent_id: usize) -> String {
    format!("ov_{task_id}_{agent_id}")
}

fn validate_scheduling_request(req: &SchedulingRequest) -> Result<(), String> {
    if req.id.trim().is_empty() {
        return Err("request id must not be empty".to_string());
    }
    if req.id.contains('\0') {
        return Err("request id contains an interior NUL byte".to_string());
    }
    if !req.time_limit_seconds.is_finite() || req.time_limit_seconds <= 0.0 {
        return Err("time_limit_seconds must be finite and positive".to_string());
    }
    Ok(())
}

fn feasible_window(task: &SchedulingTask) -> Option<(i64, i64)> {
    if task.duration_min < 0 {
        return None;
    }
    let latest_start = task.deadline_min.checked_sub(task.duration_min)?;
    let earliest_end = task.release_min.checked_add(task.duration_min)?;
    (earliest_end <= task.deadline_min && task.release_min <= latest_start)
        .then_some((latest_start, earliest_end))
}

fn empty_plan(req: &SchedulingRequest, status: &str, wall_time_seconds: f64) -> SchedulingPlan {
    SchedulingPlan {
        request_id: req.id.clone(),
        assignments: Vec::new(),
        tasks_total: req.tasks.len(),
        tasks_scheduled: 0,
        makespan_min: 0,
        solver: "cp-sat-v9.15".to_string(),
        solver_identity: scheduling_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds,
    }
}

fn scheduling_cpsat_identity(req: &SchedulingRequest) -> ExecutionIdentity {
    cp_sat_solver_identity(format!(
        "time_limit_seconds={}; search_workers=hardware_concurrency; objective=maximize_tasks_scheduled",
        req.time_limit_seconds
    ))
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_wrap,
    clippy::doc_markdown,
    clippy::similar_names
)]
mod tests {
    use super::*;
    use crate::scheduling::problem::{SchedulingAgent, SchedulingTask};
    use crate::test_support::MockContext;
    use converge_pack::TextPayload;

    fn agent(id: usize, caps: &[&str]) -> SchedulingAgent {
        SchedulingAgent {
            id,
            name: format!("a{id}"),
            capabilities: caps.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn task(id: usize, cap: &str, duration: i64, release: i64, deadline: i64) -> SchedulingTask {
        SchedulingTask {
            id,
            name: format!("t{id}"),
            required_capability: cap.into(),
            duration_min: duration,
            release_min: release,
            deadline_min: deadline,
        }
    }

    fn req(tasks: Vec<SchedulingTask>, agents: Vec<SchedulingAgent>) -> SchedulingRequest {
        SchedulingRequest {
            id: "r".into(),
            agents,
            tasks,
            horizon_min: 480,
            time_limit_seconds: 5.0,
        }
    }

    #[test]
    fn small_instance_optimal() {
        let r = req(
            vec![
                task(1, "py", 30, 0, 60),
                task(2, "py", 30, 0, 120),
                task(3, "py", 30, 60, 120),
            ],
            vec![agent(0, &["py"])],
        );
        let plan = solve_cpsat(&r);
        assert_eq!(plan.status, "optimal");
        assert_eq!(plan.solver_identity.backend, "cp-sat-v9.15");
        assert!(
            plan.solver_identity
                .native_identity
                .as_ref()
                .is_some_and(|native| native.backend.contains("OR-Tools"))
        );
        assert_eq!(plan.tasks_scheduled, 3);
        assert_eq!(plan.solver, "cp-sat-v9.15");
        for a in &plan.assignments {
            assert!(a.end_min - a.start_min == 30);
        }
    }

    #[test]
    fn unschedulable_task_drops_to_zero() {
        // duration > deadline; bound math is degenerate so the model is rejected.
        let r = req(vec![task(1, "py", 100, 0, 30)], vec![agent(0, &["py"])]);
        let plan = solve_cpsat(&r);
        assert_eq!(plan.tasks_scheduled, 0);
    }

    #[test]
    fn rejects_invalid_time_limit_without_panic() {
        let mut r = req(vec![task(1, "py", 10, 0, 30)], vec![agent(0, &["py"])]);
        r.time_limit_seconds = f64::NAN;
        let plan = solve_cpsat(&r);
        assert_eq!(plan.status, "invalid");
        assert!(plan.assignments.is_empty());
    }

    #[test]
    fn capability_routing() {
        let r = req(
            vec![task(1, "rs", 10, 0, 60), task(2, "py", 10, 0, 60)],
            vec![agent(0, &["py"]), agent(1, &["rs"])],
        );
        let plan = solve_cpsat(&r);
        assert_eq!(plan.tasks_scheduled, 2);
        let by_id: HashMap<_, _> = plan
            .assignments
            .iter()
            .map(|a| (a.task_id, a.agent_id))
            .collect();
        assert_eq!(by_id[&1], 1);
        assert_eq!(by_id[&2], 0);
    }

    #[test]
    fn supports_non_dense_agent_ids() {
        let r = req(
            vec![task(1, "py", 10, 0, 60), task(2, "py", 10, 0, 60)],
            vec![agent(10, &["py"]), agent(20, &["py"])],
        );
        let plan = solve_cpsat(&r);
        assert_eq!(plan.tasks_scheduled, 2);
        assert!(
            plan.assignments
                .iter()
                .all(|a| a.agent_id == 10 || a.agent_id == 20)
        );
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let r = req(vec![task(1, "py", 10, 0, 60)], vec![agent(0, &["py"])]);
        let ctx = MockContext::empty().with_seed("scheduling-request:r", r);
        let s = CpSatSchedulerSuggestor;
        assert_eq!(s.name(), "CpSatSchedulerSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let r = req(vec![task(1, "py", 10, 0, 60)], vec![agent(0, &["py"])]);
        let ctx = MockContext::empty()
            .with_seed("scheduling-request:r", r)
            .with_strategy("scheduling-plan-cpsat:r", TextPayload::new("existing"));
        let s = CpSatSchedulerSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed(
            "scheduling-request:bad",
            TextPayload::new("not a scheduling request"),
        );
        let s = CpSatSchedulerSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: 250 tasks across 8 agents with tight time windows and high
    /// contention on a shared capability. Optional-interval NoOverlap on each
    /// agent generates a deep search space — designed to consume the 30 s budget.
    #[test]
    fn stress_30s_250_tasks_tight_windows() {
        let caps = ["py", "rs", "ml"];
        // Few agents, narrow capability coverage → high resource contention.
        let agents: Vec<_> = (0..8).map(|i| agent(i, &[caps[i % caps.len()]])).collect();
        let n_tasks = 250;
        let mut state: u64 = 0xDEAD_BEEF_F00D_F00D;
        let step = |s: &mut u64| -> i64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((*s >> 33) & 0x3F) as i64
        };
        let tasks: Vec<_> = (0..n_tasks)
            .map(|i| {
                let cap = caps[i % caps.len()];
                let release = step(&mut state) * 5;
                // Narrow deadline window forces overlap conflicts.
                let duration = 20 + step(&mut state) % 30;
                let slack = 30 + step(&mut state) % 60;
                let deadline = release + duration + slack;
                task(i, cap, duration, release, deadline)
            })
            .collect();
        let r = SchedulingRequest {
            id: "stress".into(),
            agents,
            tasks,
            horizon_min: 2_000,
            time_limit_seconds: 30.0,
        };
        let started = std::time::Instant::now();
        let plan = solve_cpsat(&r);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible scheduling plan, got {} in {elapsed:.1}s",
            plan.status
        );
        assert!(plan.tasks_scheduled > 0);
        for a in &plan.assignments {
            assert!(a.end_min > a.start_min);
        }
    }
}
