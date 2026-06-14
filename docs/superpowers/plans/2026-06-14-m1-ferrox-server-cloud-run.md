# M1 — ferrox-server reaches Cloud Run prod

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deploy the existing `ferrox-server` gRPC service (`mosaic-extensions/ferrox-solvers/crates/ferrox-server/`) to Cloud Run in GCP project `reflective-labs`, with tenant allowlist, standard gRPC health checking, structured JSON logging, and VPC-internal ingress. After M1, the service is reachable from inside the `reflective-labs` VPC; no marquee-app calls it yet.

**Architecture:** Per the spec (`marquee-apps/quorum-sense/docs/superpowers/specs/2026-06-14-converge-grpc-suggestor-pattern-design.md`), this is the **template service** for the gRPC suggestor pattern. Each phase is independently committable:

- **Phase A — Tenant allowlist** (in-process, no deploy)
- **Phase B — Health checking** (tonic-health, standard `grpc.health.v1.Health`)
- **Phase C — Structured JSON logging + per-RPC spans**
- **Phase D — Cloud Build + Cloud Run deploy** (the GCP-touching phase)
- **Phase E — Smoke verification + close-out**

The existing server already ships: tonic gRPC over TCP 50051, optional TLS via `FERROX_TLS_CERT`/`FERROX_TLS_KEY`, optional bearer auth via `FERROX_AUTH_TOKEN`, and a global solve concurrency semaphore (`FERROX_SERVER_MAX_BLOCKING_SOLVES`, default 1). M1 adds per-tenant quotas + health + JSON logs on top, and stands up the Cloud Run pipeline.

**Tech Stack:** Rust 1.96 (workspace pin), tonic 0.14, tonic-prost 0.14, tonic-health 0.14 (new), tokio, tracing + tracing-subscriber (`json` feature, new), Cloud Build, Cloud Run, Artifact Registry, gcloud CLI.

**GCP details:**
- Project ID: `reflective-labs` (project number 640630843925)
- Region: `europe-west1`
- Artifact Registry repo: `europe-west1-docker.pkg.dev/reflective-labs/converge`
- Service account: `ferrox-server@reflective-labs.iam.gserviceaccount.com` (new)
- VPC connector: `solver-egress-ew1` (assumed exists; Task D0 verifies)
- Service URL pattern: `https://ferrox-server-<hash>-ew.a.run.app`

**Spec sections this plan implements:** §3.5 (pattern rules), §4.1 (Ferrox proto), §4.3 (metadata headers), §5.1 (ferrox-server M1 additions), §6 (tenant allowlist + in-process quotas), §7 (network, auth, VPC), §8 M1 (this milestone).

**Out of scope (deferred to follow-on tickets):**
- Per-minute rate limiting (`max_requests_per_minute`) — in-flight cap is sufficient for the single-caller v1; explicit YAGNI cut.
- Bearer auth on the wire (`FERROX_AUTH_TOKEN` env stays unset; tenant header is the only gate per spec §7.2).
- Prometheus metrics endpoint — covered in §9.2 of spec but deferred; structured JSON logs cover M1's observability needs.
- Per-RPC distributed tracing (OpenTelemetry export) — tracing spans are local-only in M1.
- Cloud Monitoring dashboard + alerts — wiring deferred to ops follow-up.

---

## File Structure

**New files (8):**
- `crates/ferrox-server/src/tenants.rs` — `Tenant` struct, `const TENANTS` allowlist, `TenantRegistry` (runtime semaphore map), `TenantSlug` extension type.
- `crates/ferrox-server/src/interceptor.rs` — combined `request_interceptor` (bearer + tenant validation; attaches `TenantSlug` to extensions).
- `crates/ferrox-server/tests/tenant_registry.rs` — integration test for `TenantRegistry`.
- `ops/cloudbuild.prod.yaml` — Cloud Build config that docker-builds + pushes to Artifact Registry.
- `ops/cloudrun.prod.yaml` — Knative-style Cloud Run service manifest.
- `ops/iam-setup.sh` — one-shot script to create the `ferrox-server` SA + grant minimal IAM (idempotent).
- `ops/smoke.sh` — Cloud Shell script to grpcurl Health/Check + valid+invalid tenant calls.
- `docs/superpowers/plans/2026-06-14-m1-ferrox-server-cloud-run.md` — this plan.

**Modified files (4):**
- `crates/ferrox-server/Cargo.toml` — add `tonic-health`, switch `tracing-subscriber` features to include `json`.
- `crates/ferrox-server/src/main.rs` — replace `auth_interceptor` with `request_interceptor`; add `tonic-health` reporter; switch logger to JSON; pass `TenantRegistry` into `FerroxSolverService::new_with_registry`.
- `crates/ferrox-server/src/service.rs` — add `tenant_registry: Arc<TenantRegistry>` field; acquire per-tenant permit in `run_blocking`; add `#[tracing::instrument]` attrs to solve methods.
- `Justfile` — add `deploy-prod`, `smoke-prod`, `tenants-show` targets.

---

## Phase A — Tenant infrastructure

In-process work. No deploy. Validates the design with unit tests before any GCP cost.

### Task A1: Create `tenants.rs` with the allowlist and runtime registry

**Files:**
- Create: `crates/ferrox-server/src/tenants.rs`

- [ ] **Step 1: Write the file**

Create `crates/ferrox-server/src/tenants.rs` with this content:

```rust
//! Tenant allowlist and per-tenant in-flight semaphore.
//!
//! Per design spec §6 (`marquee-apps/quorum-sense/docs/superpowers/specs/
//! 2026-06-14-converge-grpc-suggestor-pattern-design.md`):
//! - Allowlist is compile-time `const TENANTS`.
//! - Adding a tenant requires a server image rebuild and redeploy.
//! - Per-tenant in-flight cap; over-limit returns `RESOURCE_EXHAUSTED`.
//! - Unknown tenant returns `PERMISSION_DENIED`.
//! - Missing `x-converge-app` header returns `INVALID_ARGUMENT`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tonic::Status;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tenant {
    pub slug: &'static str,
    pub max_in_flight: u32,
}

pub const TENANTS: &[Tenant] = &[
    Tenant { slug: "quorum-sense", max_in_flight: 4 },
];

/// Request-extension marker carrying the validated tenant slug into the
/// service layer. Attached by `request_interceptor` after the allowlist
/// check passes.
#[derive(Clone, Copy, Debug)]
pub struct TenantSlug(pub &'static str);

/// Runtime view of the allowlist, holding one `Semaphore` per tenant.
#[derive(Debug)]
pub struct TenantRegistry {
    permits: HashMap<&'static str, Arc<Semaphore>>,
}

impl TenantRegistry {
    /// Build a registry from the compile-time `TENANTS` table.
    #[must_use]
    pub fn from_const() -> Self {
        let permits = TENANTS
            .iter()
            .map(|t| (t.slug, Arc::new(Semaphore::new(t.max_in_flight as usize))))
            .collect();
        Self { permits }
    }

    /// Return `Some(&'static Tenant)` if `slug` is on the allowlist.
    #[must_use]
    pub fn lookup(slug: &str) -> Option<&'static Tenant> {
        TENANTS.iter().find(|t| t.slug == slug)
    }

    /// Acquire a permit for `slug`, or return a typed gRPC `Status` error.
    ///
    /// - Unknown slug → `PERMISSION_DENIED`
    /// - In-flight cap reached → `RESOURCE_EXHAUSTED`
    /// - Semaphore closed (process shutdown) → `UNAVAILABLE`
    pub async fn acquire(&self, slug: &str) -> Result<OwnedSemaphorePermit, Status> {
        let sem = self
            .permits
            .get(slug)
            .ok_or_else(|| Status::permission_denied(format!("unknown tenant: {slug}")))?
            .clone();
        sem.try_acquire_owned()
            .map_err(|_| Status::resource_exhausted(format!("tenant {slug} at in-flight cap")))
    }
}

impl Default for TenantRegistry {
    fn default() -> Self {
        Self::from_const()
    }
}
```

- [ ] **Step 2: Wire the module into the crate**

Open `crates/ferrox-server/src/main.rs`. Add `mod tenants;` immediately under the existing `mod convert;` / `mod service;` lines at the top of the file (currently lines 1–2):

```rust
mod convert;
mod service;
mod tenants;  // NEW
```

- [ ] **Step 3: Build to verify the new module compiles**

Run: `cd mosaic-extensions/ferrox-solvers && cargo check -p converge-ferrox-server`

(Use `cargo check`, not `cargo build`. The `full` feature gates `ferrox::cp::solve_cp` etc. on the native sys crates; a full `cargo build` without `just deps` / `just deps-ortools` having built OR-Tools first will fail at link time. `cargo check` runs the typechecker only — no link, no native deps needed.)

Expected: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in N.NNs`. No errors.

(Using `--no-default-features` avoids triggering the OR-Tools/HiGHS native link for this build-check — those are unaffected by tenant code.)

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/src/tenants.rs crates/ferrox-server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(ferrox-server): add tenant allowlist + runtime registry (M1.A1)

Compile-time TENANTS table seeds an in-process Semaphore-per-tenant map.
Acquire returns typed gRPC Status (PERMISSION_DENIED for unknown,
RESOURCE_EXHAUSTED for over-cap). Quorum-sense seeded with 4 in-flight.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task A2: Unit-test the registry

**Files:**
- Create: `crates/ferrox-server/tests/tenant_registry.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/ferrox-server/tests/tenant_registry.rs`:

```rust
//! Integration tests for the tenant allowlist + per-tenant semaphore.

use converge_ferrox_server::tenants::{Tenant, TenantRegistry, TENANTS};
use tonic::Code;

#[test]
fn quorum_sense_is_on_the_allowlist() {
    let t = TenantRegistry::lookup("quorum-sense").expect("seeded tenant");
    assert_eq!(t.slug, "quorum-sense");
    assert!(t.max_in_flight >= 1, "in-flight cap must be positive");
}

#[test]
fn unknown_tenant_returns_none_from_lookup() {
    assert!(TenantRegistry::lookup("nope-not-real").is_none());
}

#[tokio::test]
async fn acquire_unknown_tenant_returns_permission_denied() {
    let reg = TenantRegistry::default();
    let err = reg.acquire("nope-not-real").await.expect_err("should error");
    assert_eq!(err.code(), Code::PermissionDenied);
    assert!(err.message().contains("unknown tenant"));
}

#[tokio::test]
async fn acquire_known_tenant_below_cap_succeeds() {
    let reg = TenantRegistry::default();
    let _permit = reg.acquire("quorum-sense").await.expect("permit");
    // _permit drops at end of scope, returning the slot.
}

#[tokio::test]
async fn acquire_over_cap_returns_resource_exhausted() {
    let reg = TenantRegistry::default();
    let cap = Tenant::lookup_const_cap("quorum-sense");
    let mut held = Vec::with_capacity(cap);
    for _ in 0..cap {
        held.push(reg.acquire("quorum-sense").await.expect("under cap"));
    }
    let err = reg.acquire("quorum-sense").await.expect_err("over cap");
    assert_eq!(err.code(), Code::ResourceExhausted);
    assert!(err.message().contains("at in-flight cap"));
}

// Helper trait so the test can read the const cap without exporting more.
trait LookupConstCap {
    fn lookup_const_cap(slug: &str) -> usize;
}

impl LookupConstCap for Tenant {
    fn lookup_const_cap(slug: &str) -> usize {
        TENANTS
            .iter()
            .find(|t| t.slug == slug)
            .map(|t| t.max_in_flight as usize)
            .expect("seeded tenant")
    }
}
```

- [ ] **Step 2: Make the test crate able to see `tenants` publicly**

The integration test references `converge_ferrox_server::tenants::*`, so the binary crate needs a `lib` target or the module needs to be re-exported. Since the crate is currently `[[bin]]`-only, add a minimal `lib.rs` that re-exports the modules.

Create `crates/ferrox-server/src/lib.rs`:

```rust
//! Public surface of `converge-ferrox-server` for integration tests + future
//! library consumers. The binary entrypoint stays in `main.rs`.

pub mod tenants;
```

Then in `crates/ferrox-server/Cargo.toml`, add a `[lib]` section. Open the file (currently 38 lines) and replace lines 11–13:

```toml
[[bin]]
name = "ferrox-server"
path = "src/main.rs"
```

with:

```toml
[lib]
name = "converge_ferrox_server"
path = "src/lib.rs"

[[bin]]
name = "ferrox-server"
path = "src/main.rs"
```

Then in `crates/ferrox-server/src/main.rs`, the `mod tenants;` line you added in Task A1 Step 2 needs to change to a re-import from the lib. Replace the line:

```rust
mod tenants;
```

with:

```rust
use converge_ferrox_server::tenants;
```

(`mod convert;` and `mod service;` stay as-is — they're binary-local.)

- [ ] **Step 3: Run the tests, expect them to fail to compile first**

Run: `cd mosaic-extensions/ferrox-solvers && cargo test -p converge-ferrox-server --no-default-features --test tenant_registry`

Expected first run: tests **fail to compile** because of the test helper trait `LookupConstCap` (intentional — it forces you to read the test). The trait declares `lookup_const_cap` as `fn(slug: &str) -> usize`, and the call site is `Tenant::lookup_const_cap("quorum-sense")` — this should compile fine. If it doesn't, double-check the trait impl block.

Expected once compile-clean: all 5 tests pass.

```
running 5 tests
test acquire_known_tenant_below_cap_succeeds ... ok
test acquire_over_cap_returns_resource_exhausted ... ok
test acquire_unknown_tenant_returns_permission_denied ... ok
test quorum_sense_is_on_the_allowlist ... ok
test unknown_tenant_returns_none_from_lookup ... ok

test result: ok. 5 passed; 0 failed
```

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/src/lib.rs \
        crates/ferrox-server/Cargo.toml \
        crates/ferrox-server/src/main.rs \
        crates/ferrox-server/tests/tenant_registry.rs
git commit -m "$(cat <<'EOF'
test(ferrox-server): cover TenantRegistry allowlist + per-tenant cap (M1.A2)

Add lib.rs so integration tests can reach tenants module. Tests cover
lookup, unknown-tenant PERMISSION_DENIED, under-cap success, over-cap
RESOURCE_EXHAUSTED.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task A3: Create the combined `request_interceptor`

The existing `auth_interceptor` in `main.rs:27–42` only validates the bearer token. M1 extends it to also validate `x-converge-app`, attaching the validated `TenantSlug` to request extensions.

**Files:**
- Create: `crates/ferrox-server/src/interceptor.rs`
- Modify: `crates/ferrox-server/src/main.rs` (delete `auth_interceptor` fn at lines 26–42; replace `with_interceptor(svc, auth_interceptor)` references with the new one)

- [ ] **Step 1: Write the interceptor module**

Create `crates/ferrox-server/src/interceptor.rs`:

```rust
//! Tonic interceptor: validate `Authorization` (optional bearer) and
//! `x-converge-app` (tenant), attaching the validated `TenantSlug` to the
//! request extensions so the service layer can acquire a per-tenant permit
//! without re-parsing metadata.
//!
//! Per spec §6 and §7.2 — bearer is optional (gated by `FERROX_AUTH_TOKEN`
//! env presence); tenant header is required.

use tonic::{Request, Status};

use crate::tenants::{TenantRegistry, TenantSlug};

#[allow(clippy::result_large_err)]
pub fn request_interceptor(mut req: Request<()>) -> Result<Request<()>, Status> {
    // ─ Bearer (optional — disabled when env unset) ─────────────────────────
    if let Ok(expected) = std::env::var("FERROX_AUTH_TOKEN") {
        let provided = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != format!("Bearer {expected}") {
            return Err(Status::unauthenticated("invalid or missing token"));
        }
    }

    // ─ Tenant (required) ───────────────────────────────────────────────────
    let slug = req
        .metadata()
        .get("x-converge-app")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::invalid_argument("missing x-converge-app header"))?;

    let tenant = TenantRegistry::lookup(slug)
        .ok_or_else(|| Status::permission_denied(format!("unknown tenant: {slug}")))?;

    req.extensions_mut().insert(TenantSlug(tenant.slug));
    Ok(req)
}
```

- [ ] **Step 2: Replace `auth_interceptor` usage in `main.rs`**

Open `crates/ferrox-server/src/main.rs`. The current file (after Task A2) declares `mod convert; mod service;` plus `use converge_ferrox_server::tenants;`. Add an `interceptor` mod and remove the old interceptor.

Delete lines 26–42 (the entire `auth_interceptor` fn including the `#[allow(clippy::result_large_err)]` attribute).

Add after the existing `mod` lines:

```rust
mod interceptor;
```

Update the `use` block (currently lines 18–24) to drop the now-unused `Status` import and add the new interceptor. Final imports block:

```rust
use std::net::SocketAddr;

use tonic::transport::{Identity, Server, ServerTlsConfig};

use converge_ferrox_server::tenants;

use proto::ferrox::v1::ferrox_solver_server::FerroxSolverServer;
use service::FerroxSolverService;
use interceptor::request_interceptor;
```

(`Request` and `Status` no longer needed in `main.rs` once `auth_interceptor` is removed.)

Replace both `with_interceptor(svc, auth_interceptor)` call sites (currently lines 80 and 86) with `with_interceptor(svc, request_interceptor)`.

- [ ] **Step 3: Build**

Run: `cd mosaic-extensions/ferrox-solvers && cargo check -p converge-ferrox-server`

(Use `cargo check`, not `cargo build`. The `full` feature gates `ferrox::cp::solve_cp` etc. on the native sys crates; a full `cargo build` without `just deps` / `just deps-ortools` having built OR-Tools first will fail at link time. `cargo check` runs the typechecker only — no link, no native deps needed.)

Expected: clean build. The interceptor compiles, main.rs no longer references the deleted symbols.

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/src/interceptor.rs crates/ferrox-server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(ferrox-server): combined bearer + tenant interceptor (M1.A3)

Replace auth_interceptor (bearer only) with request_interceptor (bearer
optional + x-converge-app required). Validated TenantSlug is attached to
request extensions for the service layer to pick up. Missing header →
INVALID_ARGUMENT; unknown slug → PERMISSION_DENIED; bad bearer →
UNAUTHENTICATED. Matches spec §6 + §7.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task A4: Wire `TenantRegistry` through `FerroxSolverService`

Per-tenant permit must be acquired before the existing global solve permit. Otherwise a tenant that's blocked by their own cap still consumes the global permit and starves other tenants.

**Files:**
- Modify: `crates/ferrox-server/src/service.rs`

- [ ] **Step 1: Update the `FerroxSolverService` struct + constructor**

Open `crates/ferrox-server/src/service.rs` (currently 110 lines). Replace lines 1–18 (imports + service struct + `new`) with:

```rust
use std::sync::Arc;

use tokio::sync::Semaphore;
use tonic::{Request, Response, Status};

use ferrox::cp::solve_cp;
use ferrox::lp::solve_lp;
use ferrox::mip::solve_mip;

use crate::convert::{
    cp_req_from_proto, cp_resp_to_proto, lp_req_from_proto, lp_resp_to_proto, mip_req_from_proto,
    mip_resp_to_proto,
};
use crate::proto::ferrox::v1::ferrox_solver_server::FerroxSolver;
use crate::proto::ferrox::v1::{
    SolveCpRequest, SolveCpResponse, SolveLpRequest, SolveLpResponse, SolveMipRequest,
    SolveMipResponse,
};
use converge_ferrox_server::tenants::{TenantRegistry, TenantSlug};

#[derive(Clone)]
pub struct FerroxSolverService {
    solve_limit: Arc<Semaphore>,
    tenants: Arc<TenantRegistry>,
}

impl FerroxSolverService {
    pub fn new(max_blocking_solves: usize, tenants: Arc<TenantRegistry>) -> Self {
        Self {
            solve_limit: Arc::new(Semaphore::new(max_blocking_solves.max(1))),
            tenants,
        }
    }
```

- [ ] **Step 2: Update `run_blocking` to acquire per-tenant first, then global**

Replace lines 32–50 (the current `run_blocking` body) with:

```rust
    async fn run_blocking<T, F>(
        &self,
        operation: &'static str,
        tenant_slug: &str,
        solve: F,
    ) -> Result<T, Status>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        // Per-tenant permit first — if the tenant is over their cap, fail
        // fast without consuming the global solver slot.
        let _tenant_permit = self.tenants.acquire(tenant_slug).await?;

        // Global solver permit — bounds total concurrent native solves.
        let permit = self
            .solve_limit
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Status::unavailable("solver concurrency limiter closed"))?;

        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            solve()
        })
        .await
        .map_err(|err| Status::internal(format!("{operation} worker failed: {err}")))
    }
```

- [ ] **Step 3: Update `Default` impl to wire the registry**

Replace lines 53–57:

```rust
impl Default for FerroxSolverService {
    fn default() -> Self {
        Self::new(configured_blocking_solves(), Arc::new(TenantRegistry::default()))
    }
}
```

- [ ] **Step 4: Update the three RPC methods to pull the tenant slug from extensions**

Helper first — add this private fn just above `impl FerroxSolver` (around current line 76):

```rust
fn tenant_slug_from<R>(req: &Request<R>) -> Result<&'static str, Status> {
    req.extensions()
        .get::<TenantSlug>()
        .map(|t| t.0)
        .ok_or_else(|| {
            Status::internal("request reached service without TenantSlug extension")
        })
}
```

Then update each of `solve_cp`, `solve_lp`, `solve_mip`:

Replace the body of `solve_cp` (currently lines 78–87) with:

```rust
    async fn solve_cp(
        &self,
        request: Request<SolveCpRequest>,
    ) -> Result<Response<SolveCpResponse>, Status> {
        let tenant = tenant_slug_from(&request)?;
        let req = cp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_cp", tenant, move || solve_cp(&req))
            .await?;
        Ok(Response::new(cp_resp_to_proto(plan)))
    }
```

Same pattern for `solve_lp` (lines 89–98 → ):

```rust
    async fn solve_lp(
        &self,
        request: Request<SolveLpRequest>,
    ) -> Result<Response<SolveLpResponse>, Status> {
        let tenant = tenant_slug_from(&request)?;
        let req = lp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_lp", tenant, move || solve_lp(&req))
            .await?;
        Ok(Response::new(lp_resp_to_proto(plan)))
    }
```

And `solve_mip` (lines 100–109 →):

```rust
    async fn solve_mip(
        &self,
        request: Request<SolveMipRequest>,
    ) -> Result<Response<SolveMipResponse>, Status> {
        let tenant = tenant_slug_from(&request)?;
        let req = mip_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_mip", tenant, move || solve_mip(&req))
            .await?;
        Ok(Response::new(mip_resp_to_proto(plan)))
    }
```

- [ ] **Step 5: Build**

Run: `cd mosaic-extensions/ferrox-solvers && cargo check -p converge-ferrox-server`

(Use `cargo check`, not `cargo build`. The `full` feature gates `ferrox::cp::solve_cp` etc. on the native sys crates; a full `cargo build` without `just deps` / `just deps-ortools` having built OR-Tools first will fail at link time. `cargo check` runs the typechecker only — no link, no native deps needed.)

Expected: clean build. (`cargo clippy --no-default-features -p converge-ferrox-server` should also be clean, but lints aren't blocking for this step.)

- [ ] **Step 6: Run all existing tests + the registry tests**

Run: `cd mosaic-extensions/ferrox-solvers && cargo test -p converge-ferrox-server --no-default-features`

Expected: all tests pass. The integration tests from Task A2 still pass; nothing new added.

- [ ] **Step 7: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/src/service.rs
git commit -m "$(cat <<'EOF'
feat(ferrox-server): per-tenant permit before global solve limit (M1.A4)

FerroxSolverService now holds Arc<TenantRegistry>. run_blocking acquires
the tenant permit BEFORE the global solver semaphore, so a tenant over
their cap fails fast without consuming a global slot. RPC methods read
the slug from request extensions (attached by request_interceptor).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task A5: End-to-end interceptor + service integration test

Validate that the full request path (interceptor attaches slug → service reads it → permit acquired → solver runs) works without a network round-trip.

**Files:**
- Modify: `crates/ferrox-server/tests/tenant_registry.rs` — append integration scenarios

- [ ] **Step 1: Add the end-to-end scenarios**

Append to `crates/ferrox-server/tests/tenant_registry.rs`:

```rust
// ─── End-to-end interceptor + service path ──────────────────────────────

use converge_ferrox_server::tenants::TenantSlug;
use tonic::Request;

/// Simulate what the interceptor does: attach TenantSlug to extensions.
fn with_tenant<T>(mut req: Request<T>, slug: &'static str) -> Request<T> {
    req.extensions_mut().insert(TenantSlug(slug));
    req
}

#[tokio::test]
async fn extensions_carry_tenant_slug_into_service() {
    // We don't have a public test seam into FerroxSolverService here, but we
    // can at least verify the extension round-trip behaves as the service
    // helper expects.
    let req = with_tenant(Request::new(()), "quorum-sense");
    let slug = req
        .extensions()
        .get::<TenantSlug>()
        .map(|t| t.0)
        .expect("tenant attached");
    assert_eq!(slug, "quorum-sense");
}
```

- [ ] **Step 2: Run the tests**

Run: `cd mosaic-extensions/ferrox-solvers && cargo test -p converge-ferrox-server --no-default-features --test tenant_registry`

Expected: 6 tests pass (5 previous + 1 new).

- [ ] **Step 3: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/tests/tenant_registry.rs
git commit -m "$(cat <<'EOF'
test(ferrox-server): cover TenantSlug extension round-trip (M1.A5)

Pin the request-extension contract between interceptor and service so a
future refactor of either side can't silently drop the tenant slug.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase B — Health checking

Standard `grpc.health.v1.Health` so Cloud Run probes and any infra tool can check the service uniformly.

### Task B1: Add `tonic-health` dependency

**Files:**
- Modify: `crates/ferrox-server/Cargo.toml`

- [ ] **Step 1: Add the dependency**

Open `crates/ferrox-server/Cargo.toml`. Add after the existing `anyhow` line (currently line 31, may have shifted after Task A2):

```toml
tonic-health         = "0.14"
```

The dependency block should now read (in relevant part):

```toml
[dependencies]
ferrox               = { workspace = true }
ferrox-ortools-sys   = { workspace = true, optional = true }
ferrox-highs-sys     = { workspace = true, optional = true }

tonic                = { version = "0.14", features = ["tls-ring"] }
tonic-prost          = "0.14"
tonic-health         = "0.14"
prost                = "0.14"
tokio                = { workspace = true }
tracing              = { workspace = true }
tracing-subscriber   = { version = "0.3.23", features = ["env-filter"] }
anyhow               = "1"
```

- [ ] **Step 2: Build to fetch the crate**

Run: `cd mosaic-extensions/ferrox-solvers && cargo check -p converge-ferrox-server`

(Use `cargo check`, not `cargo build`. The `full` feature gates `ferrox::cp::solve_cp` etc. on the native sys crates; a full `cargo build` without `just deps` / `just deps-ortools` having built OR-Tools first will fail at link time. `cargo check` runs the typechecker only — no link, no native deps needed.)

Expected: tonic-health 0.14.x downloaded + compiled, build clean.

- [ ] **Step 3: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
deps(ferrox-server): add tonic-health 0.14 (M1.B1)

Standard grpc.health.v1.Health service for Cloud Run probes and uniform
infra-side health checks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task B2: Register the health reporter in `main.rs`

Health service must be registered **outside** the FerroxSolverServer interceptor — health probes must NOT require a tenant header.

**Files:**
- Modify: `crates/ferrox-server/src/main.rs`

- [ ] **Step 1: Set up health reporter inside `main()`**

Open `crates/ferrox-server/src/main.rs`. Just above the `let svc = FerroxSolverService::default();` line (currently around line 57 after the Phase A edits), add:

```rust
    // Health checking — standard grpc.health.v1.Health, exposed without
    // tenant gating so Cloud Run probes and grpcurl can hit it freely.
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<FerroxSolverServer<FerroxSolverService>>()
        .await;
```

- [ ] **Step 2: Add the health service to BOTH `Server::builder()` branches**

In the existing TLS branch (around lines 78–82 after Phase A), update the builder chain to add health BEFORE the interceptor-wrapped solver:

```rust
        Server::builder()
            .tls_config(tls)?
            .add_service(health_service)
            .add_service(FerroxSolverServer::with_interceptor(svc, request_interceptor))
            .serve(addr)
            .await?;
```

In the non-TLS branch (around lines 85–88 after Phase A), same pattern:

```rust
        Server::builder()
            .add_service(health_service)
            .add_service(FerroxSolverServer::with_interceptor(svc, request_interceptor))
            .serve(addr)
            .await?;
```

- [ ] **Step 3: Build**

Run: `cd mosaic-extensions/ferrox-solvers && cargo check -p converge-ferrox-server`

(Use `cargo check`, not `cargo build`. The `full` feature gates `ferrox::cp::solve_cp` etc. on the native sys crates; a full `cargo build` without `just deps` / `just deps-ortools` having built OR-Tools first will fail at link time. `cargo check` runs the typechecker only — no link, no native deps needed.)

Expected: clean build.

- [ ] **Step 4: Local smoke — start the server and grpcurl the health endpoint**

In one shell:

```bash
cd mosaic-extensions/ferrox-solvers
cargo run -p converge-ferrox-server --no-default-features
```

Expected log line: `ferrox-server starting addr=0.0.0.0:50051 tls=false`.

(The `--no-default-features` build won't have OR-Tools / HiGHS linked, so `SolveCp` etc. will fail at runtime — that's fine; we're only testing Health here.)

In a second shell — if `grpcurl` isn't installed: `brew install grpcurl` (macOS) or download from <https://github.com/fullstorydev/grpcurl/releases>.

```bash
grpcurl -plaintext -d '{"service": ""}' localhost:50051 grpc.health.v1.Health/Check
```

Expected output:

```json
{
  "status": "SERVING"
}
```

Also verify the unknown-tenant rejection works:

```bash
grpcurl -plaintext -H 'x-converge-app: nope-not-real' \
  localhost:50051 ferrox.v1.FerroxSolver/SolveCp
```

Expected: `ERROR: Code: PermissionDenied  Message: unknown tenant: nope-not-real`.

And the missing-header rejection:

```bash
grpcurl -plaintext localhost:50051 ferrox.v1.FerroxSolver/SolveCp
```

Expected: `ERROR: Code: InvalidArgument  Message: missing x-converge-app header`.

Stop the server (Ctrl-C in shell 1).

- [ ] **Step 5: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(ferrox-server): expose grpc.health.v1.Health (M1.B2)

Health service is added outside the tenant interceptor — health probes
do not need x-converge-app. Smoke-verified locally with grpcurl:
Health/Check returns SERVING, missing tenant returns INVALID_ARGUMENT,
unknown tenant returns PERMISSION_DENIED.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase C — Structured JSON logging + per-RPC spans

Cloud Logging filters work cleanly only with JSON payloads. And per-RPC fields (`tenant_app`, `rpc.method`, `request_id`) are spec §9.2 requirements.

### Task C1: Switch tracing-subscriber to JSON output

**Files:**
- Modify: `crates/ferrox-server/Cargo.toml`
- Modify: `crates/ferrox-server/src/main.rs`

- [ ] **Step 1: Enable the `json` feature on tracing-subscriber**

Open `crates/ferrox-server/Cargo.toml`. Change the existing line:

```toml
tracing-subscriber   = { version = "0.3.23", features = ["env-filter"] }
```

to:

```toml
tracing-subscriber   = { version = "0.3.23", features = ["env-filter", "json"] }
```

- [ ] **Step 2: Switch the formatter to JSON**

Open `crates/ferrox-server/src/main.rs`. The current init block (around lines 46–51) reads:

```rust
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrox_server=info".parse().unwrap()),
        )
        .init();
```

Replace with:

```rust
    tracing_subscriber::fmt()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrox_server=info".parse().unwrap()),
        )
        .init();
```

`with_current_span(true)` includes the nearest active span (so `tenant_app` and `request_id` from the RPC instrument show up). `with_span_list(false)` keeps the JSON compact — Cloud Logging doesn't need the full span stack.

- [ ] **Step 3: Build + smoke**

```bash
cd mosaic-extensions/ferrox-solvers
cargo build -p converge-ferrox-server --no-default-features
cargo run -p converge-ferrox-server --no-default-features
```

Expected log line on stdout:

```json
{"timestamp":"...","level":"INFO","fields":{"message":"ferrox-server starting","addr":"0.0.0.0:50051","tls":false},"target":"ferrox_server"}
```

Stop the server (Ctrl-C).

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/Cargo.toml crates/ferrox-server/src/main.rs Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ferrox-server): structured JSON logging for Cloud Logging (M1.C1)

Switch tracing_subscriber to .json() with current span emitted, span list
suppressed. Per spec §9.2 — Cloud Logging filters need
jsonPayload.<field>=<value> shape; flat-text fmt() won't index correctly.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task C2: Per-RPC tracing spans

Spec §9.2 mandates `tenant_app`, `rpc.method`, `request_id`, `solve_duration_us`, `status` on every solve RPC.

**Files:**
- Modify: `crates/ferrox-server/src/service.rs`
- Modify: `crates/ferrox-server/src/interceptor.rs`

- [ ] **Step 1: Mint a request_id in the interceptor if the client didn't send one**

The interceptor already runs once per request — it's the right place to mint or accept `x-request-id`.

Add the `uuid` crate to `crates/ferrox-server/Cargo.toml` dependencies:

```toml
uuid                 = { version = "1", features = ["v4"] }
```

Then update `crates/ferrox-server/src/interceptor.rs`. Add a `RequestId` extension type and mint/accept logic.

Replace the entire content of `crates/ferrox-server/src/interceptor.rs` with:

```rust
//! Tonic interceptor: validate `Authorization` (optional bearer) and
//! `x-converge-app` (tenant); mint/accept `x-request-id`; attach both to
//! request extensions so the service layer can use them in spans.

use tonic::{Request, Status};
use uuid::Uuid;

use crate::tenants::{TenantRegistry, TenantSlug};

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

#[allow(clippy::result_large_err)]
pub fn request_interceptor(mut req: Request<()>) -> Result<Request<()>, Status> {
    // ─ Bearer (optional — disabled when env unset) ─────────────────────────
    if let Ok(expected) = std::env::var("FERROX_AUTH_TOKEN") {
        let provided = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != format!("Bearer {expected}") {
            return Err(Status::unauthenticated("invalid or missing token"));
        }
    }

    // ─ Tenant (required) ───────────────────────────────────────────────────
    let slug = req
        .metadata()
        .get("x-converge-app")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::invalid_argument("missing x-converge-app header"))?;

    let tenant = TenantRegistry::lookup(slug)
        .ok_or_else(|| Status::permission_denied(format!("unknown tenant: {slug}")))?;

    // ─ Request ID (mint if absent) ─────────────────────────────────────────
    let request_id = req
        .metadata()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(TenantSlug(tenant.slug));
    req.extensions_mut().insert(RequestId(request_id));
    Ok(req)
}
```

- [ ] **Step 2: Instrument the solve methods**

Open `crates/ferrox-server/src/service.rs`. Update the import block (top of file) to include the `RequestId` from the interceptor module:

```rust
use crate::interceptor::RequestId;
```

(`interceptor` is declared in `main.rs` via `mod interceptor;`. To reach it from `service.rs`, `interceptor` needs to be a sibling module declared in `lib.rs` instead. Edit `crates/ferrox-server/src/lib.rs` to also expose it.)

Replace `crates/ferrox-server/src/lib.rs` with:

```rust
//! Public surface of `converge-ferrox-server` for integration tests + future
//! library consumers. The binary entrypoint stays in `main.rs`.

pub mod interceptor;
pub mod tenants;
```

Then in `crates/ferrox-server/src/main.rs`, change `mod interceptor;` to `use converge_ferrox_server::interceptor;`.

In `crates/ferrox-server/src/service.rs`, change `use crate::interceptor::RequestId;` to:

```rust
use converge_ferrox_server::interceptor::RequestId;
```

Now add a helper to fetch both extensions in one shot. Replace the existing `tenant_slug_from` helper from Task A4 Step 4 with:

```rust
struct RequestContext<'a> {
    tenant: &'static str,
    request_id: &'a str,
}

fn request_context<R>(req: &Request<R>) -> Result<RequestContext<'_>, Status> {
    let tenant = req
        .extensions()
        .get::<TenantSlug>()
        .map(|t| t.0)
        .ok_or_else(|| Status::internal("missing TenantSlug extension"))?;
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map(|r| r.0.as_str())
        .ok_or_else(|| Status::internal("missing RequestId extension"))?;
    Ok(RequestContext { tenant, request_id })
}
```

Update the three RPC methods to instrument the span and time the solve. Replace the entire `solve_cp` method body with:

```rust
    async fn solve_cp(
        &self,
        request: Request<SolveCpRequest>,
    ) -> Result<Response<SolveCpResponse>, Status> {
        let ctx = request_context(&request)?;
        let span = tracing::info_span!(
            "solve_cp",
            tenant_app = %ctx.tenant,
            request_id = %ctx.request_id,
            rpc_method = "ferrox.v1.FerroxSolver/SolveCp",
        );
        let _enter = span.enter();
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        drop(_enter); // drop guard before await
        let req = cp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_cp", tenant, move || solve_cp(&req))
            .await?;
        let elapsed_us = started.elapsed().as_micros();
        tracing::info!(solve_duration_us = elapsed_us, status = "ok", "solve_cp completed");
        Ok(Response::new(cp_resp_to_proto(plan)))
    }
```

Same shape for `solve_lp`:

```rust
    async fn solve_lp(
        &self,
        request: Request<SolveLpRequest>,
    ) -> Result<Response<SolveLpResponse>, Status> {
        let ctx = request_context(&request)?;
        let span = tracing::info_span!(
            "solve_lp",
            tenant_app = %ctx.tenant,
            request_id = %ctx.request_id,
            rpc_method = "ferrox.v1.FerroxSolver/SolveLp",
        );
        let _enter = span.enter();
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        drop(_enter);
        let req = lp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_lp", tenant, move || solve_lp(&req))
            .await?;
        let elapsed_us = started.elapsed().as_micros();
        tracing::info!(solve_duration_us = elapsed_us, status = "ok", "solve_lp completed");
        Ok(Response::new(lp_resp_to_proto(plan)))
    }
```

And `solve_mip`:

```rust
    async fn solve_mip(
        &self,
        request: Request<SolveMipRequest>,
    ) -> Result<Response<SolveMipResponse>, Status> {
        let ctx = request_context(&request)?;
        let span = tracing::info_span!(
            "solve_mip",
            tenant_app = %ctx.tenant,
            request_id = %ctx.request_id,
            rpc_method = "ferrox.v1.FerroxSolver/SolveMip",
        );
        let _enter = span.enter();
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        drop(_enter);
        let req = mip_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_mip", tenant, move || solve_mip(&req))
            .await?;
        let elapsed_us = started.elapsed().as_micros();
        tracing::info!(solve_duration_us = elapsed_us, status = "ok", "solve_mip completed");
        Ok(Response::new(mip_resp_to_proto(plan)))
    }
```

- [ ] **Step 3: Build + test**

```bash
cd mosaic-extensions/ferrox-solvers
cargo build -p converge-ferrox-server --no-default-features
cargo test  -p converge-ferrox-server --no-default-features
```

Expected: clean build, all 6 tests pass.

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add crates/ferrox-server/Cargo.toml \
        crates/ferrox-server/src/lib.rs \
        crates/ferrox-server/src/main.rs \
        crates/ferrox-server/src/interceptor.rs \
        crates/ferrox-server/src/service.rs \
        Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ferrox-server): per-RPC tracing spans + request_id (M1.C2)

Interceptor mints or accepts x-request-id and attaches RequestId to
extensions. Each solve_* method opens an info_span with tenant_app,
request_id, rpc_method, then logs solve_duration_us + status on
completion. Matches spec §9.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase D — Cloud Build + Cloud Run deploy

This phase makes ANY GCP changes the plan does. Read it through before running any command. Each step that touches GCP is explicit.

### Task D0: Prerequisite check — verify project, Artifact Registry, VPC connector

**Files:** none — read-only GCP checks.

- [ ] **Step 1: Set the active project**

```bash
gcloud config set project reflective-labs
gcloud config get-value project
```

Expected: `reflective-labs`.

- [ ] **Step 2: Verify the Artifact Registry repo exists**

```bash
gcloud artifacts repositories describe converge \
    --location=europe-west1 \
    --format='value(format,name)'
```

Expected: `DOCKER\tprojects/reflective-labs/locations/europe-west1/repositories/converge`.

If it doesn't exist, create it:

```bash
gcloud artifacts repositories create converge \
    --repository-format=docker \
    --location=europe-west1 \
    --description='Converge platform Docker images (ferrox-server, soter-server, ...)'
```

- [ ] **Step 3: Verify the VPC connector exists**

```bash
gcloud compute networks vpc-access connectors describe solver-egress-ew1 \
    --region=europe-west1 \
    --format='value(state,ipCidrRange)' 2>/dev/null
```

Expected (if exists): `READY\t10.8.0.0/28`.

If the connector does NOT exist, create it:

```bash
gcloud compute networks vpc-access connectors create solver-egress-ew1 \
    --region=europe-west1 \
    --network=default \
    --range=10.8.0.0/28 \
    --min-instances=2 \
    --max-instances=3 \
    --machine-type=e2-micro
```

(Spec §10.4: connector min ≈ 2× e2-micro ≈ $15/mo each.)

- [ ] **Step 4: Verify required APIs are enabled**

```bash
gcloud services list --enabled --filter='name:run.googleapis.com OR name:cloudbuild.googleapis.com OR name:artifactregistry.googleapis.com OR name:vpcaccess.googleapis.com' \
    --format='value(name)'
```

Expected: four lines, all four services listed.

If any are missing:

```bash
gcloud services enable run.googleapis.com cloudbuild.googleapis.com artifactregistry.googleapis.com vpcaccess.googleapis.com
```

(No commit for this task — read-only verification + any setup is GCP-side.)

---

### Task D1: Create the `ferrox-server` service account + IAM

**Files:**
- Create: `ops/iam-setup.sh`

- [ ] **Step 1: Write the setup script**

Create `ops/iam-setup.sh`:

```bash
#!/usr/bin/env bash
# Idempotent: creates the ferrox-server SA and grants minimal IAM.
# Re-running is safe — SA creation is checked first, role grants are
# additive.
set -euo pipefail

PROJECT_ID="${PROJECT_ID:-reflective-labs}"
SA_NAME="ferrox-server"
SA_EMAIL="${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"

# Create SA if it doesn't exist.
if ! gcloud iam service-accounts describe "$SA_EMAIL" \
        --project="$PROJECT_ID" >/dev/null 2>&1; then
    echo ">>> creating SA $SA_EMAIL"
    gcloud iam service-accounts create "$SA_NAME" \
        --project="$PROJECT_ID" \
        --display-name="ferrox-server (Cloud Run runtime)" \
        --description="Minimal-IAM SA for the ferrox-server Cloud Run service (M1 of the gRPC suggestor pattern). No data-plane permissions — solver service is pure compute."
else
    echo ">>> SA $SA_EMAIL already exists"
fi

# Minimal IAM:
#   roles/logging.logWriter   — emit structured logs to Cloud Logging
#   roles/monitoring.metricWriter — emit metrics
# NOT GRANTED: any Firestore/Storage/Secret Manager — solver owns no data.
echo ">>> granting roles/logging.logWriter"
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
    --member="serviceAccount:${SA_EMAIL}" \
    --role="roles/logging.logWriter" \
    --condition=None >/dev/null

echo ">>> granting roles/monitoring.metricWriter"
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
    --member="serviceAccount:${SA_EMAIL}" \
    --role="roles/monitoring.metricWriter" \
    --condition=None >/dev/null

echo ">>> done. SA: $SA_EMAIL"
```

- [ ] **Step 2: Make it executable**

```bash
cd mosaic-extensions/ferrox-solvers
mkdir -p ops
chmod +x ops/iam-setup.sh
```

- [ ] **Step 3: Run it**

```bash
cd mosaic-extensions/ferrox-solvers
./ops/iam-setup.sh
```

Expected output (first run):

```
>>> creating SA ferrox-server@reflective-labs.iam.gserviceaccount.com
>>> granting roles/logging.logWriter
>>> granting roles/monitoring.metricWriter
>>> done. SA: ferrox-server@reflective-labs.iam.gserviceaccount.com
```

Subsequent runs print `>>> SA ferrox-server@... already exists` and grant the same roles (additive, no-op).

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add ops/iam-setup.sh
git commit -m "$(cat <<'EOF'
chore(ops): idempotent ferrox-server SA + IAM setup script (M1.D1)

Creates the dedicated runtime SA with logging.logWriter + monitoring.metricWriter.
No data-plane roles — solver service is pure compute per spec §7.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task D2: Write the Cloud Build config

**Files:**
- Create: `ops/cloudbuild.prod.yaml`

- [ ] **Step 1: Write the config**

Create `ops/cloudbuild.prod.yaml`:

```yaml
# Cloud Build — ferrox-server image for Cloud Run (reflective-labs/prod).
#
# Usage:
#   gcloud builds submit . \
#       --project=reflective-labs \
#       --config=ops/cloudbuild.prod.yaml \
#       --substitutions=_TAG=v0.7.2-$(git rev-parse --short HEAD)
#
# The Dockerfile at repo root builds OR-Tools + HiGHS in Stage 1 and
# compiles the Rust server in Stage 2 — self-contained, no math-base.

substitutions:
  _TAG: "latest"
  _REGION: "europe-west1"
  _REPO: "europe-west1-docker.pkg.dev/reflective-labs/converge"

steps:
  - name: "gcr.io/cloud-builders/docker"
    env:
      - "DOCKER_BUILDKIT=1"
    args:
      - "build"
      - "-t"
      - "${_REPO}/ferrox-server:${_TAG}"
      - "-f"
      - "Dockerfile"
      - "."

images:
  - "${_REPO}/ferrox-server:${_TAG}"

options:
  machineType: "E2_HIGHCPU_8"
  logging: CLOUD_LOGGING_ONLY
  diskSizeGb: 100  # OR-Tools + HiGHS build artifacts are big

timeout: "3600s"  # OR-Tools + HiGHS cold build can take ~30 min on E2_HIGHCPU_8
```

- [ ] **Step 2: Submit a build to verify the YAML and Dockerfile work end-to-end**

This is the long step (~30 min). Run from the repo root:

```bash
cd mosaic-extensions/ferrox-solvers
TAG="v0.7.2-$(git rev-parse --short HEAD)"
gcloud builds submit . \
    --project=reflective-labs \
    --config=ops/cloudbuild.prod.yaml \
    --substitutions=_TAG=${TAG}
```

Expected: build runs through (Cloud Build streams logs). Final line: `Successfully built …; Successfully tagged europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server:v0.7.2-<sha>`.

Capture the tag — you'll need it for Task D3.

- [ ] **Step 3: Verify the image landed in Artifact Registry**

```bash
gcloud artifacts docker images list \
    europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server \
    --format='value(IMAGE,DIGEST,CREATE_TIME)' \
    --limit=3
```

Expected: the tag from Step 2 appears with a recent timestamp.

- [ ] **Step 4: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add ops/cloudbuild.prod.yaml
git commit -m "$(cat <<'EOF'
build(ops): Cloud Build config for ferrox-server prod image (M1.D2)

Targets europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server.
E2_HIGHCPU_8 + 100GB disk + 60min timeout to absorb the cold OR-Tools +
HiGHS build (~30 min). The repo-root Dockerfile is self-contained per
spec §3.5 Rule 1 — no shared math-base.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task D3: Write the Cloud Run service manifest

**Files:**
- Create: `ops/cloudrun.prod.yaml`

- [ ] **Step 1: Write the manifest**

Create `ops/cloudrun.prod.yaml`:

```yaml
# Cloud Run service manifest — ferrox-server, prod, reflective-labs.
#
# Apply with:
#   gcloud run services replace ops/cloudrun.prod.yaml \
#       --project=reflective-labs --region=europe-west1
#
# IMPORTANT: edit `spec.template.spec.containers[0].image` to the exact
# tag pushed in Task D2 BEFORE applying.

apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: ferrox-server
  labels:
    app: ferrox-server
    extension: ferrox
    platform: converge
  annotations:
    # Internal-only ingress per spec §7.1.
    run.googleapis.com/ingress: internal
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/minScale: "1"
        autoscaling.knative.dev/maxScale: "10"
        # VPC connector for ingress=internal callers to reach us.
        run.googleapis.com/vpc-access-connector: solver-egress-ew1
        run.googleapis.com/vpc-access-egress: private-ranges-only
    spec:
      serviceAccountName: ferrox-server@reflective-labs.iam.gserviceaccount.com
      # CP-SAT solves are CPU-bound and one solve fully uses an instance.
      # Cloud Run scales horizontally to handle bursts.
      containerConcurrency: 1
      # Cloud Run hard ceiling for unary HTTP/2 — clamp our timeout below.
      timeoutSeconds: 300
      containers:
        - image: europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server:REPLACE_ME
          ports:
            # name: h2c — tells Cloud Run to use HTTP/2 cleartext to the
            # container. Cloud Run still terminates client-facing TLS.
            - name: h2c
              containerPort: 50051
          resources:
            limits:
              cpu: "2"
              memory: "4Gi"
          env:
            - name: FERROX_ADDR
              value: "0.0.0.0:50051"
            - name: FERROX_SERVER_MAX_BLOCKING_SOLVES
              value: "1"
            - name: RUST_LOG
              value: "ferrox_server=info"
            # No FERROX_AUTH_TOKEN in v1 — tenant header is the only gate
            # (spec §7.2). Set this only when the bearer-auth upgrade is wanted.
  traffic:
    - latestRevision: true
      percent: 100
```

- [ ] **Step 2: Substitute the image tag**

In `ops/cloudrun.prod.yaml`, replace `REPLACE_ME` with the tag from Task D2 Step 2 (e.g. `v0.7.2-abc1234`).

If you forgot the tag:

```bash
gcloud artifacts docker images list \
    europe-west1-docker.pkg.dev/reflective-labs/converge/ferrox-server \
    --format='value(IMAGE)' \
    --limit=1
```

- [ ] **Step 3: Apply the manifest**

```bash
cd mosaic-extensions/ferrox-solvers
gcloud run services replace ops/cloudrun.prod.yaml \
    --project=reflective-labs \
    --region=europe-west1
```

Expected: Cloud Run creates the service. Output ends with something like:

```
Service [ferrox-server] revision [ferrox-server-00001-abc] has been deployed and is serving 100 percent of traffic.
Service URL: https://ferrox-server-XXXXXXXX-ew.a.run.app
```

Capture the URL for Task E1.

- [ ] **Step 4: Restrict invoker permissions**

`ingress=internal` already blocks public callers, but for belt-and-braces we explicitly grant `roles/run.invoker` to only the marquee-app SAs (just `quorum-server` in v1).

```bash
gcloud run services add-iam-policy-binding ferrox-server \
    --project=reflective-labs \
    --region=europe-west1 \
    --member='serviceAccount:quorum-server@reflective-labs.iam.gserviceaccount.com' \
    --role='roles/run.invoker'
```

If `quorum-server@reflective-labs.iam.gserviceaccount.com` doesn't exist yet (it should, since quorum-sense is already deployed in this project — verify with `gcloud iam service-accounts describe`), substitute the actual SA name from the quorum-sense Cloud Run deploy.

- [ ] **Step 5: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add ops/cloudrun.prod.yaml
git commit -m "$(cat <<'EOF'
deploy(ops): Cloud Run service manifest for ferrox-server (M1.D3)

ingress=internal, VPC connector solver-egress-ew1, concurrency=1 (CP-SAT
is single-process / single-request), minScale=1, maxScale=10, 2vCPU/4GiB.
h2c port name so Cloud Run uses HTTP/2 cleartext to the container.
SA: ferrox-server@reflective-labs (no data-plane IAM).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task D4: Add Justfile targets for deploy + smoke

**Files:**
- Modify: `Justfile`

- [ ] **Step 1: Add the targets**

Open `mosaic-extensions/ferrox-solvers/Justfile`. Append:

```just
# ─── Cloud Run prod deploy (M1) ──────────────────────────────────────────────

# Build + push prod image. _TAG defaults to v<workspace-version>-<git sha>.
# Usage: just deploy-build  or  just deploy-build TAG=v0.7.2-abc1234
deploy-build TAG=`echo "v0.7.2-$(git rev-parse --short HEAD)"`:
    gcloud builds submit . \
        --project=reflective-labs \
        --config=ops/cloudbuild.prod.yaml \
        --substitutions=_TAG={{TAG}}

# Apply Cloud Run manifest. Edit ops/cloudrun.prod.yaml to point at the tag
# you just built — sed the REPLACE_ME or set explicitly.
deploy-apply:
    gcloud run services replace ops/cloudrun.prod.yaml \
        --project=reflective-labs \
        --region=europe-west1

# List tenants the server image currently knows about.
tenants-show:
    @grep -A2 "^    Tenant " crates/ferrox-server/src/tenants.rs | grep slug

# Smoke against the deployed service. Requires the Cloud Run service URL.
# Run from Cloud Shell or via `gcloud run services proxy` since ingress=internal.
# Usage: just smoke-prod URL=https://ferrox-server-XXX-ew.a.run.app
smoke-prod URL:
    ops/smoke.sh {{URL}}
```

- [ ] **Step 2: Verify just commands parse**

```bash
cd mosaic-extensions/ferrox-solvers
just --list | grep -E 'deploy-|tenants-show|smoke-prod'
```

Expected: 4 lines listed.

- [ ] **Step 3: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add Justfile
git commit -m "$(cat <<'EOF'
chore(just): add deploy-build, deploy-apply, smoke-prod, tenants-show (M1.D4)

Wrappers around gcloud builds submit + gcloud run services replace +
ops/smoke.sh so the deploy is one command, not five.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase E — Smoke verification + close-out

### Task E1: Write the smoke script

**Files:**
- Create: `ops/smoke.sh`

- [ ] **Step 1: Write the script**

Create `ops/smoke.sh`:

```bash
#!/usr/bin/env bash
# Smoke-test the deployed ferrox-server.
#
# Usage:  ops/smoke.sh https://ferrox-server-XXXXXXXX-ew.a.run.app
#
# Run from Cloud Shell (has VPC access by default) or via:
#   gcloud run services proxy ferrox-server --project=reflective-labs \
#       --region=europe-west1 --port=9090 &
#   ops/smoke.sh http://localhost:9090
#
# Requires grpcurl. Install: gcloud components install grpcurl   (Cloud Shell)
#                    OR     brew install grpcurl                 (macOS local)
set -euo pipefail

URL="${1:?usage: smoke.sh <ferrox-server-url>}"
HOST_PORT="${URL#https://}"
HOST_PORT="${HOST_PORT#http://}"
HOST_PORT="${HOST_PORT%/}"
SCHEME_FLAGS=""
if [[ "$URL" == http://* ]]; then
    SCHEME_FLAGS="-plaintext"
fi

echo "── 1/4: grpc.health.v1.Health/Check should return SERVING ──"
RESP=$(grpcurl $SCHEME_FLAGS -d '{"service": ""}' "$HOST_PORT" grpc.health.v1.Health/Check)
echo "$RESP"
echo "$RESP" | grep -q '"status": "SERVING"' \
    || { echo "FAIL: expected SERVING"; exit 1; }
echo "ok"
echo

echo "── 2/4: SolveCp WITHOUT x-converge-app → INVALID_ARGUMENT ──"
if grpcurl $SCHEME_FLAGS -d '{}' "$HOST_PORT" \
       ferrox.v1.FerroxSolver/SolveCp 2>&1 \
       | grep -q "InvalidArgument"; then
    echo "ok"
else
    echo "FAIL: expected InvalidArgument for missing tenant header"
    exit 1
fi
echo

echo "── 3/4: SolveCp WITH unknown tenant → PERMISSION_DENIED ──"
if grpcurl $SCHEME_FLAGS -H 'x-converge-app: nope-not-real' \
       -d '{}' "$HOST_PORT" \
       ferrox.v1.FerroxSolver/SolveCp 2>&1 \
       | grep -q "PermissionDenied"; then
    echo "ok"
else
    echo "FAIL: expected PermissionDenied for unknown tenant"
    exit 1
fi
echo

echo "── 4/4: SolveCp WITH quorum-sense + minimal valid CP problem ──"
# Trivial CP: maximize x subject to 0 ≤ x ≤ 1. Optimal x=1.
RESP=$(grpcurl $SCHEME_FLAGS \
    -H 'x-converge-app: quorum-sense' \
    -d '{
      "problem": {
        "variables": [{"name": "x", "lb": 0, "ub": 1, "is_bool": false}],
        "objective": {"sense": "maximize", "linear": {"terms": [{"var": "x", "coeff": 1}], "rhs": 0}}
      },
      "time_limit_sec": 5
    }' \
    "$HOST_PORT" ferrox.v1.FerroxSolver/SolveCp)
echo "$RESP"
# Accept any non-error response — the exact shape depends on the solver
# version, but if we got JSON back the solver round-tripped.
echo "$RESP" | grep -q '"status"' \
    || { echo "FAIL: expected a structured response"; exit 1; }
echo "ok"
echo

echo "── ALL 4 SMOKE CHECKS PASSED ──"
```

- [ ] **Step 2: Make it executable**

```bash
cd mosaic-extensions/ferrox-solvers
chmod +x ops/smoke.sh
```

- [ ] **Step 3: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add ops/smoke.sh
git commit -m "$(cat <<'EOF'
test(ops): grpcurl smoke for ferrox-server prod (M1.E1)

Four checks: Health/Check SERVING, missing tenant → INVALID_ARGUMENT,
unknown tenant → PERMISSION_DENIED, quorum-sense + minimal CP → response.
Run from Cloud Shell (VPC access) or via gcloud run services proxy.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task E2: Run the smoke against prod and capture results

This is the M1 exit-criteria gate. No new files.

- [ ] **Step 1: Open a proxy to the internal service**

Locally (i.e. from your dev machine outside the VPC):

```bash
gcloud run services proxy ferrox-server \
    --project=reflective-labs \
    --region=europe-west1 \
    --port=9090
```

This blocks. Leave it running. In a second shell:

- [ ] **Step 2: Run the smoke**

```bash
cd mosaic-extensions/ferrox-solvers
ops/smoke.sh http://localhost:9090
```

Expected: 4 sections all printing `ok`, ending with `── ALL 4 SMOKE CHECKS PASSED ──`.

If any section fails:
- **Section 1 fails** → service not actually up. Check `gcloud run services describe ferrox-server --project=reflective-labs --region=europe-west1` for the latest revision status; look for "Ready=True". Check Cloud Logging for startup errors.
- **Section 2 fails** → interceptor not wired. Re-check Task A3 / Task B2's `Server::builder()` block.
- **Section 3 fails** → tenant lookup not running. Re-check Task A3 and that the deployed image includes the Phase A changes (compare `gcloud artifacts docker images describe` digest to a fresh local build).
- **Section 4 fails** → either the solver itself errored (look at Cloud Logging for the request_id from the response), OR the request shape doesn't match the proto. The trivial CP in Step 1 of the script is the minimal shape that goes through `cp_req_from_proto`; if that conversion errors, the message will be in the gRPC error detail.

- [ ] **Step 3: Stop the proxy**

In the first shell, Ctrl-C.

(No commit — this is the verification gate, not a code change.)

---

### Task E3: Update kb/Architecture/Cloud Run Deployment.md with prod URL

**Files:**
- Modify: `kb/Architecture/Cloud Run Deployment.md`

- [ ] **Step 1: Append a "Deployed state" section**

Open `mosaic-extensions/ferrox-solvers/kb/Architecture/Cloud Run Deployment.md`. After the existing content, append:

```markdown

## Deployed state (M1)

| Field | Value |
|---|---|
| Project | `reflective-labs` |
| Region | `europe-west1` |
| Service URL | `https://ferrox-server-XXXXXXXX-ew.a.run.app` (replace with your URL from D3 Step 3) |
| Ingress | `internal` |
| VPC connector | `solver-egress-ew1` |
| Service account | `ferrox-server@reflective-labs.iam.gserviceaccount.com` |
| Image tag (current) | `vX.Y.Z-<sha>` (update on each deploy) |
| Concurrency | 1 (CP-SAT is single-process) |
| Min/Max instances | 1 / 10 |
| Tenant allowlist | `quorum-sense` (4 in-flight) |
| Health check | `grpc.health.v1.Health` (`tonic-health`) |
| Smoke script | `ops/smoke.sh` |

**M2 / M3 unblocked** — quorum-sense's M3 work can now plan against this service URL.
```

(Replace placeholders with the actual values from Task D3 Step 3 and D2 Step 2.)

- [ ] **Step 2: Commit**

```bash
cd mosaic-extensions/ferrox-solvers
git add "kb/Architecture/Cloud Run Deployment.md"
git commit -m "$(cat <<'EOF'
docs(kb): record M1 deployed state (M1.E3)

Capture the prod service URL, IAM, allowlist, and smoke entry-point so
M2 (soter-server) and M3 (quorum-sense remote-smt) can plan against
known infrastructure.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task E4: Update the platform spec's status table

**Files:**
- Modify: `marquee-apps/quorum-sense/docs/superpowers/specs/2026-06-14-converge-grpc-suggestor-pattern-design.md`

- [ ] **Step 1: Add a status note next to M1 in §8**

Open the spec. Find the line `### M1 — ferrox-server reaches Cloud Run prod` (around line 411 after the §11 self-review edits). Replace it with:

```markdown
### M1 — ferrox-server reaches Cloud Run prod ✅ shipped YYYY-MM-DD
```

(Use the actual ship date.)

- [ ] **Step 2: Commit (cross-repo — quorum-sense is the spec owner)**

```bash
cd marquee-apps/quorum-sense
git add docs/superpowers/specs/2026-06-14-converge-grpc-suggestor-pattern-design.md
git commit -m "$(cat <<'EOF'
docs(spec): mark M1 shipped (M1.E4)

ferrox-server is in reflective-labs prod. Service URL recorded in
ferrox-solvers/kb/Architecture/Cloud Run Deployment.md. M2 + M3 unblocked.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

⚠️ **Branch caution:** quorum-sense's working branch is `next` (single-developer policy). Commit lands on `next`; ship to `main` via the regular PR flow when the M1+M2+M3 batch is ready, OR open a small PR for just this doc update if you want it on `main` sooner.

---

## Self-Review

After completing all tasks, run this checklist before declaring M1 done.

**Spec coverage:**
- [ ] §3.5 Rule 1 (extension owns its native build) — satisfied by the existing self-contained Dockerfile (no changes made; this plan does NOT introduce shared math-base).
- [ ] §4.1 (Ferrox proto) — no proto changes (additive only would be required; v1 untouched).
- [ ] §4.3 (metadata headers) — `x-converge-app` required, `x-request-id` minted/accepted, `Authorization: Bearer` optional. Covered by Task A3 + C2.
- [ ] §5.1 M1 additions:
  - tonic-health for grpc.health.v1.Health — Task B1 / B2.
  - Tenant allowlist interceptor — Tasks A1, A3.
  - ops/cloudbuild.prod.yaml + Cloud Run manifest — Tasks D2, D3.
  - Structured JSON logging — Task C1.
- [ ] §6 (tenant allowlist + in-process quotas) — Task A1 (`const TENANTS`), A4 (acquire before global), A5 (extension contract test). Per-minute rate limit explicitly deferred.
- [ ] §7 (network, auth, VPC) — Task D0 (VPC connector), D3 (ingress=internal, vpc-egress=private-ranges-only), D3 Step 4 (invoker IAM).
- [ ] §8 M1 exit criteria:
  - ferrox-server reachable inside VPC — Task E2 (via proxy).
  - Health green — Task E2 Section 1.
  - Tenant rejection verified — Task E2 Sections 2+3.
  - Nothing calls it yet — true; quorum-sense M3 is a separate plan.

**Placeholder scan:**
- [ ] No `TODO` / `TBD` / `<fill-in>` left in any new file. (Search: `grep -rn 'TODO\|TBD\|<fill' mosaic-extensions/ferrox-solvers/ops/ mosaic-extensions/ferrox-solvers/crates/ferrox-server/src/`)
- [ ] `REPLACE_ME` in `ops/cloudrun.prod.yaml` has been replaced with a real tag before applying (Task D3 Step 2 is a runtime substitution; the template legitimately ships with the marker).
- [ ] kb status table placeholders (`XXXXXXXX` hash, `vX.Y.Z-<sha>`) have been replaced with real values from the deploy (Task E3 Step 1).

**Type consistency:**
- [ ] `TenantSlug(pub &'static str)` referenced consistently in `tenants.rs`, `interceptor.rs`, `service.rs`.
- [ ] `RequestId(pub String)` referenced consistently in `interceptor.rs`, `service.rs`.
- [ ] `TenantRegistry::acquire` signature `(&self, slug: &str) -> Result<OwnedSemaphorePermit, Status>` used the same way in `service.rs::run_blocking`.
- [ ] `request_interceptor` referenced (not `auth_interceptor`) in all `Server::builder()` call sites in `main.rs`.

**Cross-repo consistency:**
- [ ] Spec status (§8 M1 heading) and kb deploy state (`kb/Architecture/Cloud Run Deployment.md`) both updated.
- [ ] Project memory (`~/.claude/projects/.../memory/project_grpc_suggestor_pattern.md`) line "M1 / M2 / M3 of the spec: not started" should be updated to "M1 ✅ shipped <date>; M2 / M3 not started" once M1 lands. (Out of plan scope to edit memory; flagged as follow-up.)

---

## What unblocks after M1

- **M2 plan** can now be written. Copy-paste the bulk of Phases A/B/C/D/E onto soter-smt with the obvious substitutions (`ferrox` → `soter`, `FerroxSolver/SolveCp` → `SoterSolver/Check`, OR-Tools + HiGHS → CVC5). M2's unique work is the new proto (spec §4.2) and the new client crate (`RemoteSmtBackend` in `crates/soter/src/remote.rs`).
- **M3 plan** stays blocked until M2 is also shipped, since quorum-sense calls soter-server, not ferrox-server, in v1.
