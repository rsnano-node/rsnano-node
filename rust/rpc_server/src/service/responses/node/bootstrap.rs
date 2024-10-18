use rsnano_node::{bootstrap::BootstrapInitiatorExt, Node};
use rsnano_rpc_messages::{BootstrapArgs, RpcDto, SuccessDto};
use std::{net::SocketAddrV6, sync::Arc};

pub async fn bootstrap(node: Arc<Node>, args: BootstrapArgs) -> RpcDto {
    let id = args.id.unwrap_or(String::new());
    let endpoint = SocketAddrV6::new(args.address, args.port, 0, 0);
    node.bootstrap_initiator.bootstrap2(endpoint, id);

    RpcDto::Bootstrap(SuccessDto::new())
}
