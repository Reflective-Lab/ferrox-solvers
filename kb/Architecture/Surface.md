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
- Typed request and plan payloads for CP-SAT, LP, MIP, network flow,
  scheduling, job shop, VRPTW, and Formation planning. Payload schema identity
  is the Converge `(family, version)` pair; keys and ids route instances.
- `converge.execution_identity.evidence` facts emitted beside CP-SAT
  formation plans, keeping native execution identity out of Converge's generic
  `FormationPlan` payload while using the shared audit contract.
- `ProvenanceSource` and `FERROX_PROVENANCE` for typed proposal provenance
  before crossing into `converge-pack::ProposedFact`.
- `ferrox.suggestor.execute` tracing spans on solver suggestor execution.

## Contract dependencies

- `converge-pack` — `FactPayload`, `Pack`, `ProposedFact`, `ProposedPlan`,
  `ProblemSpec`
- `converge-model` — semantic types
- `converge-provider` — capability identity (when applicable)

## Forbidden imports

Per [Extension Release Checklist §1](https://github.com/Reflective-Lab/converge/blob/main/kb/Standards/Extension%20Release%20Checklist.md):

- No imports of `converge-core` internals.
- No imports of foundation `runtime`, `provider`, or transport crates.
- No re-exports of foundation types except those promised stable.
