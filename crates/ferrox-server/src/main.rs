mod convert;
mod service;
// Re-import from the library target so integration tests share the same module path.
// Unused for now; wired in by M1.A3 when the tenant interceptor lands.
#[allow(unused_imports)]
use converge_ferrox_server::tenants;

pub mod proto {
    pub mod ferrox {
        #[allow(
            clippy::doc_markdown,
            clippy::default_trait_access,
            clippy::too_many_lines,
            clippy::large_enum_variant
        )]
        pub mod v1 {
            tonic::include_proto!("ferrox.v1");
        }
    }
}

use std::net::SocketAddr;

use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic::{Request, Status};

use proto::ferrox::v1::ferrox_solver_server::FerroxSolverServer;
use service::FerroxSolverService;

#[allow(clippy::result_large_err)]
fn auth_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let expected = std::env::var("FERROX_AUTH_TOKEN").ok();
    let Some(token) = expected else {
        return Ok(req); // auth disabled when env var is absent
    };
    let provided = req
        .metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided == format!("Bearer {token}") {
        Ok(req)
    } else {
        Err(Status::unauthenticated("invalid or missing token"))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrox_server=info".parse().unwrap()),
        )
        .init();

    let addr: SocketAddr = std::env::var("FERROX_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

    let svc = FerroxSolverService::default();

    let cert_path = std::env::var("FERROX_TLS_CERT").unwrap_or_else(|_| "/tls/server.crt".into());
    let key_path = std::env::var("FERROX_TLS_KEY").unwrap_or_else(|_| "/tls/server.key".into());

    let use_tls =
        std::path::Path::new(&cert_path).exists() && std::path::Path::new(&key_path).exists();

    tracing::info!(addr = %addr, tls = use_tls, "ferrox-server starting");

    if use_tls {
        let cert = std::fs::read(&cert_path)?;
        let key = std::fs::read(&key_path)?;
        let identity = Identity::from_pem(cert, key);
        let mut tls = ServerTlsConfig::new().identity(identity);

        if let Ok(ca_path) = std::env::var("FERROX_TLS_CA") {
            let ca = std::fs::read(ca_path)?;
            tls = tls.client_ca_root(tonic::transport::Certificate::from_pem(ca));
        }

        Server::builder()
            .tls_config(tls)?
            .add_service(FerroxSolverServer::with_interceptor(svc, auth_interceptor))
            .serve(addr)
            .await?;
    } else {
        tracing::warn!("TLS cert/key not found — starting without TLS (dev/test only)");
        Server::builder()
            .add_service(FerroxSolverServer::with_interceptor(svc, auth_interceptor))
            .serve(addr)
            .await?;
    }

    Ok(())
}
