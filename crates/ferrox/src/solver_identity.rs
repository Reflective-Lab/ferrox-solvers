use converge_pack::ExecutionIdentity;
// `ExecutionProducerIdentity` + `NativeExecutionIdentity` are only consumed
// by the native-solver builders (`cp_sat_*`, `glop_*`, `min_cost_flow_*`,
// `highs_*`), all of which are feature-gated. Match the same gating on the
// imports so a feature-off build doesn't drop into `-D unused-imports`.
#[cfg(any(feature = "ortools", feature = "highs"))]
use converge_pack::{ExecutionProducerIdentity, NativeExecutionIdentity};

pub fn unspecified_solver_identity() -> ExecutionIdentity {
    ExecutionIdentity::unspecified(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

pub fn non_native_solver_identity(
    solver: impl Into<String>,
    runtime_config: impl Into<String>,
) -> ExecutionIdentity {
    ExecutionIdentity::non_native(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        solver,
        runtime_config,
    )
}

#[cfg(feature = "ortools")]
pub fn cp_sat_solver_identity(runtime_config: impl Into<String>) -> ExecutionIdentity {
    ortools_solver_identity("cp-sat-v9.15", "cp_sat", runtime_config)
}

#[cfg(feature = "ortools")]
pub fn glop_solver_identity(runtime_config: impl Into<String>) -> ExecutionIdentity {
    ortools_solver_identity("glop-v9.15", "glop", runtime_config)
}

#[cfg(feature = "ortools")]
pub fn min_cost_flow_solver_identity(runtime_config: impl Into<String>) -> ExecutionIdentity {
    ortools_solver_identity(
        "simple-min-cost-flow-v9.15",
        "simple_min_cost_flow",
        runtime_config,
    )
}

#[cfg(feature = "ortools")]
fn ortools_solver_identity(
    solver: impl Into<String>,
    component: &str,
    runtime_config: impl Into<String>,
) -> ExecutionIdentity {
    let native_backend = format!("{}:{component}", ferrox_ortools_sys::ORTOOLS_NAME);
    ExecutionIdentity::new(
        ExecutionProducerIdentity::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        solver,
        ferrox_ortools_sys::ORTOOLS_VERSION,
        ferrox_ortools_sys::ORTOOLS_BUILD_CONFIG,
        runtime_config,
        Some(NativeExecutionIdentity::new(
            native_backend,
            ferrox_ortools_sys::ORTOOLS_VERSION,
            ferrox_ortools_sys::ORTOOLS_SOURCE_URL,
            ferrox_ortools_sys::ORTOOLS_EXPECTED_COMMIT,
            ferrox_ortools_sys::ortools_source_commit(),
            ferrox_ortools_sys::ortools_source_mode(),
        )),
    )
}

#[cfg(feature = "highs")]
pub fn highs_solver_identity(runtime_config: impl Into<String>) -> ExecutionIdentity {
    ExecutionIdentity::new(
        ExecutionProducerIdentity::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        "highs-v1.14.0",
        ferrox_highs_sys::HIGHS_VERSION,
        ferrox_highs_sys::HIGHS_BUILD_CONFIG,
        runtime_config,
        Some(NativeExecutionIdentity::new(
            ferrox_highs_sys::HIGHS_NAME,
            ferrox_highs_sys::HIGHS_VERSION,
            ferrox_highs_sys::HIGHS_SOURCE_URL,
            ferrox_highs_sys::HIGHS_EXPECTED_COMMIT,
            ferrox_highs_sys::highs_source_commit(),
            ferrox_highs_sys::highs_source_mode(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_native_identity_uses_shared_execution_contract() {
        let identity = non_native_solver_identity("test-solver", "test-config");

        assert_eq!(identity.backend, "test-solver");
        assert_eq!(identity.runtime_config, "test-config");
        assert_eq!(identity.producer.name, env!("CARGO_PKG_NAME"));
        assert!(identity.native_identity.is_none());
    }
}
