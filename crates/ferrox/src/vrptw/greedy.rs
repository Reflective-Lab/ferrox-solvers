use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, Suggestor};
use std::time::Instant;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};
use crate::solver_identity::non_native_solver_identity;

use super::problem::{RouteStop, VrptwPlan, VrptwRequest};

pub(super) const REQUEST_PREFIX: &str = "vrptw-request:";
const PLAN_PREFIX: &str = "vrptw-plan-greedy:";

/// Routes a vehicle via Nearest-Neighbour heuristic with time-window feasibility check.
///
/// **Algorithm:** O(n²) — at each stop, visit the nearest unvisited customer
/// whose time window is still reachable.  Backtracks to depot when no further
/// customer is reachable.
///
/// **Confidence:** capped at 0.60 — nearest-neighbour can be 20-25% worse
/// than optimal on real instances.
pub struct NearestNeighborSuggestor;

#[async_trait]
impl Suggestor for NearestNeighborSuggestor {
    fn name(&self) -> &'static str {
        "NearestNeighborSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("O(n²) — nearest-neighbour with TW feasibility; deterministic, sub-ms for n ≤ 10 000")
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
                    let plan = solve_nn(req);
                    let confidence = (plan.visit_ratio() * 0.60).min(0.60);
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

/// Nearest-neighbour TSP with time-window feasibility.
#[allow(clippy::similar_names)]
pub fn solve_nn(req: &VrptwRequest) -> VrptwPlan {
    let t0 = Instant::now();
    let n = req.customers.len();
    let mut visited = vec![false; n];
    let mut route: Vec<RouteStop> = Vec::new();

    let mut cur_x = req.depot.x;
    let mut cur_y = req.depot.y;
    let mut cur_t = req.depot.ready_time;
    let mut total_dist = 0.0_f64;

    loop {
        // Find nearest reachable unvisited customer.
        let best = (0..n)
            .filter(|&i| !visited[i])
            .filter_map(|i| {
                let c = &req.customers[i];
                let dx = cur_x - c.x;
                let dy = cur_y - c.y;
                let dist = (dx * dx + dy * dy).sqrt();
                #[allow(clippy::cast_possible_truncation)]
                let travel = dist.ceil() as i64;
                let arrival = cur_t + travel;
                // Must arrive before window closes, and vehicle returns in time.
                let depart = arrival.max(c.window_open) + c.service_time;
                let depot_dx = c.x - req.depot.x;
                let depot_dy = c.y - req.depot.y;
                #[allow(clippy::cast_possible_truncation)]
                let return_dist =
                    ((depot_dx * depot_dx + depot_dy * depot_dy).sqrt().ceil()) as i64;
                if arrival <= c.window_close && depart + return_dist <= req.depot.due_time {
                    Some((i, dist, travel, arrival, depart))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        match best {
            Some((i, dist, travel, arrival, depart)) => {
                let c = &req.customers[i];
                visited[i] = true;
                total_dist += dist;
                cur_x = c.x;
                cur_y = c.y;
                cur_t = depart;
                let _ = travel;
                let _ = arrival;
                route.push(RouteStop {
                    customer_id: c.id,
                    customer_name: c.name.clone(),
                    arrival: depart - c.service_time,
                    departure: depart,
                });
            }
            None => break,
        }
    }

    // Return to depot.
    let depot_dx = cur_x - req.depot.x;
    let depot_dy = cur_y - req.depot.y;
    let return_dist = (depot_dx * depot_dx + depot_dy * depot_dy).sqrt();
    total_dist += return_dist;
    #[allow(clippy::cast_possible_truncation)]
    let return_time = cur_t + return_dist.ceil() as i64;

    VrptwPlan {
        request_id: req.id.clone(),
        customers_visited: route.len(),
        customers_total: n,
        route,
        total_distance: total_dist,
        return_time,
        solver: "nearest-neighbour".to_string(),
        solver_identity: non_native_solver_identity(
            "nearest-neighbour",
            "algorithm=nearest_neighbour",
        ),
        status: "feasible".to_string(),
        wall_time_seconds: t0.elapsed().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockContext;
    use crate::vrptw::problem::{Customer, Depot};
    use converge_pack::Suggestor;
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

    fn depot(due: i64) -> Depot {
        Depot {
            x: 0.0,
            y: 0.0,
            ready_time: 0,
            due_time: due,
        }
    }

    fn req(customers: Vec<Customer>, due: i64) -> VrptwRequest {
        VrptwRequest {
            id: "v1".into(),
            depot: depot(due),
            customers,
            time_limit_seconds: 1.0,
        }
    }

    #[test]
    fn empty_customers_returns_to_depot() {
        let r = req(vec![], 1000);
        let plan = solve_nn(&r);
        assert_eq!(plan.customers_visited, 0);
        assert_eq!(plan.customers_total, 0);
        assert_eq!(plan.return_time, 0);
        assert!((plan.visit_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn travel_metric_is_euclidean() {
        let a = customer(1, 3.0, 4.0, 0, 100);
        let b = customer(2, 0.0, 0.0, 0, 100);
        assert!((a.travel_to(&b) - 5.0).abs() < 1e-9);
        let d = depot(100);
        assert!((d.travel_to_customer(&a) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn visits_reachable_customer() {
        let r = req(vec![customer(1, 1.0, 0.0, 0, 100)], 1000);
        let plan = solve_nn(&r);
        assert_eq!(plan.customers_visited, 1);
        assert!((plan.visit_ratio() - 1.0).abs() < f64::EPSILON);
        assert!(plan.return_time > 0);
    }

    #[test]
    fn skips_unreachable_due_to_window_close() {
        let r = req(vec![customer(1, 1000.0, 0.0, 0, 1)], 100_000);
        let plan = solve_nn(&r);
        assert_eq!(plan.customers_visited, 0);
    }

    #[test]
    fn skips_when_return_exceeds_due_time() {
        let r = req(vec![customer(1, 100.0, 0.0, 0, 1000)], 50);
        let plan = solve_nn(&r);
        assert_eq!(plan.customers_visited, 0);
    }

    #[test]
    fn picks_nearest_first() {
        let r = req(
            vec![
                customer(1, 10.0, 0.0, 0, 100),
                customer(2, 1.0, 0.0, 0, 100),
            ],
            1_000,
        );
        let plan = solve_nn(&r);
        assert_eq!(plan.route[0].customer_id, 2);
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let r = req(vec![customer(1, 1.0, 0.0, 0, 100)], 1000);
        let ctx = MockContext::empty().with_seed(&format!("{REQUEST_PREFIX}v1"), r);
        let s = NearestNeighborSuggestor;
        assert_eq!(s.name(), "NearestNeighborSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let r = req(vec![customer(1, 1.0, 0.0, 0, 100)], 1000);
        let ctx = MockContext::empty()
            .with_seed(&format!("{REQUEST_PREFIX}v1"), r)
            .with_strategy("vrptw-plan-greedy:v1", TextPayload::new("existing"));
        let s = NearestNeighborSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_skips_malformed_seed() {
        let ctx = MockContext::empty().with_seed(
            &format!("{REQUEST_PREFIX}bad"),
            TextPayload::new("not a vrptw request"),
        );
        let s = NearestNeighborSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }
}
