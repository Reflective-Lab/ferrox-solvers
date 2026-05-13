//! OR-Tools `SimpleMinCostFlow` Suggestor.
//!
//! Seeds use `network-flow-request:*`; plans are emitted as
//! `network-flow-plan-ortools:*`.

pub mod problem;
pub mod suggestor;

pub use problem::{
    FlowArc, FlowArcPlan, FlowSolveMode, MinCostFlowPlan, MinCostFlowRequest, NodeSupply,
};
pub use suggestor::{MinCostFlowSuggestor, solve_min_cost_flow};
