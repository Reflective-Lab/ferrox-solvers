---
tags: [architecture, solvers, capability-map]
source: mixed
---
# Capability Map

This map separates the upstream solver platform from the Ferrox product
surface. OR-Tools v9.15 is a broad C++ optimization suite; Ferrox exposes
product-shaped Converge suggestors and narrow safe wrappers around the parts
needed by those suggestors.

Ferrox is an optimization and search extension. It is not an SMT layer, does
not try to expose satisfiability-modulo-theories semantics, and should not
become a general replacement for an SMT engine. SMT remains separate and
deferred unless Converge defines a distinct SMT contract outside Ferrox.

## Relationship To converge-optimization

Converge already carries `converge-optimization` in the foundation workspace as
the portable, in-process Rust optimization layer. It exposes optimization packs
through `converge_pack::PackSuggestor`, plus lower-level algorithms such as
Hungarian and auction assignment, Dijkstra shortest paths, max flow,
min-cost flow, Hopcroft-Karp bipartite matching, knapsack, set cover, and
scheduling helpers. With the `sat` feature, it also has a small Varisat-backed
CP module.

That changes Ferrox prioritization. Do not wrap OR-Tools graph, assignment, or
routing APIs simply because they exist upstream. Use `converge-optimization`
for lightweight Rust defaults, portable pack logic, and fast in-process
Suggestors. Use Ferrox when a native solver provides a clear product advantage:
larger instances, richer modeling primitives, stronger optimality behavior,
native backend parity, or solver diagnostics that the pure Rust layer does not
provide.

The Varisat-backed CP surface in `converge-optimization` is SAT/CP support, not
SMT. It does not change the Ferrox SMT posture below.

## Upstream OR-Tools v9.15

At a high level, OR-Tools v9.15 includes:

- CP-SAT for integer constraint programming, scheduling, combinatorial search,
  Boolean literals, linear constraints, intervals, no-overlap constraints, and
  routing-style circuit primitives.
- GLOP for linear programming through the legacy linear solver wrapper.
- Routing and legacy CP libraries for vehicle routing and routing search.
- Graph and flow algorithms, including max flow, min-cost flow, shortest-path
  style utilities, and linear-sum assignment.
- MathOpt as the newer unified modeling layer for LP/MIP-style optimization
  backends.
- PDLP for large-scale linear and quadratic programming.
- Additional algorithms such as knapsack, packing, set cover, scheduling
  helpers, and wrappers over supported commercial and open-source solvers.

That upstream list is not the Ferrox public contract. It is the capability
pool Ferrox may choose from when there is a product-shaped reason to wrap a
small, safe slice.

## Suggestor-First Exposure

Solver functionality should be exposed through Suggestors, not as a pile of
raw algorithm entrypoints. Low-level solvers and pure algorithms may exist
inside a crate, but the capability Converge products discover and use should
be a named Suggestor with:

- a stable seed prefix and plan prefix;
- typed request and plan data;
- explicit confidence semantics;
- happy-path, negative, and property tests where the input space is broad;
- documentation that says when to use the fast baseline versus the stronger
  native solver.

When a portable Rust baseline belongs in `converge-optimization`, expose it as
a pack or Suggestor there. When Ferrox adds native strength on top, expose that
as a separate competing Suggestor with a distinct plan prefix, so formations can
see both candidates and choose by confidence, policy, or downstream gates.

Ferrox also exposes a machine-readable selection catalog at
`ferrox::catalog`. Use `recommend_for_use_case` or `recommend_suggestors` to
map product intent to the right registration set. The catalog intentionally
includes `converge-optimization` baselines, Ferrox native Suggestors, Pack
surfaces, and the deferred SMT case so products do not have to infer solver
ownership from module names.

The portable baseline split is now:

- `converge-optimization::suggestors::GreedySchedulerSuggestor` owns the pure
  Rust EDF scheduling baseline for `scheduling-request:*` seeds and
  `scheduling-plan-greedy:*` plans.
- `converge-optimization::suggestors::NearestNeighborTimeWindowRoutingSuggestor`
  owns the pure Rust single-vehicle time-window routing baseline for
  `vrptw-request:*` seeds and `vrptw-plan-greedy:*` plans.
- `GreedyJobShopSuggestor`: reconcile with the existing
  `converge-optimization` job-shop pack rather than duplicating the problem
  class.

## Selection Matrix

| Use case | Register first | Register alongside / fallback | Do not use |
|---|---|---|---|
| Field crew or agent task scheduling | `CpSatSchedulerSuggestor` when OR-Tools is available | `GreedySchedulerSuggestor` from `converge-optimization` for immediate baseline | LP/MIP; they do not model optional intervals naturally |
| Factory job shop | `CpSatJobShopSuggestor` | `PackSuggestor<JobShopSchedulingPack>` or current greedy job-shop baseline | Generic CP-SAT unless no domain Suggestor fits |
| Single-vehicle time-window routing | `CpSatVrptwSuggestor` | `NearestNeighborTimeWindowRoutingSuggestor` from `converge-optimization` | Native OR-Tools routing until a routing-specific contract exists |
| Linear assignment / matching | `AssignmentSuggestor` or `PackSuggestor<AssignmentPack>` from `converge-optimization` | Ferrox native assignment only if a future workload proves the need | CP-SAT for ordinary square assignment |
| Source/sink min-cost flow | `FlowOptimizationSuggestor` from `converge-optimization` | `MinCostFlowSuggestor` when integer OR-Tools flow semantics or max-flow-min-cost mode are needed | General MIP unless the flow model has extra side constraints |
| Continuous LP | `GlopLpSuggestor` | Future MathOpt only with a migration/backend story | CP-SAT/MIP if variables are continuous only |
| Binary/integer MIP | `HighsMipSuggestor` | Domain pack if one already exists | GLOP, because integrality will be lost |
| Custom finite-domain model | `CpSatSuggestor` | Domain-specific Suggestor if the model repeats | SMT unless the question is logical satisfiability over theories |
| Cedar/logical counterexample | future `ferrox-smt` / `smt-gates` | Lean/Coq/Agda for checked proof evidence | Ferrox optimization solvers; CP-SAT is not SMT |

## Ferrox Today

| Category | Ferrox status | Current shape |
|---|---|---|
| CP-SAT | Current | `CpSatSuggestor`, `CpSatSchedulerSuggestor`, `CpSatJobShopSuggestor`, `CpSatVrptwSuggestor`, and `CpSatFormationSuggestor` behind the `ortools` feature. |
| GLOP | Current | `GlopLpSuggestor` for continuous LP requests behind the `ortools` feature. |
| HiGHS | Current | `HighsMipSuggestor` for continuous, integer, and binary MIP requests behind the `highs` feature. |
| Native routing | Deferred | Ferrox models VRPTW through CP-SAT today; it does not wrap OR-Tools' native routing solver yet. |
| Graph, flow, assignment | Partial native layer | `MinCostFlowSuggestor` exposes OR-Tools `SimpleMinCostFlow` for min-cost-flow and max-flow-with-min-cost requests. Broader graph algorithms and linear-sum assignment remain deferred because `converge-optimization` already covers Rust graph, flow, and assignment baselines. |
| MathOpt / PDLP | Deferred | OR-Tools v9.15 includes MathOpt and PDLP, but Ferrox has no Rust wrapper or suggestor contract for them yet. |
| SMT | Non-goal | Ferrox does not wrap Z3, cvc5, or an SMT-LIB surface; SMT belongs in a separate contract if needed. |

The current wrappers are deliberately narrow:

- CP-SAT safe wrapper: integer and Boolean variables, linear `<=`, `>=`, and
  `==` constraints, `AllDifferent`, fixed and optional intervals,
  `NoOverlap`, `Circuit`, objective direction, solve status, objective value,
  variable values, and wall time.
- GLOP safe wrapper: continuous variables, bounded row constraints, linear
  objective coefficients, objective direction, status, objective value, and
  primal variable values.
- SimpleMinCostFlow safe wrapper: directed arcs with integer capacities and
  costs, node supplies/demands, balanced min-cost solve, max-flow-with-min-cost
  solve, per-arc flows, optimal cost, and fulfilled-flow reporting.
- HiGHS safe wrapper: continuous, integer, and binary columns, row constraints,
  time limit, MIP relative gap, model status, objective value, column values,
  and reported MIP gap.

## Near-Term CP-SAT Primitives

Near-term CP-SAT expansion should remain incremental and problem-driven. Good
candidates are primitives that unlock existing product-shaped suggestors or
remove repeated modeling work:

- Optional interval and no-overlap improvements for richer scheduling and job
  shop variants.
- Reified linear constraints and enforcement literals when a suggestor needs
  conditional constraints.
- Cumulative/resource capacity constraints for teams, machines, crews, and
  capacity-limited windows.
- Circuit/path variants only when they support a Ferrox routing or sequencing
  request type.
- Solver parameters that affect explainable product behavior, such as time
  limits, search logging, and optimality gap reporting.

Avoid broad CP-SAT API mirroring. Each primitive should have a named Ferrox
use case, a safe Rust boundary, tests, and confidence semantics.

## Native Routing

OR-Tools v9.15 has a native routing library that is more specialized than the
current Ferrox CP-SAT VRPTW model and the heuristic routing packs in
`converge-optimization`. Ferrox should only wrap it when the product surface
needs routing-specific features such as multi-vehicle routing, dimensions,
pickup and delivery, disjunction penalties, vehicle capacities, or route-level
local search controls.

The first Ferrox routing API should not be a direct clone of OR-Tools
`RoutingModel`. It should be a narrow request/plan contract for a concrete
problem class, with stable confidence semantics and explicit limits on which
native routing features are supported.

## Graph, Flow, And Assignment

OR-Tools v9.15 exposes graph and network-flow algorithms, including max flow,
min-cost flow, and linear-sum assignment. Ferrox now exposes the smallest
useful slice first: `MinCostFlowSuggestor` reads `network-flow-request:*` seeds
and writes `network-flow-plan-ortools:*` strategies.

Use it for exact capacity matching, flow allocation, transportation-style
movement, and min-cost routing subproblems with integer capacities and costs.
Do not publish a generic graph toolbox unless a Converge product contract
actually needs that breadth. Native max-flow-only, shortest-path, and
linear-sum-assignment surfaces remain deferred because the pure Rust
`converge-optimization` layer already has the ordinary graph and assignment
coverage.

Before adding a native linear-sum-assignment wrapper, compare the desired
workload against `converge_optimization::assignment`. A Ferrox wrapper is only
worth carrying if it demonstrably improves scale, latency, diagnostics, or
feature coverage for a named Suggestor.

## MathOpt And PDLP

MathOpt is OR-Tools' newer unified optimization modeling layer, and PDLP is
aimed at large-scale LP/QP workloads. Both are deferred in Ferrox.

Reasons to keep them deferred now:

- Ferrox already has a stable GLOP LP surface and a HiGHS MIP surface.
- A MathOpt wrapper would overlap existing LP/MIP contracts unless it brings a
  clear migration or backend-selection story.
- PDLP needs different expectations around scaling, precision, termination,
  and confidence than the current small LP/MIP suggestors.

Revisit MathOpt or PDLP only with an issue that names the product workload,
the expected model size, the desired backend behavior, and how confidence or
optimality reporting will be represented.

## SMT Posture

Ferrox solves optimization/search problems: scheduling, routing, assignment,
LP, MIP, and related product-shaped optimization requests. SMT answers a
different class of question: logical satisfiability over theories such as
bitvectors, arrays, uninterpreted functions, arithmetic, and quantifiers.

The boundary is:

- Keep SMT out of Ferrox unless Converge creates a separate SMT extension or
  contract.
- Do not market CP-SAT as SMT. CP-SAT can encode Boolean and integer
  constraints, but it does not provide SMT-LIB semantics or theory solver
  coverage.
- If a product needs proof-oriented satisfiability, symbolic execution,
  program verification, theorem-like reasoning, or theory-specific models,
  route it to an SMT-specific component rather than Ferrox.

## Version And Vendor Discipline

The Makefile currently declares `ORTOOLS_TAG := v9.15`. That tag must be
reconciled whenever docs, solver labels, or native wrapper expectations change.

The `vendor/` directory is ignored by git. Do not treat a local vendored
checkout as source of record. Regenerate it from the Makefile, and keep the
documented capability map aligned with the checked-in build recipe and the
safe wrapper surface.
