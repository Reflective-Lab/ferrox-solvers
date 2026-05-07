use serde::{Deserialize, Serialize};

/// An agent that can execute tasks requiring one of its declared capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingAgent {
    pub id: usize,
    pub name: String,
    /// Capability tags this agent possesses (e.g. "python", "ml", "rust").
    pub capabilities: Vec<String>,
}

/// A unit of work to be scheduled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingTask {
    pub id: usize,
    pub name: String,
    /// The single capability an agent must have to execute this task.
    pub required_capability: String,
    /// Duration in minutes.
    pub duration_min: i64,
    /// Earliest start (minutes from horizon start).
    pub release_min: i64,
    /// Latest finish (minutes from horizon start).  Must be ≥ release + duration.
    pub deadline_min: i64,
}

/// Seeded into `ContextKey::Seeds` with id prefix `"scheduling-request:"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingRequest {
    pub id: String,
    pub agents: Vec<SchedulingAgent>,
    pub tasks: Vec<SchedulingTask>,
    /// Planning horizon in minutes.
    pub horizon_min: i64,
    /// Per-solver time budget in seconds.  Suggestors may honour or ignore this.
    #[serde(default = "default_time_limit")]
    pub time_limit_seconds: f64,
}

fn default_time_limit() -> f64 {
    30.0
}

/// A single task-to-agent assignment with resolved timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    pub task_id: usize,
    pub task_name: String,
    pub agent_id: usize,
    pub agent_name: String,
    pub start_min: i64,
    pub end_min: i64,
}

/// Written to `ContextKey::Strategies` with id prefix `"scheduling-plan-<solver>:"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingPlan {
    pub request_id: String,
    pub assignments: Vec<TaskAssignment>,
    pub tasks_total: usize,
    pub tasks_scheduled: usize,
    /// Completion time of the last scheduled task (0 if nothing scheduled).
    pub makespan_min: i64,
    /// Short identifier for the algorithm that produced this plan.
    pub solver: String,
    /// `"optimal"`, `"feasible"`, `"infeasible"`, or `"error"`.
    pub status: String,
    pub wall_time_seconds: f64,
}

impl SchedulingPlan {
    /// Throughput ratio: scheduled / total tasks.  Used to derive confidence.
    #[allow(clippy::cast_precision_loss)]
    pub fn throughput_ratio(&self) -> f64 {
        if self.tasks_total == 0 {
            return 0.0;
        }
        self.tasks_scheduled as f64 / self.tasks_total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_plan(tasks_total: usize, tasks_scheduled: usize) -> SchedulingPlan {
        SchedulingPlan {
            request_id: "r".into(),
            assignments: vec![],
            tasks_total,
            tasks_scheduled,
            makespan_min: 0,
            solver: "x".into(),
            status: "feasible".into(),
            wall_time_seconds: 0.0,
        }
    }

    #[test]
    fn throughput_ratio_zero_when_no_tasks() {
        let p = empty_plan(0, 0);
        assert!((p.throughput_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn throughput_ratio_partial() {
        let p = empty_plan(10, 7);
        assert!((p.throughput_ratio() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn request_serde_round_trip_with_default_time_limit() {
        let json = r#"{"id":"r","agents":[],"tasks":[],"horizon_min":120}"#;
        let r: SchedulingRequest = serde_json::from_str(json).unwrap();
        assert!((r.time_limit_seconds - 30.0).abs() < f64::EPSILON);
    }
}
