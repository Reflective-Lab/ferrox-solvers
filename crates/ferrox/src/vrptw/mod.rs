//! Vehicle Routing with Time Windows — competing Formation suggestors.
//!
//! TSP with Time Windows (TSPTW): a single vehicle departs a depot, visits a set
//! of customers (each with an arrival time window and service duration), and
//! returns to the depot before closing time.  Customers are optional: the
//! objective is to maximise the number visited.
//!
//! | Suggestor | Algorithm | Confidence | Latency |
//! |---|---|---|---|
//! | [`NearestNeighborSuggestor`] | Nearest-neighbour heuristic | ≤ 0.60 | sub-ms |
//! | [`CpSatVrptwSuggestor`] | CP-SAT `AddCircuit` + time vars | 1.0 optimal | seconds |
//!
//! # CP-SAT model
//!
//! ```text
//! Nodes: 0 = depot, 1..=n = customers.
//!
//! x_ij ∈ {0,1}   arc i→j is used
//! x_ii ∈ {0,1}   self-loop: customer i is skipped
//!
//! AddCircuit over all arcs
//!
//! t_i ∈ [window_open_i, window_close_i]  arrival time (scaled × 100)
//!
//! For each arc (i→j), i≠j:
//!   t_j − t_i − M·x_ij ≥ svc_i + travel_ij − M   (Big-M time consistency)
//!
//! Objective: minimise Σ x_ii   (= maximise customers visited)
//! ```
//!
//! # Benchmark result (20-customer Solomon-style instance)
//!
//! ```text
//! Nearest neighbour:   5 / 20 customers   < 0.1 ms
//! CP-SAT:              8 / 20 customers   4.9 s   ← optimal  (+60% throughput)
//! ```

pub mod greedy;
pub mod problem;

#[cfg(feature = "ortools")]
pub mod cpsat;

pub use greedy::NearestNeighborSuggestor;
pub use problem::{Customer, Depot, RouteStop, VrptwPlan, VrptwRequest};

#[cfg(feature = "ortools")]
pub use cpsat::CpSatVrptwSuggestor;
