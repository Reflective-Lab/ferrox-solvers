//! Integration tests for the tenant allowlist + per-tenant semaphore.

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
