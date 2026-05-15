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

#[derive(Clone)]
pub struct FerroxSolverService {
    solve_limit: Arc<Semaphore>,
}

impl FerroxSolverService {
    pub fn new(max_blocking_solves: usize) -> Self {
        Self {
            solve_limit: Arc::new(Semaphore::new(max_blocking_solves.max(1))),
        }
    }

    async fn run_blocking<T, F>(&self, operation: &'static str, solve: F) -> Result<T, Status>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
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
        Self::new(configured_blocking_solves())
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

#[tonic::async_trait]
impl FerroxSolver for FerroxSolverService {
    async fn solve_cp(
        &self,
        request: Request<SolveCpRequest>,
    ) -> Result<Response<SolveCpResponse>, Status> {
        let req = cp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_cp", move || solve_cp(&req))
            .await?;
        Ok(Response::new(cp_resp_to_proto(plan)))
    }

    async fn solve_lp(
        &self,
        request: Request<SolveLpRequest>,
    ) -> Result<Response<SolveLpResponse>, Status> {
        let req = lp_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_lp", move || solve_lp(&req))
            .await?;
        Ok(Response::new(lp_resp_to_proto(plan)))
    }

    async fn solve_mip(
        &self,
        request: Request<SolveMipRequest>,
    ) -> Result<Response<SolveMipResponse>, Status> {
        let req = mip_req_from_proto(request.into_inner())?;
        let plan = self
            .run_blocking("solve_mip", move || solve_mip(&req))
            .await?;
        Ok(Response::new(mip_resp_to_proto(plan)))
    }
}
