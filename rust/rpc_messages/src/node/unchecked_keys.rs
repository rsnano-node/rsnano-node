use crate::RpcCommand;
use rsnano_core::HashOrAccount;
use rsnano_core::{BlockHash, JsonBlock};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn unchecked_keys(key: HashOrAccount, count: u64) -> Self {
        Self::UncheckedKeys(UncheckedKeysArgs { key, count })
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UncheckedKeysArgs {
    pub key: HashOrAccount,
    pub count: u64,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UncheckedKeysDto {
    pub unchecked: Vec<UncheckedKeyDto>,
}

impl UncheckedKeysDto {
    pub fn new(unchecked: Vec<UncheckedKeyDto>) -> Self {
        Self { unchecked }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UncheckedKeyDto {
    pub key: BlockHash,
    pub hash: BlockHash,
    pub modified_timestamp: u64,
    pub contents: JsonBlock,
}

impl UncheckedKeyDto {
    pub fn new(
        key: BlockHash,
        hash: BlockHash,
        modified_timestamp: u64,
        contents: JsonBlock,
    ) -> Self {
        Self {
            key,
            hash,
            modified_timestamp,
            contents,
        }
    }
}
