use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    account_service_server::AccountServiceServer, block_service_server::BlockServiceServer,
    ledger_service_server::LedgerServiceServer, network_service_server::NetworkServiceServer,
    node_service_server::NodeServiceServer, subscription_service_server::SubscriptionServiceServer,
};
use rsnano_node::Node;
use tonic::transport::Server;
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

    let api_key_interceptor = ApiKeyInterceptor::new(config.api_keys.clone());

    let account_service = AccountServiceImpl { node: node.clone() };
    let block_service = BlockServiceImpl { node: node.clone() };
    let ledger_service = LedgerServiceImpl { node: node.clone() };
    let network_service = NetworkServiceImpl { node: node.clone() };
    let node_service = NodeServiceImpl { node: node.clone() };
    let subscription_service = SubscriptionServiceImpl {
        node: node.clone(),
        max_lag: config.stream_max_lag,
        send_timeout_ms: config.stream_send_timeout_ms,
    };

    let mut router = Server::builder()
        .tcp_nodelay(true)
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

    let listener = tokio::net::TcpListener::bind(addr).await?;
    router
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            shutdown,
        )
        .await?;

    info!("gRPC server stopped");
    Ok(())
}
