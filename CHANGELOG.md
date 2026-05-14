# Changelog

All notable changes to Ferrox will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic
Versioning before 1.0 with the usual pre-1.0 compatibility caveats.

## [Unreleased]

## [0.5.1] - 2026-05-14

### Added

- OR-Tools SimpleMinCostFlow wrapper and `MinCostFlowSuggestor` for
  `network-flow-request:*` seeds, including happy-path, negative, and
  property tests.
- Public re-exports and a `just example-flow` Formation demo for discovering
  the new min-cost-flow Suggestor surface.
- CP-SAT Boolean, table, optional-interval, cumulative, and 2D no-overlap
  request primitives across the Rust and gRPC conversion surfaces.
- Capability-map documentation that distinguishes Ferrox optimization/search
  from deferred SMT-style satisfiability work and from the existing
  `converge-optimization` pure Rust baseline, with a Suggestor-first rule for
  exposing solver capabilities and an explicit owner split for portable
  scheduling/routing baselines.
- Machine-readable `ferrox::catalog` selection guidance for matching common
  use cases to Suggestors, Packs, native features, seed prefixes, plan prefixes,
  and confidence expectations.
- Typed `ProvenanceSource` / `FERROX_PROVENANCE` adapter so solver-backed
  proposals use Ferrox's canonical provenance at the `ProposedFact` boundary.
- `ferrox.suggestor.execute` tracing spans at solver suggestor boundaries,
  with structured provenance, suggestor name, context keys, and input count.

### Changed

- OR-Tools native dependency reconciliation now verifies the requested
  `ORTOOLS_TAG` and rebuilds ignored local vendor checkouts when the tag or
  generated CMake config is stale.
- HiGHS native dependency reconciliation now verifies the requested
  `HIGHS_TAG`, switches stale ignored vendor checkouts, and rebuilds against
  the current HiGHS header layout.
- CP-SAT and MIP suggestors now reject unknown variable, interval, and
  objective references as `invalid` plans instead of solving weakened models.

### Fixed

- Greedy and CP-SAT task scheduling now handle stable, non-dense agent IDs
  without indexing the internal availability arrays by public agent ID.
- OR-Tools and HiGHS sys crates copy native runtime libraries into Cargo's
  build output and add that output to the runtime search path, so downstream
  examples and tests can launch without hand-setting `DYLD_LIBRARY_PATH`.

## [0.5.0] - 2026-05-07

### Added

- Standard GitHub community health files.
- `AGENTS.md` and capitalized `Justfile` for agent and local workflow entry.
- README guidance on native solver builds (`make ortools`, `make highs`,
  `FERROX_ORTOOLS_ROOT`, `FERROX_HIGHS_ROOT`).
- Comprehensive negative and property tests across CP, LP, MIP, scheduling,
  job-shop, and VRPTW suggestors plus a shared `test_support` MockContext.

### Changed

- Cargo packages renamed under the `converge-ferrox-*` prefix while keeping
  Rust library and binary names stable.

### Fixed

- CI `EXCLUDES` updated from `--exclude ferrox-server` to
  `--exclude converge-ferrox-server` so the `Check` / `Test` / `Lint`
  jobs actually skip the native-linking server crate. Same fix applied
  to the coverage workflow.

## [0.4.1] - 2026-05-05

### Added

- Current documented baseline for solver-backed Converge suggestors.
- OR-Tools CP-SAT support for scheduling, routing, job-shop, and generic CP
  models.
- HiGHS support for LP and MIP models.
- gRPC server wrapper for solver deployments.
- Standalone examples for CP Sudoku, multi-agent assignment, job-shop,
  vehicle routing, and MIP.
