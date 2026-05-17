---
source: mixed
---
# Changelog

All notable changes to `ferrox` are recorded here.

## [Unreleased]

## [0.7.1] — 2026-05-17

- Bumped `converge-pack`, `converge-core`, `converge-model`,
  `converge-provider` floor from 3.9.0 to 3.9.1.
- No public API change; patch over 0.7.0.
- Internal sys-crate workspace pins (`converge-ferrox-ortools-sys`,
  `converge-ferrox-highs-sys`, `converge-ferrox-solver`) bumped to 0.7.1
  in lockstep.

## [Earlier]

- Adopted the [Extension Release Checklist](https://github.com/Reflective-Lab/converge/blob/main/kb/Standards/Extension%20Release%20Checklist.md):
  - Wired `just security-audit`, `just coverage`, `just performance-profile`, `just soak`.
  - Added `.github/workflows/{ci,coverage,security,stability}.yml`.
  - Coverage floor 80% enforced in coverage workflow.

## [0.1.0] — YYYY-MM-DD

- Initial release.
