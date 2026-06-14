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
