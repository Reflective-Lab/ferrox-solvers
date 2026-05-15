use serde::{Deserialize, Serialize};

use converge_pack::{ExecutionIdentity, FactPayload};

/// One operation within a job: must execute on `machine_id` for `duration` units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub machine_id: usize,
    pub duration: i64,
}

/// A job is an ordered sequence of operations; each must complete before the next begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: usize,
    pub name: String,
    /// Operations in their required execution order.
    pub operations: Vec<Operation>,
}

/// Seeded into `ContextKey::Seeds` with id prefix `"jspbench-request:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobShopRequest {
    pub id: String,
    pub jobs: Vec<Job>,
    pub num_machines: usize,
    #[serde(default = "default_time_limit")]
    pub time_limit_seconds: f64,
}

impl FactPayload for JobShopRequest {
    const FAMILY: &'static str = "ferrox.jobshop.request";
    const VERSION: u16 = 1;
}

fn default_time_limit() -> f64 {
    30.0
}

impl JobShopRequest {
    /// Trivial upper bound on makespan: sum of all operation durations.
    pub fn horizon(&self) -> i64 {
        self.jobs
            .iter()
            .flat_map(|j| j.operations.iter())
            .map(|o| o.duration)
            .sum()
    }
}

/// A scheduled operation with resolved timing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledOp {
    pub job_id: usize,
    pub job_name: String,
    pub machine_id: usize,
    pub op_index: usize,
    pub start: i64,
    pub end: i64,
}

/// Written to `ContextKey::Strategies` with id prefix `"jspbench-plan-<solver>:"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobShopPlan {
    pub request_id: String,
    pub schedule: Vec<ScheduledOp>,
    pub makespan: i64,
    /// Proven lower bound (available when status is `"optimal"`).
    pub lower_bound: Option<i64>,
    pub solver: String,
    pub execution_identity: ExecutionIdentity,
    /// `"optimal"`, `"feasible"`, or `"error"`.
    pub status: String,
    pub wall_time_seconds: f64,
}

impl FactPayload for JobShopPlan {
    const FAMILY: &'static str = "ferrox.jobshop.plan";
    const VERSION: u16 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizon_zero_when_no_jobs() {
        let r = JobShopRequest {
            id: "r".into(),
            jobs: vec![],
            num_machines: 1,
            time_limit_seconds: 1.0,
        };
        assert_eq!(r.horizon(), 0);
    }

    #[test]
    fn horizon_sums_durations() {
        let r = JobShopRequest {
            id: "r".into(),
            jobs: vec![Job {
                id: 0,
                name: "j".into(),
                operations: vec![
                    Operation {
                        machine_id: 0,
                        duration: 4,
                    },
                    Operation {
                        machine_id: 1,
                        duration: 6,
                    },
                ],
            }],
            num_machines: 2,
            time_limit_seconds: 1.0,
        };
        assert_eq!(r.horizon(), 10);
    }

    #[test]
    fn request_default_time_limit() {
        let json = r#"{"id":"r","jobs":[],"num_machines":1}"#;
        let r: JobShopRequest = serde_json::from_str(json).unwrap();
        assert!((r.time_limit_seconds - 30.0).abs() < f64::EPSILON);
    }
}
