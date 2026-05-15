//! Demonstrates MinCostFlowSuggestor with a small supply-chain network.

use converge_core::{ContextState, Engine};
use converge_pack::{ContextKey, ProposedFact};
use ferrox::network_flow::{
    FlowArc, FlowSolveMode, MinCostFlowPlan, MinCostFlowRequest, MinCostFlowSuggestor, NodeSupply,
};

#[tokio::main]
async fn main() {
    let mut engine = Engine::new();
    engine.register_suggestor(MinCostFlowSuggestor);

    let request = MinCostFlowRequest {
        id: "warehouse-flow".to_string(),
        arcs: vec![
            arc("source-a", 0, 1, 3, 1),
            arc("source-b", 0, 2, 5, 2),
            arc("a-sink", 1, 3, 3, 1),
            arc("b-sink", 2, 3, 5, 1),
        ],
        supplies: vec![
            NodeSupply { node: 0, supply: 5 },
            NodeSupply {
                node: 3,
                supply: -5,
            },
        ],
        mode: FlowSolveMode::BalancedMinCost,
    };

    let mut ctx = ContextState::new();
    ctx.add_proposal(ProposedFact::new(
        ContextKey::Seeds,
        "network-flow-request:warehouse-flow",
        request,
        "network-flow-example",
    ))
    .unwrap();

    let result = engine.run(ctx).await.unwrap();
    let plans: Vec<&MinCostFlowPlan> = result
        .context
        .get(ContextKey::Strategies)
        .iter()
        .filter_map(|f| f.payload::<MinCostFlowPlan>())
        .collect();

    for plan in plans {
        println!("Status: {}", plan.status);
        println!("Solver: {}", plan.solver);
        println!("Cost:   {}", plan.optimal_cost);
        println!("Flow:   {}/{}", plan.fulfilled_flow, plan.expected_flow);
        println!();

        for arc in &plan.arcs {
            println!(
                "{:>8}: {} -> {}  flow {}/{}  unit cost {}",
                arc.name, arc.tail, arc.head, arc.flow, arc.capacity, arc.unit_cost
            );
        }
    }
}

fn arc(name: &str, tail: i32, head: i32, capacity: i64, unit_cost: i64) -> FlowArc {
    FlowArc {
        name: name.to_string(),
        tail,
        head,
        capacity,
        unit_cost,
    }
}
