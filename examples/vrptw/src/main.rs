//! Vehicle Routing with Time Windows — Formation demo.
//!
//! Registers two competing suggestors in a single Converge Engine:
//!
//! - `NearestNeighborSuggestor` — O(n²) greedy heuristic, answers in < 1 ms
//! - `CpSatVrptwSuggestor`      — OR-Tools CP-SAT AddCircuit, proves optimality
//!
//! Both accept the same `vrptw-request:` seed.  Each emits a solver-prefixed
//! plan to `ContextKey::Strategies`.  The demo uses a Solomon-style 20-customer
//! instance and compares both plans.
//!
//! Run with:
//!
//! ```bash
//! just example-vrptw
//! ```

use converge_core::{ContextState, Engine};
use converge_pack::{ContextKey, Suggestor};
use ferrox::vrptw::{
    CpSatVrptwSuggestor, Customer, Depot, NearestNeighborSuggestor, VrptwPlan, VrptwRequest,
};

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

// ── Solomon-style instance generator ─────────────────────────────────────────
//
// Depot at (50, 50), horizon [0, 480] (8 hours in minutes).
// Customers placed in a 100×100 grid with tight random time windows.

fn build_request() -> VrptwRequest {
    let mut rng = 13u64;

    let num_customers: usize = 20;
    let horizon: i64 = 480;

    let depot = Depot {
        x: 50.0,
        y: 50.0,
        ready_time: 0,
        due_time: horizon,
    };

    let customers: Vec<Customer> = (1..=num_customers)
        .map(|id| {
            let x = rng_range(&mut rng, 0, 100) as f64;
            let y = rng_range(&mut rng, 0, 100) as f64;

            // Travel time from depot (Euclidean, integer ceiling).
            let dx = x - depot.x;
            let dy = y - depot.y;
            #[allow(clippy::cast_possible_truncation)]
            let travel_from_depot = (dx * dx + dy * dy).sqrt().ceil() as i64;

            // Window opens at ready time between [travel, horizon/2].
            let open_lo = travel_from_depot;
            let open_hi = (horizon / 2).max(open_lo + 1);
            let window_open = rng_range(&mut rng, open_lo as u64, open_hi as u64) as i64;

            let service_time = rng_range(&mut rng, 10, 30) as i64;

            // Window closes: [open + service + travel_back, open + 3*(service+travel_back)].
            let min_stay = service_time + travel_from_depot;
            let close_lo = (window_open + min_stay).min(horizon);
            let close_hi = (window_open + min_stay * 3).min(horizon);
            let window_close = if close_hi > close_lo {
                rng_range(&mut rng, close_lo as u64, close_hi as u64) as i64
            } else {
                close_lo
            };

            Customer {
                id,
                name: format!("C{id:02}"),
                x,
                y,
                window_open,
                window_close,
                service_time,
            }
        })
        .collect();

    VrptwRequest {
        id: "vrptw-demo".to_string(),
        depot,
        customers,
        time_limit_seconds: 30.0,
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let req = build_request();
    let n = req.customers.len();

    println!("\n══════════════════════════════════════════════════════════════");
    println!("  VRPTW Formation Demo");
    println!(
        "  {} customers   depot ({:.0},{:.0})   horizon {}",
        n, req.depot.x, req.depot.y, req.depot.due_time
    );
    println!("══════════════════════════════════════════════════════════════\n");

    // Print customer table.
    println!("  Customer table:");
    println!(
        "  {:>4}  {:>6}  {:>6}  {:>6}  {:>6}  {:>4}  {:>5}",
        "ID", "X", "Y", "Open", "Close", "Svc", "TW"
    );
    for c in &req.customers {
        println!(
            "  {:>4}  {:>6.1}  {:>6.1}  {:>6}  {:>6}  {:>4}  {:>5}",
            c.name,
            c.x,
            c.y,
            c.window_open,
            c.window_close,
            c.service_time,
            c.window_close - c.window_open
        );
    }
    println!();

    // ── Formation ─────────────────────────────────────────────────────────────

    let mut engine = Engine::new();
    engine.register_suggestor(NearestNeighborSuggestor);
    engine.register_suggestor(CpSatVrptwSuggestor);

    let mut ctx = ContextState::new();
    ctx.add_input(
        ContextKey::Seeds,
        "vrptw-request:vrptw-demo",
        serde_json::to_string(&req).unwrap(),
    )
    .unwrap();

    let result = engine.run(ctx).await.unwrap();
    let strategies = result.context.get(ContextKey::Strategies);

    let nn_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("vrptw-plan-greedy:"))
        .and_then(|f| serde_json::from_str::<VrptwPlan>(f.content()).ok());

    let cpsat_plan = strategies
        .iter()
        .find(|f| f.id().starts_with("vrptw-plan-cpsat:"))
        .and_then(|f| serde_json::from_str::<VrptwPlan>(f.content()).ok());

    // ── Report ────────────────────────────────────────────────────────────────

    if let Some(g) = &nn_plan {
        let conf = g.visit_ratio() * 0.60;
        println!("── NearestNeighborSuggestor ──────────────────────────────────");
        println!(
            "  Hint:       {}",
            NearestNeighborSuggestor.complexity_hint().unwrap_or("-")
        );
        println!(
            "  Visited:    {} / {}  ({:.1}%)",
            g.customers_visited,
            g.customers_total,
            g.visit_ratio() * 100.0
        );
        println!("  Distance:   {:.1}", g.total_distance);
        println!("  Return:     t={}", g.return_time);
        println!("  Confidence: {:.2}", conf);
        println!("  Time:       {:.2} ms\n", g.wall_time_seconds * 1000.0);

        println!("  Route:");
        println!("    DEPOT");
        for stop in &g.route {
            println!(
                "    → {:4}  arrive={:4}  depart={:4}",
                stop.customer_name, stop.arrival, stop.departure
            );
        }
        println!("    → DEPOT  t={}\n", g.return_time);
    }

    if let Some(cp) = &cpsat_plan {
        let conf = match cp.status.as_str() {
            "optimal" => cp.visit_ratio(),
            "feasible" => cp.visit_ratio() * 0.85,
            _ => 0.0,
        };
        let extra = cp.customers_visited as i64
            - nn_plan.as_ref().map_or(0, |g| g.customers_visited as i64);

        println!("── CpSatVrptwSuggestor ───────────────────────────────────────");
        println!(
            "  Hint:       {}",
            CpSatVrptwSuggestor.complexity_hint().unwrap_or("-")
        );
        println!("  Status:     {}", cp.status);
        println!(
            "  Visited:    {} / {}  ({:.1}%)  ← +{} vs greedy",
            cp.customers_visited,
            cp.customers_total,
            cp.visit_ratio() * 100.0,
            extra
        );
        println!("  Distance:   {:.1}", cp.total_distance);
        println!("  Return:     t={}", cp.return_time);
        println!("  Confidence: {:.2}", conf);
        println!("  Time:       {:.0} ms\n", cp.wall_time_seconds * 1000.0);

        println!("  Route:");
        println!("    DEPOT");
        for stop in &cp.route {
            println!(
                "    → {:4}  arrive={:4}  depart={:4}",
                stop.customer_name, stop.arrival, stop.departure
            );
        }
        println!("    → DEPOT  t={}\n", cp.return_time);
    }

    // ── Formation verdict ─────────────────────────────────────────────────────

    println!("── Formation verdict ─────────────────────────────────────────");
    match (&nn_plan, &cpsat_plan) {
        (Some(g), Some(cp)) => {
            let winner = if cp.customers_visited >= g.customers_visited {
                "CpSatVrptwSuggestor"
            } else {
                "NearestNeighborSuggestor"
            };
            println!("  Winner:  {winner}");
            println!(
                "  Visited: {} (greedy) → {} (CP-SAT)  (+{} customers, {:.1}% improvement)",
                g.customers_visited,
                cp.customers_visited,
                cp.customers_visited as i64 - g.customers_visited as i64,
                (cp.customers_visited as f64 - g.customers_visited as f64)
                    / (g.customers_visited as f64).max(1.0)
                    * 100.0
            );
        }
        (Some(_), None) => println!("  Winner:  NearestNeighborSuggestor (CP-SAT unavailable)"),
        (None, Some(_)) => println!("  Winner:  CpSatVrptwSuggestor"),
        _ => println!("  No plans produced."),
    }

    println!("══════════════════════════════════════════════════════════════\n");
}
