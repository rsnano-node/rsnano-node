use crate::RpcCommand;
use rsnano_core::{BlockHash, WorkNonce};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn work_validate(work: WorkNonce, hash: BlockHash) -> Self {
        Self::WorkValidate(WorkValidateArgs::new(work, hash))
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorkValidateArgs {
    pub work: WorkNonce,
    pub hash: BlockHash,
}

impl WorkValidateArgs {
    pub fn new(work: WorkNonce, hash: BlockHash) -> Self {
        Self { work, hash }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkValidateDto {
    pub valid_all: bool,
    pub valid_receive: bool,
    pub difficulty: u64,
    pub multiplier: f64,
}

impl WorkValidateDto {
    pub fn new(valid_all: bool, valid_receive: bool, difficulty: u64, multiplier: f64) -> Self {
        Self {
            valid_all,
            valid_receive,
            difficulty,
            multiplier,
        }
    }
}
