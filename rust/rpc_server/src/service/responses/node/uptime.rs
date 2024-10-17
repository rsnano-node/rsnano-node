use rsnano_node::Node;
use rsnano_rpc_messages::{RpcDto, UptimeDto};
use std::{sync::Arc, time::Instant};

pub async fn uptime(node: Arc<Node>) -> RpcDto {
    let seconds = Instant::now() - node.telemetry.startup_time;
    let uptime = UptimeDto::new(seconds.as_secs());
    RpcDto::Uptime(uptime)
}
