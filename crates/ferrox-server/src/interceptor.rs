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
