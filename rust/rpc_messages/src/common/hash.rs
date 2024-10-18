use rsnano_core::BlockHash;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HashRpcMessage {
    pub hash: BlockHash,
}

impl HashRpcMessage {
    pub fn new(hash: BlockHash) -> Self {
        Self { hash }
    }
}
