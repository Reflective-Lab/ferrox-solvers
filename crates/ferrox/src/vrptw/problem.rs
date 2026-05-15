use serde::{Deserialize, Serialize};

use converge_pack::{ExecutionIdentity, FactPayload};

/// A customer to be visited by the vehicle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Customer {
    pub id: usize,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// Earliest arrival time.
    pub window_open: i64,
    /// Latest arrival time (must arrive by this time).
    pub window_close: i64,
    /// Service duration at this customer.
    pub service_time: i64,
}

impl Customer {
    pub fn travel_to(&self, other: &Customer) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// The depot — vehicle starts and ends here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Depot {
    pub x: f64,
    pub y: f64,
    pub ready_time: i64,
    pub due_time: i64,
}

impl Depot {
    pub fn travel_to_customer(&self, c: &Customer) -> f64 {
        let dx = self.x - c.x;
        let dy = self.y - c.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Seeded into `ContextKey::Seeds` with id prefix `"vrptw-request:"`.
///
/// Models a single-vehicle TSP with Time Windows (TSPTW).
/// Customers are optional: the objective is to maximise customers visited
/// while respecting time windows and the vehicle's return deadline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VrptwRequest {
    pub id: String,
    pub depot: Depot,
    pub customers: Vec<Customer>,
    #[serde(default = "default_time_limit")]
    pub time_limit_seconds: f64,
}

impl FactPayload for VrptwRequest {
    const FAMILY: &'static str = "ferrox.vrptw.request";
    const VERSION: u16 = 1;
}

fn default_time_limit() -> f64 {
    30.0
}

/// One stop in the vehicle's route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteStop {
    pub customer_id: usize,
    pub customer_name: String,
    pub arrival: i64,
    pub departure: i64,
}

/// Written to `ContextKey::Strategies` with id prefix `"vrptw-plan-<solver>:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VrptwPlan {
    pub request_id: String,
    /// Ordered stops (depot → customers → depot is implied).
    pub route: Vec<RouteStop>,
    pub customers_total: usize,
    pub customers_visited: usize,
    /// Total travel distance (unscaled, Euclidean).
    pub total_distance: f64,
    /// Time the vehicle returns to depot.
    pub return_time: i64,
    pub solver: String,
    pub execution_identity: ExecutionIdentity,
    pub status: String,
    pub wall_time_seconds: f64,
}

impl FactPayload for VrptwPlan {
    const FAMILY: &'static str = "ferrox.vrptw.plan";
    const VERSION: u16 = 1;
}

impl VrptwPlan {
    #[allow(clippy::cast_precision_loss)]
    pub fn visit_ratio(&self) -> f64 {
        if self.customers_total == 0 {
            return 0.0;
        }
        self.customers_visited as f64 / self.customers_total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver_identity::non_native_solver_identity;

    fn cust(x: f64, y: f64) -> Customer {
        Customer {
            id: 1,
            name: "c".into(),
            x,
            y,
            window_open: 0,
            window_close: 100,
            service_time: 1,
        }
    }

    #[test]
    fn customer_travel_is_euclidean() {
        let a = cust(0.0, 0.0);
        let b = cust(3.0, 4.0);
        assert!((a.travel_to(&b) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn depot_travel_to_customer_is_euclidean() {
        let d = Depot {
            x: 0.0,
            y: 0.0,
            ready_time: 0,
            due_time: 100,
        };
        assert!((d.travel_to_customer(&cust(0.0, 5.0)) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn visit_ratio_zero_when_no_customers() {
        let p = VrptwPlan {
            request_id: "r".into(),
            route: vec![],
            customers_total: 0,
            customers_visited: 0,
            total_distance: 0.0,
            return_time: 0,
            solver: "x".into(),
            execution_identity: non_native_solver_identity("x", "test"),
            status: "feasible".into(),
            wall_time_seconds: 0.0,
        };
        assert!((p.visit_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn request_default_time_limit() {
        let json =
            r#"{"id":"r","depot":{"x":0,"y":0,"ready_time":0,"due_time":100},"customers":[]}"#;
        let r: VrptwRequest = serde_json::from_str(json).unwrap();
        assert!((r.time_limit_seconds - 30.0).abs() < f64::EPSILON);
    }
}
