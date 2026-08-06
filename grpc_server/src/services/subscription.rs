use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use rsnano_grpc_proto::nano::v1::{
    ConfirmationEvent, ElectionEvent, SubscribeActiveElectionsRequest,
    SubscribeConfirmationsRequest, SubscribeTelemetryRequest, TelemetryEvent,
    subscription_service_server::SubscriptionService,
};
use rsnano_node::Node;
use rsnano_types::NodeId;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

pub struct SubscriptionServiceImpl {
    pub node: Arc<Node>,
    pub max_lag: usize,
    pub send_timeout_ms: u64,
}

#[tonic::async_trait]
impl SubscriptionService for SubscriptionServiceImpl {
    type SubscribeConfirmationsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<ConfirmationEvent, Status>> + Send>>;
    type SubscribeTelemetryStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<TelemetryEvent, Status>> + Send>>;
    type SubscribeActiveElectionsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<ElectionEvent, Status>> + Send>>;

    async fn subscribe_confirmations(
        &self,
        request: Request<SubscribeConfirmationsRequest>,
    ) -> Result<Response<Self::SubscribeConfirmationsStream>, Status> {
        let _req = request.into_inner();
        let (_tx, rx) = mpsc::channel::<Result<ConfirmationEvent, Status>>(self.max_lag);

        // TODO: Bridge from NodeEvent::BlockConfirmed to the gRPC stream.
        // This requires registering a handler on the node's CompositeNodeEventHandler
        // during daemon startup and forwarding events to _tx.

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }

    async fn subscribe_telemetry(
        &self,
        _request: Request<SubscribeTelemetryRequest>,
    ) -> Result<Response<Self::SubscribeTelemetryStream>, Status> {
        let (tx, rx) = mpsc::channel::<Result<TelemetryEvent, Status>>(self.max_lag);
        let send_timeout = Duration::from_millis(self.send_timeout_ms);

        let node_weak = Arc::downgrade(&self.node);
        self.node
            .telemetry
            .on_telemetry_processed(Box::new(move |data, peer_addr| {
                if let Some(_node) = node_weak.upgrade() {
                    let event = TelemetryEvent {
                        block_count: data.block_count.to_string(),
                        cemented_count: data.cemented_count.to_string(),
                        unchecked_count: data.unchecked_count.to_string(),
                        account_count: data.account_count.to_string(),
                        bandwidth_cap: data.bandwidth_cap.to_string(),
                        peer_count: data.peer_count.to_string(),
                        protocol_version: data.protocol_version.to_string(),
                        uptime: data.uptime.to_string(),
                        genesis_block: data.genesis_block.to_string(),
                        major_version: data.major_version.to_string(),
                        minor_version: data.minor_version.to_string(),
                        patch_version: data.patch_version.to_string(),
                        pre_release_version: data.pre_release_version.to_string(),
                        maker: data.maker.to_string(),
                        timestamp: data
                            .timestamp
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        node_id: NodeId::from(data.node_id).to_string(),
                        signature: String::new(),
                        address: peer_addr.to_string(),
                    };

                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(send_timeout, tx.send(Ok(event))).await;
                    });
                }
            }));

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }

    async fn subscribe_active_elections(
        &self,
        _request: Request<SubscribeActiveElectionsRequest>,
    ) -> Result<Response<Self::SubscribeActiveElectionsStream>, Status> {
        let (_tx, rx) = mpsc::channel::<Result<ElectionEvent, Status>>(self.max_lag);

        // TODO: Bridge from NodeEvent::ElectionStarted/ElectionStopped to the gRPC stream.
        // This requires registering a handler on the node's CompositeNodeEventHandler
        // during daemon startup and forwarding events to _tx.

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}
