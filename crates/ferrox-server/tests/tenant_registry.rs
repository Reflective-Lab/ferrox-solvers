//! Integration tests for the tenant allowlist + per-tenant semaphore.
//!
//! The interceptor unit tests below mutate the `FERROX_AUTH_TOKEN` env var.
//! Rust 1.85+ (edition 2024) makes `std::env::set_var` / `remove_var`
//! `unsafe`, and the workspace lint forbids `unsafe_code` even in tests.
//! We use the `temp-env` crate, which serializes env mutations behind a
//! global mutex and keeps the unsafe internalized to that crate.
//! `serial_test` is layered on top to keep the scoped guards from racing
//! with each other across tokio's worker threads.

use converge_ferrox_server::tenants::{Tenant, TenantRegistry};
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
    let cap = Tenant::cap_for("quorum-sense").expect("seeded") as usize;
    let mut held = Vec::with_capacity(cap);
    for _ in 0..cap {
        held.push(reg.acquire("quorum-sense").await.expect("under cap"));
    }
    let err = reg.acquire("quorum-sense").await.expect_err("over cap");
    assert_eq!(err.code(), Code::ResourceExhausted);
    assert!(err.message().contains("at in-flight cap"));
}

// ─── End-to-end interceptor + service path ──────────────────────────────

use converge_ferrox_server::tenants::TenantSlug;
use tonic::Request;

/// Simulate what the interceptor does: attach `TenantSlug` to extensions.
fn with_tenant<T>(mut req: Request<T>, slug: &'static str) -> Request<T> {
    req.extensions_mut().insert(TenantSlug(slug));
    req
}

#[tokio::test]
async fn extensions_carry_tenant_slug_into_service() {
    let req = with_tenant(Request::new(()), "quorum-sense");
    let slug = req
        .extensions()
        .get::<TenantSlug>()
        .map(|t| t.0)
        .expect("tenant attached");
    assert_eq!(slug, "quorum-sense");
}

// ─── request_interceptor unit tests ─────────────────────────────────────

use converge_ferrox_server::interceptor::request_interceptor;
use serial_test::serial;
use tonic::metadata::MetadataValue;

const AUTH_ENV: &str = "FERROX_AUTH_TOKEN";

/// Helper: build a Request<()> with a tenant header, no bearer.
fn req_with_tenant(slug: &str) -> Request<()> {
    let mut req = Request::new(());
    let value: MetadataValue<_> = slug.parse().expect("ascii slug");
    req.metadata_mut().insert("x-converge-app", value);
    req
}

#[test]
#[serial(env)]
fn interceptor_attaches_tenant_slug_on_known_tenant() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let req = req_with_tenant("quorum-sense");
        let req = request_interceptor(req).expect("known tenant accepted");
        let attached = req
            .extensions()
            .get::<TenantSlug>()
            .map(|t| t.0)
            .expect("interceptor attaches slug");
        assert_eq!(attached, "quorum-sense");
    });
}

#[test]
#[serial(env)]
fn interceptor_rejects_missing_tenant_header() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let req = Request::new(());
        let err = request_interceptor(req).expect_err("missing header → INVALID_ARGUMENT");
        assert_eq!(err.code(), Code::InvalidArgument);
        assert!(err.message().contains("missing x-converge-app"));
    });
}

#[test]
#[serial(env)]
fn interceptor_rejects_unknown_tenant() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let req = req_with_tenant("nope-not-real");
        let err = request_interceptor(req).expect_err("unknown tenant → PERMISSION_DENIED");
        assert_eq!(err.code(), Code::PermissionDenied);
        assert!(err.message().contains("unknown tenant"));
    });
}

#[test]
#[serial(env)]
fn interceptor_bypasses_bearer_when_env_unset() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let req = req_with_tenant("quorum-sense");
        let result = request_interceptor(req);
        assert!(result.is_ok(), "bearer disabled when env unset");
    });
}

#[test]
#[serial(env)]
fn interceptor_rejects_bad_bearer_when_env_set() {
    temp_env::with_var(AUTH_ENV, Some("expected-token"), || {
        let mut req = req_with_tenant("quorum-sense");
        let auth: MetadataValue<_> = "Bearer wrong-token".parse().expect("ascii");
        req.metadata_mut().insert("authorization", auth);
        let err = request_interceptor(req).expect_err("bad bearer → UNAUTHENTICATED");
        assert_eq!(err.code(), Code::Unauthenticated);
    });
}

#[test]
#[serial(env)]
fn interceptor_accepts_good_bearer_when_env_set() {
    temp_env::with_var(AUTH_ENV, Some("expected-token"), || {
        let mut req = req_with_tenant("quorum-sense");
        let auth: MetadataValue<_> = "Bearer expected-token".parse().expect("ascii");
        req.metadata_mut().insert("authorization", auth);
        let result = request_interceptor(req);
        assert!(result.is_ok(), "good bearer accepted");
    });
}

// ─── RequestId mint / pass-through ──────────────────────────────────────

use converge_ferrox_server::interceptor::RequestId;
use uuid::Uuid;

#[test]
#[serial(env)]
fn interceptor_mints_request_id_when_header_absent() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let req = req_with_tenant("quorum-sense");
        let req = request_interceptor(req).expect("known tenant accepted");
        let id = req
            .extensions()
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .expect("interceptor attaches request id");
        assert!(!id.is_empty(), "minted id must be non-empty");
        Uuid::parse_str(&id).expect("minted id must be a valid UUID");
    });
}

#[test]
#[serial(env)]
fn interceptor_accepts_client_supplied_request_id() {
    temp_env::with_var_unset(AUTH_ENV, || {
        let mut req = req_with_tenant("quorum-sense");
        let value: MetadataValue<_> = "client-supplied-id-12345".parse().expect("ascii");
        req.metadata_mut().insert("x-request-id", value);
        let req = request_interceptor(req).expect("known tenant accepted");
        let id = req
            .extensions()
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .expect("interceptor attaches request id");
        assert_eq!(id, "client-supplied-id-12345");
    });
}
