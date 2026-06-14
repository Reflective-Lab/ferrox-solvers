---
source: llm
type: architecture-note
date: 2026-06-14
relates_to:
  - ../../crates/ferrox-server/
  - ../../Dockerfile
  - ../../proto/ferrox.proto
spec: marquee-apps/quorum-sense/docs/superpowers/specs/2026-06-14-converge-grpc-suggestor-pattern-design.md
---

# Cloud Run Deployment

Pointer note. Authoritative spec lives in quorum-sense (first consumer):
`marquee-apps/quorum-sense/docs/superpowers/specs/2026-06-14-converge-grpc-suggestor-pattern-design.md`.

## ferrox-solvers' role

ferrox-server is the **template service** for the pattern. M1 of the spec is
entirely ferrox-solvers work:

- Add `ops/cloudbuild.prod.yaml` + Cloud Run manifest under `ops/`.
- Add `tonic-health` so `grpc.health.v1.Health/Check` returns `SERVING`.
- Add tenant allowlist interceptor (compile-time `&[Tenant]` in
  `crates/ferrox-server/src/tenants.rs`).
- Deploy to GCP project `reflective-labs`, region `europe-west1`,
  `ingress=internal`.
- Image registry: `europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server:<tag>`.

## What stays

- `Dockerfile` is **self-contained** — Stage 1 builds OR-Tools v9.15 + HiGHS
  v1.14.0 in-tree; Stage 2 compiles the Rust server. Independent of
  `runtime-runway/docker/Dockerfile.math-base`. This is the §3.5 Rule 1 of the
  spec ("each extension owns its native build") and the reason ferrox is the
  template — not because it's the most complex, because its build was already
  designed this way.
- Optional bearer auth via `FERROX_AUTH_TOKEN` (env-gated in `main.rs`). v1
  prod ships with the env unset (tenant header is the only gate); deploy-time
  flip enables bearer.
- `FERROX_SERVER_MAX_BLOCKING_SOLVES=1` semaphore stays as the global ceiling
  in addition to per-tenant quotas.

## What does not change

`ferrox.v1.proto` package and `FerroxSolver` service — no proto changes needed
for Cloud Run deploy.

## Downstream

M2 (soter-server) follows the same pattern; M5 (prism-server) when prism is
ready. M4 (`RemoteCpSatBackend` consumers) when a marquee-app needs CP-SAT.
