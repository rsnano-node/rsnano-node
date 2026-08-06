use std::sync::Arc;

use rsnano_grpc_proto::nano::v1::{
    KeepaliveRequest, KeepaliveResponse, StatusRequest, StatusResponse, VersionRequest,
    VersionResponse, node_service_server::NodeService,
};
use rsnano_node::Node;
use tonic::{Request, Response, Status};

pub struct NodeServiceImpl {
    pub node: Arc<Node>,
}

#[tonic::async_trait]
impl NodeService for NodeServiceImpl {
    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let network = self.node.network.read().unwrap();
        let peer_count = network.channels().count() as u64;

        Ok(Response::new(StatusResponse {
            version: "1".to_string(),
            synced: true,
            bootstrap_storage: false,
            block_count: 0,
            cemented_count: "0".to_string(),
            unchecked_count: 0,
            account_count: 0,
            bandwidth_cap: 0,
            peer_count,
            protocol_version: self
                .node
                .network_params
                .network
                .protocol_version
                .to_string(),
            uptime: 0,
            genesis_block: String::new(),
            major_version: "0".to_string(),
            minor_version: "0".to_string(),
            patch_version: "0".to_string(),
            pre_release_version: "0".to_string(),
            maker: "RsNano".to_string(),
            timestamp: 0,
            node_id: rsnano_types::NodeId::from(self.node.node_id.public_key()).to_string(),
            signature: String::new(),
            active_difficulty: false,
        }))
    }

    async fn version(
        &self,
        _request: Request<VersionRequest>,
    ) -> Result<Response<VersionResponse>, Status> {
        Ok(Response::new(VersionResponse {
            rpc_version: "1".to_string(),
            store_version: "0".to_string(),
            protocol_version: self
                .node
                .network_params
                .network
                .protocol_version
                .to_string(),
            node_vendor: "RsNano 2.0.0".to_string(),
            store_vendor: 0,
            network: self
                .node
                .network_params
                .network
                .current_network
                .as_str()
                .to_string(),
            network_identifier: self
                .node
                .network_params
                .network
                .current_network
                .as_str()
                .to_string(),
            build_info: String::new(),
        }))
    }

    async fn keepalive(
        &self,
        request: Request<KeepaliveRequest>,
    ) -> Result<Response<KeepaliveResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Keepalive request to {}:{}", req.address, req.port);
        Ok(Response::new(KeepaliveResponse {}))
    }
}
