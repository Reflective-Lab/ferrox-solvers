---
tags: [architecture, surface]
source: mixed
---
# Surface

`ferrox` exposes one canonical published crate (`ferrox`)
plus optional adapter crates with adapter-qualified names.

## Public surface

- `ferrox` — solver models, native solver wrappers, and solver-backed
  Converge suggestors.
- `ProvenanceSource` and `FERROX_PROVENANCE` for typed proposal provenance
  before crossing into `converge-pack::ProposedFact`.
- `ferrox.suggestor.execute` tracing spans on solver suggestor execution.

## Contract dependencies

- `converge-pack` — `Pack`, `ProposedFact`, `ProposedPlan`, `ProblemSpec`
- `converge-model` — semantic types
- `converge-provider` — capability identity (when applicable)

## Forbidden imports

Per [Extension Release Checklist §1](https://github.com/Reflective-Lab/converge/blob/main/kb/Standards/Extension%20Release%20Checklist.md):

- No imports of `converge-core` internals.
- No imports of foundation `runtime`, `provider`, or transport crates.
- No re-exports of foundation types except those promised stable.
