use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, ProvenanceSource, Suggestor,
};
use ferrox_ortools_sys::OrtoolsStatus;
use ferrox_ortools_sys::safe::CpModel;
use std::time::Instant;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};
use crate::solver_identity::cp_sat_solver_identity;

use super::greedy::REQUEST_PREFIX;
use super::problem::{Customer, RouteStop, VrptwPlan, VrptwRequest};

const PLAN_PREFIX: &str = "vrptw-plan-cpsat:";
/// Distance scale factor: 1 unit = 0.01 distance units.
const SCALE: i64 = 100;

/// Solves TSPTW to optimality using CP-SAT `AddCircuit` + time-window variables.
///
/// **Model:**
/// Nodes 0 = depot, 1..=N = customers.
///
/// ```text
/// x_ij ∈ {0,1} — arc i→j is used in the route
/// x_ii ∈ {0,1} — self-loop: customer i is skipped (optional visits)
///
/// AddCircuit({all arcs including self-loops})
///
/// t_i ∈ [window_open_i, window_close_i]  — arrival time at i
///
/// For each arc (i,j), i≠j:
///   t_j ≥ t_i + service_i + travel_ij − M·(1 − x_ij)
///   → LinearGe: t_j − t_i − M·x_ij ≥ service_i + travel_ij − M
///
/// Objective: maximise customers visited = minimise Σ x_ii
/// ```
///
/// **Confidence:**
/// - `optimal` → `visit_ratio` (resource-limited if < 1.0, otherwise proven max throughput)
/// - `feasible` → `visit_ratio` × 0.85
pub struct CpSatVrptwSuggestor;

#[async_trait]
impl Suggestor for CpSatVrptwSuggestor {
    fn name(&self) -> &'static str {
        "CpSatVrptwSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some(concat!(
            "NP-hard; CP-SAT AddCircuit + time-window propagation; ",
            "proves optimality for n ≤ 25 customers within 30 s on 10-core hardware"
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

            match fact.require_payload::<VrptwRequest>() {
                Ok(req) => {
                    let plan = solve_cpsat_vrptw(req);
                    let confidence = match plan.status.as_str() {
                        "optimal" => plan.visit_ratio(),
                        "feasible" => plan.visit_ratio() * 0.85,
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
                    warn!(id = %fact.id(), error = %e, "unexpected vrptw-request payload");
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

#[allow(
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
pub fn solve_cpsat_vrptw(req: &VrptwRequest) -> VrptwPlan {
    let t0 = Instant::now();
    if let Err(reason) = validate_vrptw_request(req) {
        warn!(request_id = %req.id, reason = %reason, "invalid vrptw-request");
        return empty_plan(req, "invalid", t0.elapsed().as_secs_f64());
    }

    let n = req.customers.len();
    // Node 0 = depot, 1..=n = customers.
    let num_nodes = n + 1;

    // Scaled integer travel times.
    #[allow(clippy::cast_possible_truncation)]
    let travel = |from_x: f64, from_y: f64, to_x: f64, to_y: f64| -> i64 {
        let dx = from_x - to_x;
        let dy = from_y - to_y;
        ((dx * dx + dy * dy).sqrt() * SCALE as f64).ceil() as i64
    };

    let depot_travel_to = |c: &Customer| -> i64 { travel(req.depot.x, req.depot.y, c.x, c.y) };
    let customer_travel = |a: &Customer, b: &Customer| -> i64 { travel(a.x, a.y, b.x, b.y) };

    let horizon = req.depot.due_time * SCALE;
    let big_m = horizon + 1;

    let mut model = CpModel::new();

    // ── Arc variables ─────────────────────────────────────────────────────────

    // arc_lit[i][j] = bool literal for arc from node i to node j.
    let mut arc_lit: Vec<Vec<i32>> = vec![vec![-1; num_nodes]; num_nodes];

    for i in 0..num_nodes {
        for j in 0..num_nodes {
            if i == j && i == 0 {
                continue; // no depot self-loop
            }
            arc_lit[i][j] = model.new_bool_var(&format!("x_{i}_{j}"));
        }
    }

    // ── Time variables ────────────────────────────────────────────────────────
    // Scaled by SCALE; depot time vars for start and return.

    let depot_start_t = model.new_int_var(
        req.depot.ready_time * SCALE,
        req.depot.ready_time * SCALE,
        "t_depot_start",
    );
    let depot_end_t = model.new_int_var(0, req.depot.due_time * SCALE, "t_depot_end");

    // t[i] = scaled arrival time at customer i (1-indexed).
    let cust_t: Vec<i32> = req
        .customers
        .iter()
        .map(|c| {
            model.new_int_var(
                c.window_open * SCALE,
                c.window_close * SCALE,
                &format!("t_{}", c.id),
            )
        })
        .collect();

    // Helper: time var for node index.
    let t_node = |node: usize| -> i32 {
        if node == 0 {
            depot_start_t
        } else {
            cust_t[node - 1]
        }
    };

    // ── AddCircuit ────────────────────────────────────────────────────────────

    let mut tails: Vec<i32> = Vec::new();
    let mut heads: Vec<i32> = Vec::new();
    let mut lits: Vec<i32> = Vec::new();

    for i in 0..num_nodes {
        for j in 0..num_nodes {
            let lit = arc_lit[i][j];
            if lit == -1 {
                continue;
            }
            tails.push(i as i32);
            heads.push(j as i32);
            lits.push(lit);
        }
    }

    model.add_circuit(&tails, &heads, &lits);

    // ── Time-consistency constraints ──────────────────────────────────────────
    // For each non-self-loop arc (i→j): if x_ij = 1, t_j ≥ t_i + svc_i + travel_ij
    // Big-M: t_j - t_i - M*x_ij ≥ svc_i + travel_ij - M
    //        → LinearGe: [{t_j, 1}, {t_i, -1}, {x_ij, -M}] ≥ svc_i + travel_ij - M

    for i in 0..num_nodes {
        for j in 0..num_nodes {
            if i == j {
                continue;
            }
            let lit = arc_lit[i][j];
            if lit == -1 {
                continue;
            }

            let (svc_i, travel_ij) = if i == 0 {
                // depot → customer j
                let c = &req.customers[j - 1];
                (0i64, depot_travel_to(c))
            } else if j == 0 {
                // customer i → depot
                let c = &req.customers[i - 1];
                (c.service_time * SCALE, depot_travel_to(c))
            } else {
                // customer i → customer j
                let ci = &req.customers[i - 1];
                let cj = &req.customers[j - 1];
                (ci.service_time * SCALE, customer_travel(ci, cj))
            };

            let rhs = svc_i + travel_ij - big_m;
            let t_j = if j == 0 { depot_end_t } else { t_node(j) };
            let t_i = t_node(i);

            model.add_linear_ge(&[t_j, t_i, lit], &[1, -1, -big_m], rhs);
        }
    }

    // ── Objective: maximise customers visited = minimise Σ self-loop literals ─

    let self_loop_lits: Vec<i32> = (1..=n)
        .map(|i| arc_lit[i][i])
        .filter(|&l| l != -1)
        .collect();

    if !self_loop_lits.is_empty() {
        let coeffs = vec![1i64; self_loop_lits.len()];
        model.minimize(&self_loop_lits, &coeffs);
    }

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

    // ── Extract route from arc literals ───────────────────────────────────────

    let mut route: Vec<RouteStop> = Vec::new();
    let mut total_distance = 0.0_f64;
    let mut cur = 0usize; // start at depot

    for _ in 0..=n {
        // Find the arc leaving cur that is active.
        let next = (0..num_nodes).find(|&j| {
            if j == cur {
                return false;
            }
            let lit = arc_lit[cur][j];
            lit != -1 && solution.value(lit) == 1
        });

        match next {
            None | Some(0) => break, // returned to depot
            Some(j) => {
                let c = &req.customers[j - 1];
                let arrival_scaled = solution.value(cust_t[j - 1]);
                #[allow(clippy::cast_precision_loss)]
                let arrival = arrival_scaled / SCALE;
                let departure = arrival + c.service_time;

                // Accumulate distance.
                let (fx, fy) = if cur == 0 {
                    (req.depot.x, req.depot.y)
                } else {
                    let pc = &req.customers[cur - 1];
                    (pc.x, pc.y)
                };
                let dx = fx - c.x;
                let dy = fy - c.y;
                total_distance += (dx * dx + dy * dy).sqrt();

                route.push(RouteStop {
                    customer_id: c.id,
                    customer_name: c.name.clone(),
                    arrival,
                    departure,
                });
                cur = j;
            }
        }
    }

    // Distance back to depot.
    if let Some(last_stop) = route.last()
        && let Some(c) = req.customers.iter().find(|c| c.id == last_stop.customer_id)
    {
        let dx = c.x - req.depot.x;
        let dy = c.y - req.depot.y;
        total_distance += (dx * dx + dy * dy).sqrt();
    }

    #[allow(clippy::cast_precision_loss)]
    let return_time = solution.value(depot_end_t) / SCALE;

    VrptwPlan {
        request_id: req.id.clone(),
        customers_visited: route.len(),
        customers_total: n,
        route,
        total_distance,
        return_time,
        solver: "cp-sat-v9.15".to_string(),
        solver_identity: vrptw_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds: elapsed,
    }
}

fn validate_vrptw_request(req: &VrptwRequest) -> Result<(), String> {
    if req.id.trim().is_empty() {
        return Err("request id must not be empty".to_string());
    }
    if req.id.contains('\0') {
        return Err("request id contains an interior NUL byte".to_string());
    }
    if !req.time_limit_seconds.is_finite() || req.time_limit_seconds <= 0.0 {
        return Err("time_limit_seconds must be finite and positive".to_string());
    }
    if !req.depot.x.is_finite() || !req.depot.y.is_finite() {
        return Err("depot coordinates must be finite".to_string());
    }
    if req.depot.due_time < 0 {
        return Err("depot due_time must be non-negative".to_string());
    }
    if req.depot.ready_time > req.depot.due_time {
        return Err("depot ready_time exceeds due_time".to_string());
    }
    let horizon = scaled_time(req.depot.due_time, "depot due_time")?;
    horizon
        .checked_add(1)
        .ok_or_else(|| "VRPTW big-M overflows i64".to_string())?;
    scaled_time(req.depot.ready_time, "depot ready_time")?;

    for customer in &req.customers {
        if !customer.x.is_finite() || !customer.y.is_finite() {
            return Err(format!(
                "customer '{}' coordinates must be finite",
                customer.id
            ));
        }
        if customer.window_open > customer.window_close {
            return Err(format!(
                "customer '{}' window_open exceeds window_close",
                customer.id
            ));
        }
        if customer.service_time < 0 {
            return Err(format!(
                "customer '{}' service_time must be non-negative",
                customer.id
            ));
        }
        scaled_time(customer.window_open, "customer window_open")?;
        scaled_time(customer.window_close, "customer window_close")?;
        scaled_time(customer.service_time, "customer service_time")?;
    }

    Ok(())
}

fn scaled_time(value: i64, label: &'static str) -> Result<i64, String> {
    value
        .checked_mul(SCALE)
        .ok_or_else(|| format!("{label} overflows scaled i64 time"))
}

fn empty_plan(req: &VrptwRequest, status: &str, wall_time_seconds: f64) -> VrptwPlan {
    VrptwPlan {
        request_id: req.id.clone(),
        route: Vec::new(),
        customers_total: req.customers.len(),
        customers_visited: 0,
        total_distance: 0.0,
        return_time: 0,
        solver: "cp-sat-v9.15".to_string(),
        solver_identity: vrptw_cpsat_identity(req),
        status: status.to_string(),
        wall_time_seconds,
    }
}

fn vrptw_cpsat_identity(req: &VrptwRequest) -> ExecutionIdentity {
    cp_sat_solver_identity(format!(
        "time_limit_seconds={}; search_workers=hardware_concurrency; objective=maximize_customers_visited; distance_scale={SCALE}",
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
    use crate::test_support::MockContext;
    use crate::vrptw::problem::{Customer, Depot};
    use converge_pack::TextPayload;

    fn customer(id: usize, x: f64, y: f64, open: i64, close: i64) -> Customer {
        Customer {
            id,
            name: format!("c{id}"),
            x,
            y,
            window_open: open,
            window_close: close,
            service_time: 1,
        }
    }

    fn req(customers: Vec<Customer>, due: i64, time_limit: f64) -> VrptwRequest {
        VrptwRequest {
            id: "v".into(),
            depot: Depot {
                x: 0.0,
                y: 0.0,
                ready_time: 0,
                due_time: due,
            },
            customers,
            time_limit_seconds: time_limit,
        }
    }

    #[test]
    fn small_tour_optimal() {
        let r = req(
            vec![
                customer(1, 1.0, 0.0, 0, 50),
                customer(2, 2.0, 0.0, 0, 50),
                customer(3, 3.0, 0.0, 0, 50),
            ],
            200,
            5.0,
        );
        let plan = solve_cpsat_vrptw(&r);
        assert_eq!(plan.status, "optimal");
        assert_eq!(plan.solver_identity.backend, "cp-sat-v9.15");
        assert!(
            plan.solver_identity
                .native_identity
                .as_ref()
                .is_some_and(|native| native.backend.contains("OR-Tools"))
        );
        assert_eq!(plan.customers_visited, 3);
        assert!(plan.return_time > 0);
        assert!(plan.total_distance > 0.0);
    }

    #[test]
    fn skips_unreachable_customer_via_self_loop() {
        // Customer 2 has impossible window — solver must skip via self-loop.
        let r = req(
            vec![
                customer(1, 1.0, 0.0, 0, 50),
                customer(2, 100.0, 100.0, 0, 1), // unreachable
            ],
            200,
            5.0,
        );
        let plan = solve_cpsat_vrptw(&r);
        assert!(matches!(plan.status.as_str(), "optimal" | "feasible"));
        // At least one customer skipped; visit count <= 1.
        assert!(plan.customers_visited <= 1);
    }

    #[test]
    fn rejects_invalid_time_window_without_panic() {
        let r = req(vec![customer(1, 1.0, 0.0, 50, 10)], 200, 5.0);
        let plan = solve_cpsat_vrptw(&r);
        assert_eq!(plan.status, "invalid");
        assert!(plan.route.is_empty());
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let r = req(vec![customer(1, 1.0, 0.0, 0, 50)], 200, 1.0);
        let ctx = MockContext::empty().with_seed("vrptw-request:v", r);
        let s = CpSatVrptwSuggestor;
        assert_eq!(s.name(), "CpSatVrptwSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let r = req(vec![customer(1, 1.0, 0.0, 0, 50)], 200, 1.0);
        let ctx = MockContext::empty()
            .with_seed("vrptw-request:v", r)
            .with_strategy("vrptw-plan-cpsat:v", TextPayload::new("existing"));
        let s = CpSatVrptwSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty()
            .with_seed("vrptw-request:bad", TextPayload::new("not a vrptw request"));
        let s = CpSatVrptwSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    /// Stress: 40-customer Solomon-style RC2-class instance with tight
    /// individual time windows but a long depot horizon. CP-SAT's AddCircuit
    /// with Big-M time propagation scales poorly past ~30 customers — this
    /// instance reliably consumes the 30 s budget.
    #[test]
    fn stress_30s_vrptw_40_customers() {
        let n = 40;
        let mut state: u64 = 0xCAFE_BABE_FACE_DEAD;
        let next = |s: &mut u64| -> u64 {
            *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *s
        };
        let customers: Vec<_> = (1..=n)
            .map(|i| {
                let x = f64::from(((next(&mut state) >> 33) & 0x7F) as u32);
                let y = f64::from(((next(&mut state) >> 33) & 0x7F) as u32);
                // Random tight time window of width ~120 inside [0, 800].
                let open = ((next(&mut state) >> 33) & 0x1FF) as i64;
                let close = open + 120 + ((next(&mut state) >> 33) & 0x3F) as i64;
                customer(i, x, y, open, close)
            })
            .collect();
        let r = req(customers, 1_500, 30.0);
        let started = std::time::Instant::now();
        let plan = solve_cpsat_vrptw(&r);
        let elapsed = started.elapsed().as_secs_f64();
        assert!(
            matches!(plan.status.as_str(), "optimal" | "feasible"),
            "stress should yield a feasible VRPTW plan, got {} in {elapsed:.1}s",
            plan.status
        );
        assert_eq!(plan.customers_total, n);
        assert!(plan.customers_visited > 0);
    }
}
