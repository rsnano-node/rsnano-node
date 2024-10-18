use rsnano_node::{Node, NodeExt};
use rsnano_rpc_messages::{ErrorDto, RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn search_receivable_all(node: Arc<Node>, enable_control: bool) -> RpcDto {
    if enable_control {
        node.search_receivable_all();
        RpcDto::SearchReceivableAll(SuccessDto::new())
    } else {
        RpcDto::Error(ErrorDto::RPCControlDisabled)
    }
}
