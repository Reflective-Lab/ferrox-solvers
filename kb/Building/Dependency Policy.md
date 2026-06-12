---
source: llm
type: reference
---
# Dependency Policy

How Ferrox pins, reviews, and upgrades its dependencies. Last full
review: 2026-06-12 (everything current as of that date).

## What we depend on

| Dependency | Pin | Where pinned | Upstream cadence |
|---|---|---|---|
| OR-Tools | `v9.15` | `Makefile` (`ORTOOLS_TAG`, commit-verified) | ~2 releases/year |
| HiGHS | `v1.14.0` | `Makefile` (`HIGHS_TAG`, commit-verified) | ~quarterly; 1.x.y patches are solver bug fixes |
| converge-* | `3.9.2` | `Cargo.toml` workspace deps | platform-controlled (ours) |
| serde, tokio, thiserror, tracing, proptest, async-trait, cc | loose semver | `Cargo.toml` | continuous; resolved by `cargo update` |

Native solvers are cloned by tag and verified against an expected
commit in the `Makefile` — the pin is the source of truth, not the
contents of `vendor/`.

## Policy: pin and update deliberately

CP-SAT and HiGHS are mature solvers with very stable C APIs. A pinned
version genuinely lasts years. We do not chase releases.

1. **Review twice a year, adopt only with cause.** When OR-Tools cuts
   a release (~June and ~January historically), read the release notes
   for both solvers. Upgrade only for: a performance win on our problem
   class, a bug we have actually hit, or a security fix in bundled deps
   (Abseil/Protobuf are the usual suspects in OR-Tools).
2. **Take HiGHS patch releases (`1.x.y`) when convenient.** They are
   low-risk solver bug fixes. Skip minor releases unless the notes are
   relevant.
3. **Upgrade triggers, not timers:** wrong/suboptimal solution, crash,
   CVE in a bundled dependency, or a new solver feature we need.
4. **Rust deps:** run `cargo update` as part of the normal release
   flow. Loose ranges keep us on the latest compatible versions;
   `just security-audit` catches advisories between releases.
5. **Converge floor:** track the platform floor in `CLAUDE.md`
   (currently >= 3.9.1); bump workspace deps when the platform
   releases, since we control that cadence.

## Solver upgrade ritual

Before bumping `ORTOOLS_TAG` or `HIGHS_TAG`:

1. Update the tag *and* the expected commit in the `Makefile`.
2. Rebuild: `make ortools` / `make highs`.
3. Re-run the soak and property suites against the new solver —
   upgrades can change *which* optimal solution is returned even when
   both are correct.
4. Run `just release-check` before tagging.

## How to check for new versions

```bash
# Native solvers
gh api repos/google/or-tools/releases --jq '.[0].tag_name'
gh api repos/ERGO-Code/HiGHS/releases --jq '.[0].tag_name'

# Converge (sparse index; the crates.io JSON API rejects scripts)
curl -s https://index.crates.io/co/nv/converge-core | tail -1 | jq -r .vers

# Rust deps
cargo update --dry-run
```
