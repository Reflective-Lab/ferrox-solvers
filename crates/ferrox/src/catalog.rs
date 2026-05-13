//! Solver/Suggestor selection catalog.
//!
//! This module is intentionally data-oriented. Products should be able to ask
//! "what should I register for this problem?" without reverse-engineering crate
//! modules, feature flags, seed prefixes, or confidence semantics.

/// Where the capability is owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityHome {
    /// Portable Rust baseline in `converge-optimization`.
    ConvergeOptimization,
    /// Native solver-backed Suggestor in Ferrox.
    Ferrox,
    /// Not owned by Ferrox; needs a separate SMT/SAT contract.
    ExternalSmt,
}

/// How the capability is exposed to Converge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposureKind {
    /// Direct `Suggestor` implementation.
    Suggestor,
    /// `converge_pack::PackSuggestor<P>` around a pack.
    PackSuggestor,
    /// Deferred or external capability, documented so it is not misrouted.
    Deferred,
}

/// Product-shaped problem class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemClass {
    TaskScheduling,
    JobShopScheduling,
    TimeWindowRouting,
    Assignment,
    NetworkFlow,
    LinearProgramming,
    MixedIntegerProgramming,
    ConstraintProgramming,
    FormationAssembly,
    LogicalCounterexample,
}

/// What the use case values most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionGoal {
    /// Prefer a portable, low-latency baseline.
    FastBaseline,
    /// Prefer stronger optimization if available, while keeping a baseline as fallback.
    StrongerOptimization,
    /// Prefer a general modeler over a domain-shaped Suggestor.
    GeneralModeling,
    /// This is not optimization; route toward SMT/SAT counterexample search.
    LogicalCounterexample,
}

/// A common application use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonUseCase {
    FieldCrewScheduling,
    FactoryJobShop,
    DeliveryTimeWindows,
    AssignmentMatching,
    SourceSinkFlow,
    LinearProgram,
    MixedIntegerProgram,
    CustomCpSatModel,
    FormationAssembly,
    CedarPolicyCounterexample,
}

/// Selection request for catalog recommendations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRequest {
    pub problem_class: ProblemClass,
    pub goal: SelectionGoal,
    /// If false, recommendations demote native Ferrox candidates.
    pub native_available: bool,
    /// If true, pack surfaces are preferred over raw direct Suggestors when both fit.
    pub prefer_pack: bool,
}

impl SelectionRequest {
    #[must_use]
    pub const fn new(problem_class: ProblemClass, goal: SelectionGoal) -> Self {
        Self {
            problem_class,
            goal,
            native_available: true,
            prefer_pack: false,
        }
    }

    #[must_use]
    pub const fn without_native(mut self) -> Self {
        self.native_available = false;
        self
    }

    #[must_use]
    pub const fn prefer_pack(mut self) -> Self {
        self.prefer_pack = true;
        self
    }
}

impl From<CommonUseCase> for SelectionRequest {
    fn from(use_case: CommonUseCase) -> Self {
        match use_case {
            CommonUseCase::FieldCrewScheduling => Self::new(
                ProblemClass::TaskScheduling,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::FactoryJobShop => Self::new(
                ProblemClass::JobShopScheduling,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::DeliveryTimeWindows => Self::new(
                ProblemClass::TimeWindowRouting,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::AssignmentMatching => {
                Self::new(ProblemClass::Assignment, SelectionGoal::FastBaseline)
            }
            CommonUseCase::SourceSinkFlow => Self::new(
                ProblemClass::NetworkFlow,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::LinearProgram => Self::new(
                ProblemClass::LinearProgramming,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::MixedIntegerProgram => Self::new(
                ProblemClass::MixedIntegerProgramming,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::CustomCpSatModel => Self::new(
                ProblemClass::ConstraintProgramming,
                SelectionGoal::GeneralModeling,
            ),
            CommonUseCase::FormationAssembly => Self::new(
                ProblemClass::FormationAssembly,
                SelectionGoal::StrongerOptimization,
            ),
            CommonUseCase::CedarPolicyCounterexample => Self::new(
                ProblemClass::LogicalCounterexample,
                SelectionGoal::LogicalCounterexample,
            ),
        }
    }
}

/// A candidate Suggestor or Pack surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverCandidate {
    pub id: &'static str,
    pub home: CapabilityHome,
    pub exposure: ExposureKind,
    pub problem_class: ProblemClass,
    pub symbol: &'static str,
    pub crate_name: &'static str,
    pub seed_prefix: Option<&'static str>,
    pub plan_prefix: Option<&'static str>,
    pub feature: Option<&'static str>,
    pub confidence: &'static str,
    pub when_to_use: &'static str,
}

/// Full known catalog. This includes external/deferred entries so product
/// code can avoid treating CP-SAT or optimization solvers as SMT.
pub const SOLVER_CATALOG: &[SolverCandidate] = &[
    SolverCandidate {
        id: "converge.task-scheduling.greedy",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::TaskScheduling,
        symbol: "GreedySchedulerSuggestor",
        crate_name: "converge-optimization",
        seed_prefix: Some("scheduling-request:"),
        plan_prefix: Some("scheduling-plan-greedy:"),
        feature: None,
        confidence: "throughput_ratio * 0.65, capped at 0.65",
        when_to_use: "Immediate portable baseline for skilled task scheduling with time windows.",
    },
    SolverCandidate {
        id: "ferrox.task-scheduling.cpsat",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::TaskScheduling,
        symbol: "CpSatSchedulerSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("scheduling-request:"),
        plan_prefix: Some("scheduling-plan-cpsat:"),
        feature: Some("ortools"),
        confidence: "optimal => throughput_ratio; feasible => throughput_ratio * 0.85",
        when_to_use: "Quality anchor when schedule optimality or high utilization matters.",
    },
    SolverCandidate {
        id: "converge.job-shop.pack",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::PackSuggestor,
        problem_class: ProblemClass::JobShopScheduling,
        symbol: "PackSuggestor<JobShopSchedulingPack>",
        crate_name: "converge-optimization",
        seed_prefix: None,
        plan_prefix: None,
        feature: None,
        confidence: "pack confidence 0.75",
        when_to_use: "Portable pack baseline for job-shop scheduling through the gate path.",
    },
    SolverCandidate {
        id: "ferrox.job-shop.cpsat",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::JobShopScheduling,
        symbol: "CpSatJobShopSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("jspbench-request:"),
        plan_prefix: Some("jspbench-plan-cpsat:"),
        feature: Some("ortools"),
        confidence: "optimal => 1.0; feasible => 0.85",
        when_to_use: "Native interval/NoOverlap job-shop solver for makespan quality.",
    },
    SolverCandidate {
        id: "converge.vrptw.nearest-neighbor",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::TimeWindowRouting,
        symbol: "NearestNeighborTimeWindowRoutingSuggestor",
        crate_name: "converge-optimization",
        seed_prefix: Some("vrptw-request:"),
        plan_prefix: Some("vrptw-plan-greedy:"),
        feature: None,
        confidence: "visit_ratio * 0.60, capped at 0.60",
        when_to_use: "Immediate single-vehicle time-window routing baseline.",
    },
    SolverCandidate {
        id: "ferrox.vrptw.cpsat",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::TimeWindowRouting,
        symbol: "CpSatVrptwSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("vrptw-request:"),
        plan_prefix: Some("vrptw-plan-cpsat:"),
        feature: Some("ortools"),
        confidence: "optimal => visit_ratio; feasible => visit_ratio * 0.85",
        when_to_use: "Native CP-SAT TSPTW solver when visit count or time-window feasibility matters.",
    },
    SolverCandidate {
        id: "converge.assignment.hungarian",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::Assignment,
        symbol: "AssignmentSuggestor",
        crate_name: "converge-optimization",
        seed_prefix: Some("assignment-request:"),
        plan_prefix: Some("assignment-plan:"),
        feature: None,
        confidence: "utilization ratio",
        when_to_use: "Exact linear-sum assignment; no native Ferrox wrapper needed by default.",
    },
    SolverCandidate {
        id: "converge.assignment.pack",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::PackSuggestor,
        problem_class: ProblemClass::Assignment,
        symbol: "PackSuggestor<AssignmentPack>",
        crate_name: "converge-optimization",
        seed_prefix: None,
        plan_prefix: None,
        feature: None,
        confidence: "pack confidence 0.8 for fully matched output",
        when_to_use: "Gate-oriented task assignment pack.",
    },
    SolverCandidate {
        id: "converge.flow.min-cost",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::NetworkFlow,
        symbol: "FlowOptimizationSuggestor",
        crate_name: "converge-optimization",
        seed_prefix: Some("flow-request:"),
        plan_prefix: Some("flow-plan:"),
        feature: None,
        confidence: "fulfillment ratio",
        when_to_use: "Portable source/sink min-cost flow baseline.",
    },
    SolverCandidate {
        id: "ferrox.flow.simple-min-cost",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::NetworkFlow,
        symbol: "MinCostFlowSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("network-flow-request:"),
        plan_prefix: Some("network-flow-plan-ortools:"),
        feature: Some("ortools"),
        confidence: "balanced optimal => 1.0; max-flow-min-cost => fulfillment ratio",
        when_to_use: "Native OR-Tools integer min-cost circulation or max-flow-with-min-cost.",
    },
    SolverCandidate {
        id: "ferrox.lp.glop",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::LinearProgramming,
        symbol: "GlopLpSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("glop-request:"),
        plan_prefix: Some("glop-plan:"),
        feature: Some("ortools"),
        confidence: "optimal => 1.0; feasible => 0.85",
        when_to_use: "Continuous linear programs.",
    },
    SolverCandidate {
        id: "ferrox.mip.highs",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::MixedIntegerProgramming,
        symbol: "HighsMipSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("mip-request:"),
        plan_prefix: Some("mip-plan:"),
        feature: Some("highs"),
        confidence: "optimal => 1.0; feasible/time-limit => 0.85",
        when_to_use: "Continuous, integer, and binary MIP models.",
    },
    SolverCandidate {
        id: "ferrox.cp.cpsat",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::ConstraintProgramming,
        symbol: "CpSatSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("cpsat-request:"),
        plan_prefix: Some("cpsat-plan:"),
        feature: Some("ortools"),
        confidence: "optimal => 1.0; feasible => 0.85",
        when_to_use: "General integer/Boolean CP-SAT model when no domain Suggestor fits.",
    },
    SolverCandidate {
        id: "converge.formation.matching",
        home: CapabilityHome::ConvergeOptimization,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::FormationAssembly,
        symbol: "FormationAssemblySuggestor",
        crate_name: "converge-optimization",
        seed_prefix: Some("formation-request:"),
        plan_prefix: Some("formation-plan:"),
        feature: None,
        confidence: "coverage ratio",
        when_to_use: "Portable formation assembly baseline.",
    },
    SolverCandidate {
        id: "ferrox.formation.cpsat",
        home: CapabilityHome::Ferrox,
        exposure: ExposureKind::Suggestor,
        problem_class: ProblemClass::FormationAssembly,
        symbol: "CpSatFormationSuggestor",
        crate_name: "converge-ferrox-solver",
        seed_prefix: Some("cpsat-formation-request:"),
        plan_prefix: Some("cpsat-formation-plan:"),
        feature: Some("ortools"),
        confidence: "coverage ratio",
        when_to_use: "Weighted formation assembly when confidence, latency, and cost hints matter.",
    },
    SolverCandidate {
        id: "external.smt.counterexample",
        home: CapabilityHome::ExternalSmt,
        exposure: ExposureKind::Deferred,
        problem_class: ProblemClass::LogicalCounterexample,
        symbol: "ferrox-smt or smt-gates",
        crate_name: "not implemented",
        seed_prefix: None,
        plan_prefix: None,
        feature: None,
        confidence: "not applicable",
        when_to_use: "Logical satisfiability/counterexample search for policy and invariant claims.",
    },
];

/// Return the complete known solver catalog.
#[must_use]
pub const fn solver_catalog() -> &'static [SolverCandidate] {
    SOLVER_CATALOG
}

/// Find a candidate by stable catalog id.
#[must_use]
pub fn candidate_by_id(id: &str) -> Option<&'static SolverCandidate> {
    SOLVER_CATALOG.iter().find(|candidate| candidate.id == id)
}

/// Recommend candidates for a selection request, ordered by preference.
#[must_use]
pub fn recommend_suggestors(request: SelectionRequest) -> Vec<&'static SolverCandidate> {
    let mut candidates: Vec<_> = SOLVER_CATALOG
        .iter()
        .filter(|candidate| candidate.problem_class == request.problem_class)
        .collect();

    candidates.sort_by_key(|candidate| -score_candidate(candidate, request));
    candidates
}

/// Recommend candidates for a named common use case.
#[must_use]
pub fn recommend_for_use_case(use_case: CommonUseCase) -> Vec<&'static SolverCandidate> {
    recommend_suggestors(use_case.into())
}

fn score_candidate(candidate: &SolverCandidate, request: SelectionRequest) -> i32 {
    let mut score = 0;

    match request.goal {
        SelectionGoal::FastBaseline => match candidate.home {
            CapabilityHome::ConvergeOptimization => score += 100,
            CapabilityHome::Ferrox => score += 40,
            CapabilityHome::ExternalSmt => score -= 200,
        },
        SelectionGoal::StrongerOptimization => match candidate.home {
            CapabilityHome::Ferrox if request.native_available => score += 100,
            CapabilityHome::Ferrox => score += 25,
            CapabilityHome::ConvergeOptimization => score += 70,
            CapabilityHome::ExternalSmt => score -= 200,
        },
        SelectionGoal::GeneralModeling => match candidate.problem_class {
            ProblemClass::ConstraintProgramming
            | ProblemClass::LinearProgramming
            | ProblemClass::MixedIntegerProgramming => score += 100,
            _ => score += 20,
        },
        SelectionGoal::LogicalCounterexample => match candidate.home {
            CapabilityHome::ExternalSmt => score += 100,
            _ => score -= 200,
        },
    }

    if request.prefer_pack {
        match candidate.exposure {
            ExposureKind::PackSuggestor => score += 20,
            ExposureKind::Suggestor => score += 5,
            ExposureKind::Deferred => {}
        }
    }

    if candidate.home == CapabilityHome::Ferrox && !request.native_available {
        score -= 60;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduling_quality_goal_prefers_native_then_baseline() {
        let recommendations = recommend_for_use_case(CommonUseCase::FieldCrewScheduling);
        assert_eq!(recommendations[0].id, "ferrox.task-scheduling.cpsat");
        assert_eq!(recommendations[1].id, "converge.task-scheduling.greedy");
        assert_eq!(
            recommendations[0].seed_prefix,
            recommendations[1].seed_prefix
        );
        assert_ne!(
            recommendations[0].plan_prefix,
            recommendations[1].plan_prefix
        );
    }

    #[test]
    fn scheduling_fast_goal_prefers_portable_baseline() {
        let recommendations = recommend_suggestors(SelectionRequest::new(
            ProblemClass::TaskScheduling,
            SelectionGoal::FastBaseline,
        ));
        assert_eq!(recommendations[0].id, "converge.task-scheduling.greedy");
    }

    #[test]
    fn no_native_demotes_ferrox_but_keeps_it_visible() {
        let recommendations = recommend_suggestors(
            SelectionRequest::new(
                ProblemClass::TimeWindowRouting,
                SelectionGoal::StrongerOptimization,
            )
            .without_native(),
        );
        assert_eq!(recommendations[0].id, "converge.vrptw.nearest-neighbor");
        assert!(
            recommendations
                .iter()
                .any(|candidate| candidate.id == "ferrox.vrptw.cpsat")
        );
    }

    #[test]
    fn assignment_pack_can_be_preferred_for_gate_path() {
        let recommendations = recommend_suggestors(
            SelectionRequest::new(ProblemClass::Assignment, SelectionGoal::FastBaseline)
                .prefer_pack(),
        );
        assert_eq!(recommendations[0].id, "converge.assignment.pack");
    }

    #[test]
    fn logical_counterexample_does_not_pick_optimization_solver() {
        let recommendations = recommend_for_use_case(CommonUseCase::CedarPolicyCounterexample);
        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0].home, CapabilityHome::ExternalSmt);
        assert_eq!(recommendations[0].exposure, ExposureKind::Deferred);
    }

    #[test]
    fn every_live_candidate_has_discovery_surface() {
        for candidate in SOLVER_CATALOG
            .iter()
            .filter(|candidate| candidate.exposure != ExposureKind::Deferred)
        {
            assert!(
                candidate.seed_prefix.is_some()
                    || candidate.exposure == ExposureKind::PackSuggestor,
                "{} must declare a seed prefix or be a pack",
                candidate.id
            );
            assert!(
                candidate.plan_prefix.is_some()
                    || candidate.exposure == ExposureKind::PackSuggestor,
                "{} must declare a plan prefix or be a pack",
                candidate.id
            );
            assert!(!candidate.symbol.is_empty());
            assert!(!candidate.when_to_use.is_empty());
        }
    }
}
