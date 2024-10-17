use crate::{common::CountRpcMessage, RpcCommand};
use rsnano_core::{BlockHash, JsonBlock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

impl RpcCommand {
    pub fn unchecked(count: u64) -> Self {
        Self::Unchecked(CountRpcMessage::new(count))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UncheckedDto {
    pub blocks: HashMap<BlockHash, JsonBlock>,
}

impl UncheckedDto {
    pub fn new(blocks: HashMap<BlockHash, JsonBlock>) -> Self {
        Self { blocks }
    }
}
