use async_trait::async_trait;
use converge_pack::{AgentEffect, Context, ContextKey, Suggestor};
use ferrox_ortools_sys::MinCostFlowStatus;
use ferrox_ortools_sys::safe::SimpleMinCostFlow;
use tracing::warn;

use crate::provenance::{FERROX_PROVENANCE, suggestor_span};

use super::problem::{FlowArc, FlowArcPlan, FlowSolveMode, MinCostFlowPlan, MinCostFlowRequest};

const REQUEST_PREFIX: &str = "network-flow-request:";
const PLAN_PREFIX: &str = "network-flow-plan-ortools:";

pub struct MinCostFlowSuggestor;

#[async_trait]
impl Suggestor for MinCostFlowSuggestor {
    fn name(&self) -> &'static str {
        "MinCostFlowSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn complexity_hint(&self) -> Option<&'static str> {
        Some("OR-Tools SimpleMinCostFlow; min-cost circulation/network flow")
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|f| f.id().starts_with(REQUEST_PREFIX) && !plan_exists(ctx, request_id(f.id())))
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
            if plan_exists(ctx, rid) {
                continue;
            }

            match serde_json::from_str::<MinCostFlowRequest>(fact.content()) {
                Ok(req) => {
                    let plan = solve_min_cost_flow(&req);
                    let confidence = confidence(&plan);
                    proposals.push(
                        FERROX_PROVENANCE
                            .proposed_fact(
                                ContextKey::Strategies,
                                format!("{PLAN_PREFIX}{}", plan.request_id),
                                serde_json::to_string(&plan).unwrap_or_default(),
                            )
                            .with_confidence(confidence),
                    );
                }
                Err(e) => {
                    warn!(id = %fact.id(), error = %e, "malformed network-flow-request");
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

fn plan_exists(ctx: &dyn Context, request_id: &str) -> bool {
    let plan_id = format!("{PLAN_PREFIX}{request_id}");
    ctx.get(ContextKey::Strategies)
        .iter()
        .any(|f| f.id() == plan_id.as_str())
}

pub fn solve_min_cost_flow(req: &MinCostFlowRequest) -> MinCostFlowPlan {
    if let Some(status) = validate_request(req) {
        return empty_plan(req, status);
    }

    let expected_flow = expected_flow(req);
    if req.mode == FlowSolveMode::BalancedMinCost && total_supply(req) != 0 {
        return empty_plan(req, "unbalanced");
    }

    let reserve_nodes = reserve_nodes(req);
    let reserve_arcs = i32::try_from(req.arcs.len()).unwrap_or(i32::MAX);
    let mut flow = SimpleMinCostFlow::new(reserve_nodes, reserve_arcs);
    let mut arc_ids = Vec::with_capacity(req.arcs.len());

    for arc in &req.arcs {
        arc_ids.push(flow.add_arc_with_capacity_and_unit_cost(
            arc.tail,
            arc.head,
            arc.capacity,
            arc.unit_cost,
        ));
    }

    for supply in &req.supplies {
        flow.set_node_supply(supply.node, supply.supply);
    }

    let status = match req.mode {
        FlowSolveMode::BalancedMinCost => flow.solve(),
        FlowSolveMode::MaxFlowMinCost => flow.solve_max_flow_with_min_cost(),
    };

    let status_label = min_cost_status_label(status);
    let success = status.is_success();
    let fulfilled_flow = if success { flow.maximum_flow() } else { 0 };
    let fulfillment_ratio = flow_ratio(fulfilled_flow, expected_flow);

    let arcs = req
        .arcs
        .iter()
        .zip(arc_ids)
        .map(|(arc, arc_id)| FlowArcPlan {
            name: arc.name.clone(),
            tail: arc.tail,
            head: arc.head,
            capacity: arc.capacity,
            unit_cost: arc.unit_cost,
            flow: if success { flow.flow(arc_id) } else { 0 },
        })
        .collect();

    MinCostFlowPlan {
        request_id: req.id.clone(),
        status: status_label.to_string(),
        mode: req.mode,
        arcs,
        optimal_cost: if success { flow.optimal_cost() } else { 0 },
        expected_flow,
        fulfilled_flow,
        fulfillment_ratio,
        solver: "simple-min-cost-flow-v9.15".to_string(),
    }
}

fn validate_request(req: &MinCostFlowRequest) -> Option<&'static str> {
    let invalid_arc = req
        .arcs
        .iter()
        .any(|arc| arc.tail < 0 || arc.head < 0 || arc.capacity < 0);
    let invalid_supply = req.supplies.iter().any(|supply| supply.node < 0);
    if invalid_arc || invalid_supply {
        Some("invalid")
    } else {
        None
    }
}

fn empty_plan(req: &MinCostFlowRequest, status: &'static str) -> MinCostFlowPlan {
    let expected_flow = expected_flow(req);
    MinCostFlowPlan {
        request_id: req.id.clone(),
        status: status.to_string(),
        mode: req.mode,
        arcs: req.arcs.iter().map(empty_arc_plan).collect(),
        optimal_cost: 0,
        expected_flow,
        fulfilled_flow: 0,
        fulfillment_ratio: flow_ratio(0, expected_flow),
        solver: "simple-min-cost-flow-v9.15".to_string(),
    }
}

fn empty_arc_plan(arc: &FlowArc) -> FlowArcPlan {
    FlowArcPlan {
        name: arc.name.clone(),
        tail: arc.tail,
        head: arc.head,
        capacity: arc.capacity,
        unit_cost: arc.unit_cost,
        flow: 0,
    }
}

fn min_cost_status_label(status: MinCostFlowStatus) -> &'static str {
    match status {
        MinCostFlowStatus::NotSolved => "not_solved",
        MinCostFlowStatus::Optimal => "optimal",
        MinCostFlowStatus::Feasible => "feasible",
        MinCostFlowStatus::Infeasible => "infeasible",
        MinCostFlowStatus::Unbalanced => "unbalanced",
        MinCostFlowStatus::BadResult => "bad_result",
        MinCostFlowStatus::BadCostRange => "bad_cost_range",
        MinCostFlowStatus::BadCapacityRange => "bad_capacity_range",
        MinCostFlowStatus::Error => "error",
    }
}

fn total_supply(req: &MinCostFlowRequest) -> i64 {
    req.supplies.iter().map(|supply| supply.supply).sum()
}

fn expected_flow(req: &MinCostFlowRequest) -> i64 {
    req.supplies
        .iter()
        .filter(|supply| supply.supply > 0)
        .map(|supply| supply.supply)
        .sum()
}

fn reserve_nodes(req: &MinCostFlowRequest) -> i32 {
    req.arcs
        .iter()
        .flat_map(|arc| [arc.tail, arc.head])
        .chain(req.supplies.iter().map(|supply| supply.node))
        .max()
        .map_or(0, |node| node.saturating_add(1))
}

#[allow(clippy::cast_precision_loss)]
fn flow_ratio(fulfilled_flow: i64, expected_flow: i64) -> f64 {
    if expected_flow <= 0 {
        1.0
    } else {
        ((fulfilled_flow as f64) / (expected_flow as f64)).clamp(0.0, 1.0)
    }
}

fn confidence(plan: &MinCostFlowPlan) -> f64 {
    match (plan.status.as_str(), plan.mode) {
        ("optimal", FlowSolveMode::BalancedMinCost) => 1.0,
        ("optimal", FlowSolveMode::MaxFlowMinCost) => plan.fulfillment_ratio,
        ("feasible", _) => plan.fulfillment_ratio * 0.7,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_flow::problem::{FlowArc, NodeSupply};
    use crate::test_support::MockContext;
    use proptest::prelude::*;

    fn arc(name: &str, tail: i32, head: i32, capacity: i64, unit_cost: i64) -> FlowArc {
        FlowArc {
            name: name.into(),
            tail,
            head,
            capacity,
            unit_cost,
        }
    }

    fn supply(node: i32, supply: i64) -> NodeSupply {
        NodeSupply { node, supply }
    }

    fn balanced_request(id: &str) -> MinCostFlowRequest {
        MinCostFlowRequest {
            id: id.into(),
            arcs: vec![
                arc("s-a", 0, 1, 3, 1),
                arc("s-b", 0, 2, 5, 2),
                arc("a-t", 1, 3, 3, 1),
                arc("b-t", 2, 3, 5, 1),
            ],
            supplies: vec![supply(0, 5), supply(3, -5)],
            mode: FlowSolveMode::BalancedMinCost,
        }
    }

    #[test]
    fn solves_balanced_min_cost_flow() {
        let plan = solve_min_cost_flow(&balanced_request("balanced"));
        assert_eq!(plan.status, "optimal");
        assert_eq!(plan.optimal_cost, 12);
        assert_eq!(plan.expected_flow, 5);
        assert_eq!(plan.fulfilled_flow, 5);
        assert!((plan.fulfillment_ratio - 1.0).abs() < f64::EPSILON);
        let flows: Vec<_> = plan.arcs.iter().map(|arc| arc.flow).collect();
        assert_eq!(flows, vec![3, 2, 3, 2]);
        assert_eq!(plan.solver, "simple-min-cost-flow-v9.15");
    }

    #[test]
    fn balanced_mode_rejects_unbalanced_supply() {
        let mut req = balanced_request("unbalanced");
        req.supplies = vec![supply(0, 5), supply(3, -4)];
        let plan = solve_min_cost_flow(&req);
        assert_eq!(plan.status, "unbalanced");
        assert_eq!(plan.fulfilled_flow, 0);
        assert!(plan.arcs.iter().all(|arc| arc.flow == 0));
    }

    #[test]
    fn rejects_invalid_arc_capacity() {
        let req = MinCostFlowRequest {
            id: "invalid".into(),
            arcs: vec![arc("bad", 0, 1, -1, 1)],
            supplies: vec![supply(0, 1), supply(1, -1)],
            mode: FlowSolveMode::BalancedMinCost,
        };
        let plan = solve_min_cost_flow(&req);
        assert_eq!(plan.status, "invalid");
        assert_eq!(plan.optimal_cost, 0);
    }

    #[test]
    fn max_flow_mode_reports_partial_fulfillment() {
        let req = MinCostFlowRequest {
            id: "partial".into(),
            arcs: vec![arc("limited", 0, 1, 3, 7)],
            supplies: vec![supply(0, 5), supply(1, -5)],
            mode: FlowSolveMode::MaxFlowMinCost,
        };
        let plan = solve_min_cost_flow(&req);
        assert_eq!(plan.status, "optimal");
        assert_eq!(plan.expected_flow, 5);
        assert_eq!(plan.fulfilled_flow, 3);
        assert!((plan.fulfillment_ratio - 0.6).abs() < f64::EPSILON);
        assert_eq!(plan.optimal_cost, 21);
        assert_eq!(plan.arcs[0].flow, 3);
    }

    proptest! {
        #[test]
        fn single_arc_cost_scales_with_demand(demand in 0_i64..=20, extra_capacity in 0_i64..=20, unit_cost in 0_i64..=20) {
            let req = MinCostFlowRequest {
                id: format!("single-{demand}"),
                arcs: vec![arc("a", 0, 1, demand + extra_capacity, unit_cost)],
                supplies: vec![supply(0, demand), supply(1, -demand)],
                mode: FlowSolveMode::BalancedMinCost,
            };
            let plan = solve_min_cost_flow(&req);
            assert_eq!(plan.status, "optimal");
            assert_eq!(plan.expected_flow, demand);
            assert_eq!(plan.fulfilled_flow, demand);
            assert_eq!(plan.optimal_cost, demand * unit_cost);
            assert_eq!(plan.arcs[0].flow, demand);
        }
    }

    #[tokio::test]
    async fn suggestor_emits_proposal() {
        let req = balanced_request("s");
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty().with_seed("network-flow-request:s", &body);
        let s = MinCostFlowSuggestor;
        assert_eq!(s.name(), "MinCostFlowSuggestor");
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
        assert!(s.complexity_hint().is_some());
        assert!(s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 1);
    }

    #[tokio::test]
    async fn suggestor_skips_when_plan_present() {
        let req = balanced_request("s2");
        let body = serde_json::to_string(&req).unwrap();
        let ctx = MockContext::empty()
            .with_seed("network-flow-request:s2", &body)
            .with_strategy("network-flow-plan-ortools:s2", "{}");
        let s = MinCostFlowSuggestor;
        assert!(!s.accepts(&ctx));
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }

    #[tokio::test]
    async fn suggestor_handles_malformed_seed() {
        let ctx = MockContext::empty().with_seed("network-flow-request:bad", "not json");
        let s = MinCostFlowSuggestor;
        let eff = s.execute(&ctx).await;
        assert_eq!(eff.proposals().len(), 0);
    }
}
