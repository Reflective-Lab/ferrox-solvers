# Changelog

All notable changes to Ferrox will be documented in this file.

The format is based on Keep a Changelog, and this project follows Semantic
Versioning before 1.0 with the usual pre-1.0 compatibility caveats.

## [Unreleased]

### Added

- Standard GitHub community health files.
- `AGENTS.md` and capitalized `Justfile` for agent and local workflow entry.

### Changed

- Cargo packages renamed under the `converge-ferrox-*` prefix while keeping
  Rust library and binary names stable.

## [0.4.1] - 2026-05-05

### Added

- Current documented baseline for solver-backed Converge suggestors.
- OR-Tools CP-SAT support for scheduling, routing, job-shop, and generic CP
  models.
- HiGHS support for LP and MIP models.
- gRPC server wrapper for solver deployments.
- Standalone examples for CP Sudoku, multi-agent assignment, job-shop,
  vehicle routing, and MIP.
