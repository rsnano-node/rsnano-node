use std::sync::Arc;

use anyhow::Context;
use rsnano_grpc_proto::nano::v1::{
    account_service_server::AccountServiceServer, block_service_server::BlockServiceServer,
    ledger_service_server::LedgerServiceServer, network_service_server::NetworkServiceServer,
    node_service_server::NodeServiceServer, subscription_service_server::SubscriptionServiceServer,
};
use rsnano_node::Node;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tracing::info;

use crate::GrpcServerConfig;
use crate::interceptor::ApiKeyInterceptor;
use crate::services::{
    AccountServiceImpl, BlockServiceImpl, LedgerServiceImpl, NetworkServiceImpl, NodeServiceImpl,
    SubscriptionServiceImpl,
};

pub async fn run_grpc_server(
    node: Arc<Node>,
    config: GrpcServerConfig,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let addr = config.listening_addr()?;
    info!("gRPC listening address: {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve_grpc_server(node, config, listener, shutdown).await
}

async fn serve_grpc_server(
    node: Arc<Node>,
    config: GrpcServerConfig,
    listener: tokio::net::TcpListener,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let api_key_interceptor = ApiKeyInterceptor::new(config.api_keys.clone());

    let account_service = AccountServiceImpl { node: node.clone() };
    let block_service = BlockServiceImpl { node: node.clone() };
    let ledger_service = LedgerServiceImpl { node: node.clone() };
    let network_service = NetworkServiceImpl { node: node.clone() };
    let node_service = NodeServiceImpl { node: node.clone() };
    let subscription_service = SubscriptionServiceImpl::new(
        node.clone(),
        config.stream_max_lag,
        config.stream_send_timeout_ms,
    );

    let mut server = Server::builder().tcp_nodelay(true);
    if config.enable_tls {
        server = server
            .tls_config(load_tls_config(&config).await?)
            .context("invalid gRPC TLS certificate or private key")?;
    }

    let mut router = server
        .layer(tonic::service::InterceptorLayer::new(
            api_key_interceptor.interceptor(),
        ))
        .add_service(AccountServiceServer::new(account_service))
        .add_service(BlockServiceServer::new(block_service))
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(NetworkServiceServer::new(network_service))
        .add_service(NodeServiceServer::new(node_service))
        .add_service(SubscriptionServiceServer::new(subscription_service));

    if config.enable_reflection {
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(rsnano_grpc_proto::nano::v1::FILE_DESCRIPTOR_SET)
            .build_v1()?;
        router = router.add_service(reflection);
    }

    router
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            shutdown,
        )
        .await?;

    info!("gRPC server stopped");
    Ok(())
}

async fn load_tls_config(config: &GrpcServerConfig) -> anyhow::Result<ServerTlsConfig> {
    let cert_path = config
        .tls_cert_path
        .as_deref()
        .context("enable_tls requires tls_cert_path")?;
    let key_path = config
        .tls_key_path
        .as_deref()
        .context("enable_tls requires tls_key_path")?;

    let cert = tokio::fs::read(cert_path)
        .await
        .with_context(|| format!("failed to read gRPC TLS certificate: {cert_path}"))?;
    let key = tokio::fs::read(key_path)
        .await
        .with_context(|| format!("failed to read gRPC TLS private key: {key_path}"))?;

    Ok(ServerTlsConfig::new().identity(Identity::from_pem(cert, key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_grpc_proto::nano::v1::{VersionRequest, node_service_client::NodeServiceClient};
    use tempfile::tempdir;
    use tokio::sync::oneshot;
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

    const TEST_CERT: &[u8] = include_bytes!("../testdata/tls-cert.pem");
    const TEST_KEY: &[u8] = include_bytes!("../testdata/tls-key.pem");

    #[tokio::test]
    async fn tls_requires_certificate_and_key_paths() {
        let config = GrpcServerConfig {
            enable_tls: true,
            ..GrpcServerConfig::new()
        };

        let error = load_tls_config(&config).await.unwrap_err();

        assert_eq!(error.to_string(), "enable_tls requires tls_cert_path");
    }

    #[tokio::test]
    async fn tls_requires_key_path_when_certificate_is_configured() {
        let config = GrpcServerConfig {
            enable_tls: true,
            tls_cert_path: Some("cert.pem".to_string()),
            ..GrpcServerConfig::new()
        };

        let error = load_tls_config(&config).await.unwrap_err();

        assert_eq!(error.to_string(), "enable_tls requires tls_key_path");
    }

    #[tokio::test]
    async fn server_accepts_tls_connections_when_enabled() {
        let directory = tempdir().unwrap();
        let cert_path = directory.path().join("cert.pem");
        let key_path = directory.path().join("key.pem");
        std::fs::write(&cert_path, TEST_CERT).unwrap();
        std::fs::write(&key_path, TEST_KEY).unwrap();

        let config = GrpcServerConfig {
            address: "127.0.0.1".to_string(),
            port: 0,
            enable_tls: true,
            tls_cert_path: Some(cert_path.to_string_lossy().into_owned()),
            tls_key_path: Some(key_path.to_string_lossy().into_owned()),
            enable_reflection: false,
            ..GrpcServerConfig::new()
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let node = Arc::new(Node::new_null());
        let (shutdown, wait_for_shutdown) = oneshot::channel();
        let server = tokio::spawn(serve_grpc_server(node, config, listener, async move {
            let _ = wait_for_shutdown.await;
        }));

        let tls = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(TEST_CERT))
            .domain_name("localhost");
        let channel = Endpoint::from_shared(format!("https://{address}"))
            .unwrap()
            .tls_config(tls)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let response = NodeServiceClient::new(channel)
            .version(VersionRequest {})
            .await
            .unwrap();

        shutdown.send(()).unwrap();
        server.await.unwrap().unwrap();
        assert_eq!(response.into_inner().rpc_version, "1");
    }

    #[tokio::test]
    async fn bind_failure_is_returned() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let config = GrpcServerConfig {
            address: address.ip().to_string(),
            port: address.port(),
            ..GrpcServerConfig::new()
        };

        let error = run_grpc_server(Arc::new(Node::new_null()), config, std::future::pending())
            .await
            .unwrap_err();

        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AddrInUse
        );
    }
}
