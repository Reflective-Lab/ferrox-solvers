use serde::{Deserialize, Serialize};

use converge_pack::{ExecutionIdentity, FactPayload};

use crate::domain_types::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowSolveStatus {
    NotSolved,
    Optimal,
    Feasible,
    Infeasible,
    Unbalanced,
    BadResult,
    BadCostRange,
    BadCapacityRange,
    Error,
    Invalid,
}

impl FlowSolveStatus {
    pub fn is_successful(self) -> bool {
        matches!(self, Self::Optimal | Self::Feasible)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FlowSolveMode {
    /// Requires total supply and demand to balance and all demand to be served.
    #[default]
    BalancedMinCost,
    /// Sends as much flow as possible, then minimizes cost for that flow value.
    MaxFlowMinCost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowArc {
    pub name: String,
    pub tail: NodeId,
    pub head: NodeId,
    pub capacity: i64,
    pub unit_cost: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSupply {
    pub node: NodeId,
    pub supply: i64,
}

/// Seeded into `ContextKey::Seeds` with id prefix `"network-flow-request:"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinCostFlowRequest {
    pub id: String,
    pub arcs: Vec<FlowArc>,
    pub supplies: Vec<NodeSupply>,
    #[serde(default)]
    pub mode: FlowSolveMode,
}

impl FactPayload for MinCostFlowRequest {
    const FAMILY: &'static str = "ferrox.network_flow.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowArcPlan {
    pub name: String,
    pub tail: NodeId,
    pub head: NodeId,
    pub capacity: i64,
    pub unit_cost: i64,
    pub flow: i64,
}

/// Written to `ContextKey::Strategies` with id prefix `"network-flow-plan-ortools:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinCostFlowPlan {
    pub request_id: String,
    pub status: FlowSolveStatus,
    pub mode: FlowSolveMode,
    pub arcs: Vec<FlowArcPlan>,
    pub optimal_cost: i64,
    pub expected_flow: i64,
    pub fulfilled_flow: i64,
    pub fulfillment_ratio: f64,
    pub solver: String,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for MinCostFlowPlan {
    const FAMILY: &'static str = "ferrox.network_flow.plan";
    const VERSION: u16 = 1;
}
