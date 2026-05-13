//! Job Shop Scheduling — Formation demo.
//!
//! Registers two competing suggestors in a single Converge Engine:
//!
//! - `GreedyJobShopSuggestor` — SPT list scheduling, answers in < 1 ms
//! - `CpSatJobShopSuggestor`  — OR-Tools CP-SAT interval+NoOverlap, proves optimality
//!
//! Both accept the same `jspbench-request:` seed.  Each emits a solver-prefixed
//! plan to `ContextKey::Strategies`.  The demo uses a Taillard-style 15×10
//! instance and compares both plans.
//!
//! Run with:
//!
//! ```bash
//! just example-jspbench
//! ```

use converge_core::{ContextState, Engine};
use converge_pack::{ContextKey, Suggestor};
use ferrox::jobshop::{
    CpSatJobShopSuggestor, GreedyJobShopSuggestor, Job, JobShopPlan, JobShopRequest, Operation,
};

// ── Problem parameters ────────────────────────────────────────────────────────

const NUM_JOBS: usize = 15;
const NUM_MACHINES: usize = 10;

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

// ── Taillard-style instance generator ────────────────────────────────────────

fn build_request() -> JobShopRequest {
    let mut rng = 7u64;

    let jobs: Vec<Job> = (0..NUM_JOBS)
        .map(|id| {
            // Random permutation of machines (each machine visited exactly once).
            let mut order: Vec<usize> = (0..NUM_MACHINES).collect();
            // Fisher-Yates shuffle.
            for i in (1..NUM_MACHINES).rev() {
                let j = rng_range(&mut rng, 0, (i + 1) as u64) as usize;
                order.swap(i, j);
            }
            let operations: Vec<Operation> = order
                .into_iter()
                .map(|machine_id| Operation {
                    machine_id,
                    duration: rng_range(&mut rng, 15, 100) as i64,
                })
                .collect();
            Job {
                id,
                name: format!("job-{id:02}"),
                operations,
            }
        })
        .collect();

    JobShopRequest {
        id: "jspbench-demo".to_string(),
        jobs,
        num_machines: NUM_MACHINES,
        time_limit_seconds: 30.0,
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let req = build_request();

    let horizon = req.horizon();
    let total_work: i64 = req
        .jobs
        .iter()
        .flat_map(|j| j.operations.iter())
        .map(|o| o.duration)
        .sum();
    let lower_bound_estimate = total_work / NUM_MACHINES as i64;

    println!("\n══════════════════════════════════════════════════════════════");
    println!("  Job Shop Scheduling Formation Demo");
    println!(
        "  {} jobs × {} machines   horizon {}   work {} (LB≈{})",
        NUM_JOBS, NUM_MACHINES, horizon, total_work, lower_bound_estimate
    );
    println!("══════════════════════════════════════════════════════════════\n");

    // ── Formation: two suggestors, one engine ─────────────────────────────────

    let mut engine = Engine::new();
    engine.register_suggestor(GreedyJobShopSuggestor);
    engine.register_suggestor(CpSatJobShopSuggestor);

    let mut ctx = ContextState::new();
    ctx.add_input(
        ContextKey::Seeds,
        "jspbench-request:jspbench-demo",
        serde_json::to_string(&req).unwrap(),
    )
    .unwrap();

    let result = engine.run(ctx).await.unwrap();
    let strategies = result.context.get(ContextKey::Strategies);

    let greedy_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("jspbench-plan-greedy:"))
        .and_then(|f| serde_json::from_str::<JobShopPlan>(f.content()).ok());

    let cpsat_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("jspbench-plan-cpsat:"))
        .and_then(|f| serde_json::from_str::<JobShopPlan>(f.content()).ok());

    // ── Report ────────────────────────────────────────────────────────────────

    if let Some(g) = &greedy_plan {
        println!("── GreedyJobShopSuggestor ────────────────────────────────────");
        println!(
            "  Hint:        {}",
            GreedyJobShopSuggestor.complexity_hint().unwrap_or("-")
        );
        println!("  Makespan:    {}", g.makespan);
        println!("  Confidence:  0.55  (SPT heuristic, cannot prove optimality)");
        println!("  Time:        {:.2} ms\n", g.wall_time_seconds * 1000.0);

        // Show machine utilisation from greedy.
        let mut machine_busy = vec![0i64; NUM_MACHINES];
        for op in &g.schedule {
            machine_busy[op.machine_id] += op.end - op.start;
        }
        println!("  Machine utilisation  (makespan = {}):", g.makespan);
        for (m, busy) in machine_busy.iter().enumerate() {
            let pct = if g.makespan > 0 {
                *busy * 100 / g.makespan
            } else {
                0
            };
            println!("    M{m:02}  {busy:4} / {}  ({pct:2}%)", g.makespan);
        }
        println!();
    }

    if let Some(cp) = &cpsat_plan {
        let conf = match cp.status.as_str() {
            "optimal" => 1.0_f64,
            "feasible" => 0.85,
            _ => 0.0,
        };
        let improvement = greedy_plan.as_ref().map_or(0.0, |g| {
            (g.makespan as f64 - cp.makespan as f64) / g.makespan as f64 * 100.0
        });

        println!("── CpSatJobShopSuggestor ─────────────────────────────────────");
        println!(
            "  Hint:        {}",
            CpSatJobShopSuggestor.complexity_hint().unwrap_or("-")
        );
        println!("  Status:      {}", cp.status);
        println!(
            "  Makespan:    {}   ← {:.1}% better than greedy",
            cp.makespan, improvement
        );
        if let Some(lb) = cp.lower_bound {
            println!("  Lower bound: {lb}  (proven optimal)");
        }
        println!("  Confidence:  {conf:.2}");
        println!("  Time:        {:.0} ms\n", cp.wall_time_seconds * 1000.0);

        // Sample schedule (first 5 ops sorted by machine, start).
        println!("  Sample ops (first 8 by machine, start):");
        for op in cp.schedule.iter().take(8) {
            println!(
                "    M{:02}  {:8}  op{}  t={:4}..{:4}  dur={}",
                op.machine_id,
                op.job_name,
                op.op_index,
                op.start,
                op.end,
                op.end - op.start
            );
        }
        println!();
    }

    // ── Formation verdict ─────────────────────────────────────────────────────

    println!("── Formation verdict ─────────────────────────────────────────");
    match (&greedy_plan, &cpsat_plan) {
        (Some(g), Some(cp)) => {
            let winner = if cp.makespan <= g.makespan {
                "CpSatJobShopSuggestor"
            } else {
                "GreedyJobShopSuggestor"
            };
            println!("  Winner:  {winner}");
            println!(
                "  Greedy makespan {} → CP-SAT makespan {}  ({:.1}% reduction)",
                g.makespan,
                cp.makespan,
                (g.makespan as f64 - cp.makespan as f64) / g.makespan as f64 * 100.0
            );
        }
        (Some(_), None) => println!("  Winner:  GreedyJobShopSuggestor (CP-SAT unavailable)"),
        (None, Some(_)) => println!("  Winner:  CpSatJobShopSuggestor"),
        _ => println!("  No plans produced."),
    }

    println!("══════════════════════════════════════════════════════════════\n");
}
