use crate::RpcCommand;
use rsnano_node::config::Peer;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn work_peers() -> Self {
        Self::WorkPeers
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]

pub struct WorkPeersDto {
    pub work_peers: Vec<Peer>,
}

impl WorkPeersDto {
    pub fn new(work_peers: Vec<Peer>) -> Self {
        Self { work_peers }
    }
}
