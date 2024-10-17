use super::BlockInfoDto;
use crate::{common::HashesArgs, RpcCommand};
use rsnano_core::BlockHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

impl RpcCommand {
    pub fn blocks_info(hashes: Vec<BlockHash>) -> Self {
        Self::BlocksInfo(HashesArgs::new(hashes))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BlocksInfoDto {
    blocks: HashMap<BlockHash, BlockInfoDto>,
}

impl BlocksInfoDto {
    pub fn new(blocks: HashMap<BlockHash, BlockInfoDto>) -> Self {
        Self { blocks }
    }
}
