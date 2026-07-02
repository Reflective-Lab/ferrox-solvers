---
source: mixed
---
# Milestones

> See `~/dev/reflective/stack/bedrock-platform/EPIC.md` for the coarse-grained outcomes these milestones advance.

## Shipped: v0.7.1 — Converge 3.9.1 alignment

**Released:** 2026-05-17 | **Tracks:** Converge 3.9.1

- Bumped `converge-pack`, `converge-core`, `converge-model`,
  `converge-provider` floor from 3.9.0 to 3.9.1.
- No public API change in Ferrox; patch-level over 0.7.0.
- Internal sys-crate workspace pins (`converge-ferrox-ortools-sys`,
  `converge-ferrox-highs-sys`, `converge-ferrox-solver`) bumped to 0.7.1
  in lockstep.
- All five `just release-check` gates green; tag pushed and crates
  republished to crates.io.

## Shipped: v0.5.1 — Converge 3.8.1 Solver Baseline

**Target:** 2026-05 | **Tracks:** Converge 3.8.1

- [x] Keep workspace package version at `0.5.1`.
- [x] Keep Converge dependencies on the `3.8.1` contract baseline.
- [x] Adopt Extension Release Checklist (security-audit, coverage, performance-profile, soak)
- [x] First clean `just release-check` run
- [x] Tag v0.5.1

## Next: Native Solver Assurance Hardening
**Epic:** E9

**Target:** 2026-05/06 | **Tracks:** OR-Tools + HiGHS reproducibility

Current state: Ferrox solver-backed outputs carry Converge's shared
`ExecutionIdentity`, and CP-SAT formation emits companion
`ExecutionIdentityEvidence` instead of leaking native details into
`FormationPlan`. The remaining work is reproducibility and CI enforcement:
making sure the identity Ferrox records is backed by a checked-in dependency
manifest and by platform CI that fails on native drift.

Why this matters:

- **Operator perspective:** OR-Tools and HiGHS can produce different behavior
  across versions, commits, or build flags. Production should not depend on an
  accidental local native install.
- **Audit perspective:** solver output is only inspectable if a later reviewer
  can connect the plan to the exact native source, build, and runtime config.
- **Release/CI perspective:** native checkout drift should break CI before a
  release artifact is cut.
- **Developer perspective:** external native roots are useful for local work,
  but release checks need a pinned, repeatable baseline.

- [ ] Add a checked-in native dependency lock/audit manifest for OR-Tools and
      HiGHS with name, version, source URL, expected checkout commit, build
      flags, and available artifact/header/library fingerprints.
- [ ] Add Linux and macOS CI coverage for full-feature Ferrox check, clippy,
      and tests.
- [ ] Make CI fail when the OR-Tools or HiGHS checkout commit differs from the
      checked-in manifest.
- [x] Record native solver identity on solver-backed Ferrox outputs, including
      backend version/build identity and runtime solver config, so audit can
      distinguish the same model solved by different native bits.
