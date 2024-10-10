use std::{net::Ipv6Addr, sync::Arc};
use rsnano_node::Node;

pub async fn work_peer_add(_node: Arc<Node>, _enable_control: bool, _address: Ipv6Addr, _port: u16) -> String {
    todo!("Distributed work feature is not implemented yet")
}
