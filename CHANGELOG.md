# Changelog

All notable changes to Ferrox will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic
Versioning before 1.0 with the usual pre-1.0 compatibility caveats.

## [Unreleased]

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
