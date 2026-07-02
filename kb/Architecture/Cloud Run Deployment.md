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

## Deployed state (M1 shipped 2026-06-15)

| Field | Value |
|---|---|
| Project | `reflective-labs` (number 640630843925) |
| Region | `europe-west1` |
| Service URL | `https://ferrox-server-640630843925.europe-west1.run.app` |
| Ingress | `internal` |
| VPC connector | `solver-egress-ew1` (10.8.0.0/28, 2× e2-micro) |
| Service account | `ferrox-server@reflective-labs.iam.gserviceaccount.com` |
| Image tag | `v0.7.2-2966adc` (2026-06-15 — reflection enabled; was `v0.7.2-312605d` at initial ship) |
| Image registry | `europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server` |
| Concurrency | 1 (CP-SAT is single-process) |
| Min/Max instances | 1 / 10 |
| CPU / Memory | 2 vCPU / 4 GiB |
| Cloud Run timeout | 300s |
| Bearer auth | off (FERROX_AUTH_TOKEN unset; tenant header is the gate) |
| Tenant allowlist | `quorum-sense` (4 in-flight, compiled into image) |
| Health check | `grpc.health.v1.Health` via `tonic-health` |
| Reflection | `grpc.reflection.v1.ServerReflection` via `tonic-reflection` (live since v0.7.2-2966adc) |
| Smoke script | `ops/smoke.sh <url>` |
| Cloud Build SHA | `2e9ec147-cc07-42a1-8365-3cd99b6d43a7` (~18 min cold build) |

**Smoke verified 2026-06-15:**
- `grpc.health.v1.Health/Check` → `SERVING`
- `FerroxSolver/SolveCp` without `x-converge-app` → `INVALID_ARGUMENT: missing x-converge-app header`
- `FerroxSolver/SolveCp` with unknown tenant → `PERMISSION_DENIED: unknown tenant: <slug>`
- `FerroxSolver/SolveCp` with `x-converge-app: quorum-sense` + trivial CP (max x, 0≤x≤1) → `status: "optimal"`, `objective_value: 1`, `solver: "cp-sat-v9.15"`

**Reflection follow-up (RESOLVED 2026-06-15):** `tonic-reflection 0.14` registered server-side; image `v0.7.2-2966adc` shipped + smoke verified. `grpcurl <host>:443 list` returns three services (`ferrox.v1.FerroxSolver`, `grpc.health.v1.Health`, `grpc.reflection.v1.ServerReflection`). No `-proto` flag needed.

**Smoke connectivity note:** `gcloud run services proxy` failed via Homebrew gcloud (h2c local listener broken). The smoke was run by temporarily flipping `--ingress=all` for ~3 min, hitting the service URL directly with an ID-token-authenticated grpcurl from the dev laptop, then flipping back to `--ingress=internal`. Auth still required throughout. The recurring smoke path will be: Cloud Shell (browser, inside Google's network) once the service is invoker-bound to a Cloud-Shell-reachable identity, OR via the VPC connector from another in-VPC client.

## Unblocked

M2 (soter-server new) + M3 (quorum-sense flips to `RemoteSmtBackend`) can now plan against this service URL.
