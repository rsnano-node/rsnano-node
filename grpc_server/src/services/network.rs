use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    PeerInfo, PeersRequest, PeersResponse, TelemetryRequest, TelemetryResponse,
    network_service_server::NetworkService,
};
use rsnano_node::Node;
use tonic::{Request, Response, Status};

pub struct NetworkServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl NetworkService for NetworkServiceImpl {
    async fn peers(
        &self,
        _request: Request<PeersRequest>,
    ) -> Result<Response<PeersResponse>, Status> {
        let network = self.node.network.read().unwrap();
        let mut peers = std::collections::HashMap::new();

        for channel in network.channels() {
            let peer_addr = channel.peer_addr().to_string();
            peers.insert(
                peer_addr,
                PeerInfo {
                    protocol_version: channel.protocol_version().to_string(),
                    node_id: channel
                        .node_id()
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    r#type: "tcp".to_string(),
                    account: String::new(),
                    weight: String::new(),
                },
            );
        }

        Ok(Response::new(PeersResponse { peers }))
    }

    async fn telemetry(
        &self,
        _request: Request<TelemetryRequest>,
    ) -> Result<Response<TelemetryResponse>, Status> {
        let network = self.node.network.read().unwrap();
        let peer_count = network.channels().count();
        let unchecked_count = self.node.unchecked.lock().unwrap().len();

        Ok(Response::new(TelemetryResponse {
            block_count: self.node.ledger.block_count().to_string(),
            cemented_count: self.node.ledger.confirmed_count().to_string(),
            unchecked_count: unchecked_count.to_string(),
            account_count: self.node.ledger.account_count().to_string(),
            bandwidth_cap: "0".to_string(),
            peer_count: peer_count.to_string(),
            protocol_version: self
                .node
                .network_params
                .network
                .protocol_version
                .to_string(),
            uptime: "0".to_string(),
            genesis_block: self
                .node
                .network_params
                .ledger
                .genesis_block
                .hash()
                .to_string(),
            major_version: "0".to_string(),
            minor_version: "0".to_string(),
            patch_version: "0".to_string(),
            pre_release_version: "0".to_string(),
            maker: "RsNano".to_string(),
            timestamp: 0,
            node_id: rsnano_types::NodeId::from(self.node.node_id.public_key()).to_string(),
            signature: String::new(),
        }))
    }
}
