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
use converge_ferrox_server::interceptor::RequestId;
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
}

impl Default for FerroxSolverService {
    fn default() -> Self {
        Self::new(
            configured_blocking_solves(),
            Arc::new(TenantRegistry::default()),
        )
    }
}

fn configured_blocking_solves() -> usize {
    const DEFAULT_MAX_BLOCKING_SOLVES: usize = 1;

    match std::env::var("FERROX_SERVER_MAX_BLOCKING_SOLVES") {
        Ok(raw) => raw.parse::<usize>().unwrap_or_else(|err| {
            tracing::warn!(
                value = %raw,
                error = %err,
                default = DEFAULT_MAX_BLOCKING_SOLVES,
                "invalid FERROX_SERVER_MAX_BLOCKING_SOLVES"
            );
            DEFAULT_MAX_BLOCKING_SOLVES
        }),
        Err(_) => DEFAULT_MAX_BLOCKING_SOLVES,
    }
}

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

#[tonic::async_trait]
impl FerroxSolver for FerroxSolverService {
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
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        let req = cp_req_from_proto(request.into_inner())?;
        let result = self
            .run_blocking("solve_cp", tenant, move || solve_cp(&req))
            .await;
        let elapsed_us = started.elapsed().as_micros();
        match result {
            Ok(plan) => {
                span.in_scope(|| {
                    tracing::info!(
                        solve_duration_us = elapsed_us,
                        status = "ok",
                        "solve_cp completed"
                    );
                });
                Ok(Response::new(cp_resp_to_proto(plan)))
            }
            Err(err) => {
                span.in_scope(|| {
                    tracing::warn!(
                        solve_duration_us = elapsed_us,
                        status = "error",
                        code = %err.code(),
                        message = %err.message(),
                        "solve_cp failed"
                    );
                });
                Err(err)
            }
        }
    }

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
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        let req = lp_req_from_proto(request.into_inner())?;
        let result = self
            .run_blocking("solve_lp", tenant, move || solve_lp(&req))
            .await;
        let elapsed_us = started.elapsed().as_micros();
        match result {
            Ok(plan) => {
                span.in_scope(|| {
                    tracing::info!(
                        solve_duration_us = elapsed_us,
                        status = "ok",
                        "solve_lp completed"
                    );
                });
                Ok(Response::new(lp_resp_to_proto(plan)))
            }
            Err(err) => {
                span.in_scope(|| {
                    tracing::warn!(
                        solve_duration_us = elapsed_us,
                        status = "error",
                        code = %err.code(),
                        message = %err.message(),
                        "solve_lp failed"
                    );
                });
                Err(err)
            }
        }
    }

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
        let started = std::time::Instant::now();
        let tenant = ctx.tenant;
        let req = mip_req_from_proto(request.into_inner())?;
        let result = self
            .run_blocking("solve_mip", tenant, move || solve_mip(&req))
            .await;
        let elapsed_us = started.elapsed().as_micros();
        match result {
            Ok(plan) => {
                span.in_scope(|| {
                    tracing::info!(
                        solve_duration_us = elapsed_us,
                        status = "ok",
                        "solve_mip completed"
                    );
                });
                Ok(Response::new(mip_resp_to_proto(plan)))
            }
            Err(err) => {
                span.in_scope(|| {
                    tracing::warn!(
                        solve_duration_us = elapsed_us,
                        status = "error",
                        code = %err.code(),
                        message = %err.message(),
                        "solve_mip failed"
                    );
                });
                Err(err)
            }
        }
    }
}
