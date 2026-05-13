//! Multi-Agent Task Assignment with Time Windows — Formation demo.
//!
//! Registers two competing suggestors in a single Converge Engine:
//!
//! - `GreedySchedulerSuggestor` — EDF heuristic, answers in < 1 ms
//! - `CpSatSchedulerSuggestor` — OR-Tools CP-SAT, proves optimality
//!
//! Both accept the same `scheduling-request:` seed.  Each emits a
//! solver-prefixed plan to `ContextKey::Strategies`.  The demo compares both
//! plans and highlights the quality gap.
//!
//! Run with:
//!
//! ```bash
//! just example-maatw
//! ```

use converge_core::{ContextState, Engine};
use converge_pack::{ContextKey, Suggestor};
use ferrox::scheduling::{
    CpSatSchedulerSuggestor, GreedySchedulerSuggestor, SchedulingAgent, SchedulingPlan,
    SchedulingRequest, SchedulingTask,
};

// ── Problem parameters ────────────────────────────────────────────────────────

const NUM_AGENTS: usize = 12;
const NUM_TASKS: usize = 60;
const HORIZON: i64 = 360;
const SKILLS: &[&str] = &["python", "rust", "ml", "data", "api"];

// ── Deterministic LCG — no rand dep ──────────────────────────────────────────

fn lcg(s: &mut u64) -> u64 {
    *s = s
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *s >> 33
}
fn rng_range(s: &mut u64, lo: u64, hi: u64) -> u64 {
    lo + lcg(s) % (hi - lo)
}

// ── Instance generator ────────────────────────────────────────────────────────

fn build_request() -> SchedulingRequest {
    let mut rng = 42u64;

    let agents: Vec<SchedulingAgent> = (0..NUM_AGENTS)
        .map(|id| {
            let n = if rng_range(&mut rng, 0, 4) == 0 {
                2usize
            } else {
                1
            };
            let mut caps: Vec<String> = Vec::new();
            for _ in 0..n {
                let s = SKILLS[rng_range(&mut rng, 0, SKILLS.len() as u64) as usize].to_string();
                if !caps.contains(&s) {
                    caps.push(s);
                }
            }
            SchedulingAgent {
                id,
                name: format!("agent-{id:02}"),
                capabilities: caps,
            }
        })
        .collect();

    let covered: Vec<bool> = SKILLS
        .iter()
        .map(|s| {
            agents
                .iter()
                .any(|a| a.capabilities.iter().any(|c| c == *s))
        })
        .collect();

    let tasks: Vec<SchedulingTask> = (0..NUM_TASKS)
        .filter_map(|id| {
            let mut si = rng_range(&mut rng, 0, SKILLS.len() as u64) as usize;
            while !covered[si] {
                si = (si + 1) % SKILLS.len();
            }
            let skill = SKILLS[si].to_string();
            let dur = rng_range(&mut rng, 20, 75) as i64;
            let rel = rng_range(&mut rng, 0, 181) as i64;
            let slack = rng_range(&mut rng, 2, 4) as i64;
            let dl = (rel + dur * slack).min(HORIZON);
            if dl < rel + dur {
                return None; // skip infeasible window
            }
            Some(SchedulingTask {
                id,
                name: format!("task-{id:03}"),
                required_capability: skill,
                duration_min: dur,
                release_min: rel,
                deadline_min: dl,
            })
        })
        .collect();

    SchedulingRequest {
        id: "maatw-demo".to_string(),
        agents,
        tasks,
        horizon_min: HORIZON,
        time_limit_seconds: 30.0,
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let req = build_request();

    // Skill distribution for context.
    let mut skill_agents = std::collections::HashMap::<String, usize>::new();
    let mut skill_tasks = std::collections::HashMap::<String, usize>::new();
    for a in &req.agents {
        for c in &a.capabilities {
            *skill_agents.entry(c.clone()).or_default() += 1;
        }
    }
    for t in &req.tasks {
        *skill_tasks
            .entry(t.required_capability.clone())
            .or_default() += 1;
    }

    println!("\n══════════════════════════════════════════════════════════════");
    println!("  MAATW Formation Demo");
    println!(
        "  {} agents · {} tasks · {} skills · horizon {} min",
        req.agents.len(),
        req.tasks.len(),
        SKILLS.len(),
        HORIZON
    );
    println!("══════════════════════════════════════════════════════════════\n");

    println!("Skill coverage (agents / tasks):");
    for s in SKILLS {
        println!(
            "  {:8}  {:2} agents  {:3} tasks",
            s,
            skill_agents.get(*s).copied().unwrap_or(0),
            skill_tasks.get(*s).copied().unwrap_or(0)
        );
    }
    println!();

    // ── Formation: two suggestors, one engine ─────────────────────────────────

    let mut engine = Engine::new();
    engine.register_suggestor(GreedySchedulerSuggestor);
    engine.register_suggestor(CpSatSchedulerSuggestor);

    let mut ctx = ContextState::new();
    ctx.add_input(
        ContextKey::Seeds,
        "scheduling-request:maatw-demo",
        serde_json::to_string(&req).unwrap(),
    )
    .unwrap();

    let result = engine.run(ctx).await.unwrap();
    let strategies = result.context.get(ContextKey::Strategies);

    // Collect greedy and CP-SAT plans.
    let greedy_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("scheduling-plan-greedy:"))
        .and_then(|f| serde_json::from_str::<SchedulingPlan>(f.content()).ok());

    let cpsat_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("scheduling-plan-cpsat:"))
        .and_then(|f| serde_json::from_str::<SchedulingPlan>(f.content()).ok());

    // ── Report ────────────────────────────────────────────────────────────────

    if let Some(g) = &greedy_plan {
        let pct = g.tasks_scheduled as f64 / g.tasks_total as f64 * 100.0;
        let conf = (g.throughput_ratio() * 0.65 * 100.0).min(65.0);
        println!("── GreedySchedulerSuggestor ──────────────────────────────────");
        println!(
            "  Hint:        {}",
            GreedySchedulerSuggestor.complexity_hint().unwrap_or("-")
        );
        println!(
            "  Throughput:  {} / {} tasks  ({:.1}%)",
            g.tasks_scheduled, g.tasks_total, pct
        );
        println!("  Makespan:    {} min", g.makespan_min);
        println!(
            "  Confidence:  {:.2}  (greedy cannot prove optimality)",
            conf / 100.0
        );
        println!("  Time:        {:.2} ms\n", g.wall_time_seconds * 1000.0);

        println!("  Sample (first 5 by start):");
        for a in g.assignments.iter().take(5) {
            let task = req.tasks.iter().find(|t| t.id == a.task_id).unwrap();
            println!(
                "    {:10} [{:6}, {:3}min, win={:3}-{:3}] → {} @ t={:3}..{:3}",
                a.task_name,
                task.required_capability,
                task.duration_min,
                task.release_min,
                task.deadline_min,
                a.agent_name,
                a.start_min,
                a.end_min
            );
        }
        println!();
    }

    if let Some(cp) = &cpsat_plan {
        let pct = cp.tasks_scheduled as f64 / cp.tasks_total as f64 * 100.0;
        let conf = match cp.status.as_str() {
            "optimal" => cp.throughput_ratio(),
            "feasible" => cp.throughput_ratio() * 0.85,
            _ => 0.0,
        };
        let gain = cp.tasks_scheduled as i64
            - greedy_plan.as_ref().map_or(0, |g| g.tasks_scheduled as i64);

        println!("── CpSatSchedulerSuggestor ───────────────────────────────────");
        println!(
            "  Hint:        {}",
            CpSatSchedulerSuggestor.complexity_hint().unwrap_or("-")
        );
        println!("  Status:      {}", cp.status);
        println!(
            "  Throughput:  {} / {} tasks  ({:.1}%)  ← +{} vs greedy",
            cp.tasks_scheduled, cp.tasks_total, pct, gain
        );
        println!("  Makespan:    {} min", cp.makespan_min);
        println!("  Confidence:  {:.2}", conf);
        println!("  Time:        {:.0} ms\n", cp.wall_time_seconds * 1000.0);

        println!("  Sample (first 5 by start):");
        for a in cp.assignments.iter().take(5) {
            let task = req.tasks.iter().find(|t| t.id == a.task_id).unwrap();
            println!(
                "    {:10} [{:6}, {:3}min, win={:3}-{:3}] → {} @ t={:3}..{:3}",
                a.task_name,
                task.required_capability,
                task.duration_min,
                task.release_min,
                task.deadline_min,
                a.agent_name,
                a.start_min,
                a.end_min
            );
        }
        println!();

        // Utilisation per agent.
        let mut load = vec![0i64; req.agents.len()];
        for a in &cp.assignments {
            load[a.agent_id] += req
                .tasks
                .iter()
                .find(|t| t.id == a.task_id)
                .map_or(0, |t| t.duration_min);
        }
        let active = load.iter().filter(|&&l| l > 0).count();
        let total_work: i64 = load.iter().sum();
        let avg_util = if cp.makespan_min > 0 && active > 0 {
            total_work as f64 / (cp.makespan_min as f64 * active as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Active agents:  {} / {}   avg utilisation {:.1}%",
            active,
            req.agents.len(),
            avg_util
        );
        println!();
    }

    // ── Formation verdict ─────────────────────────────────────────────────────

    println!("── Formation verdict ─────────────────────────────────────────");
    match (&greedy_plan, &cpsat_plan) {
        (Some(g), Some(cp)) => {
            let winner = if cp.tasks_scheduled >= g.tasks_scheduled {
                "CpSatSchedulerSuggestor"
            } else {
                "GreedySchedulerSuggestor"
            };
            println!("  Winner:  {winner}");
            println!(
                "  Gain:    +{} tasks over greedy  ({:.1}% throughput improvement)",
                cp.tasks_scheduled as i64 - g.tasks_scheduled as i64,
                (cp.tasks_scheduled as f64 - g.tasks_scheduled as f64) / g.tasks_scheduled as f64
                    * 100.0
            );
        }
        (Some(_), None) => println!("  Winner:  GreedySchedulerSuggestor (CP-SAT unavailable)"),
        (None, Some(_)) => println!("  Winner:  CpSatSchedulerSuggestor"),
        _ => println!("  No plans produced."),
    }

    println!("══════════════════════════════════════════════════════════════\n");
}
