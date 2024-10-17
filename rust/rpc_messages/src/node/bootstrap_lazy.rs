use crate::RpcCommand;
use rsnano_core::BlockHash;
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn bootstrap_lazy(args: BootstrapLazyArgs) -> Self {
        Self::BoostrapLazy(args)
    }
}

impl From<BlockHash> for BootstrapLazyArgs {
    fn from(value: BlockHash) -> Self {
        Self::builder(value).build()
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BootstrapLazyArgs {
    pub hash: BlockHash,
    pub force: Option<bool>,
    pub id: Option<String>,
}

impl BootstrapLazyArgs {
    pub fn builder(hash: BlockHash) -> BootsrapLazyArgsBuilder {
        BootsrapLazyArgsBuilder {
            args: BootstrapLazyArgs {
                hash,
                force: None,
                id: None,
            },
        }
    }
}

pub struct BootsrapLazyArgsBuilder {
    args: BootstrapLazyArgs,
}

impl BootsrapLazyArgsBuilder {
    pub fn force(mut self) -> Self {
        self.args.force = Some(true);
        self
    }

    pub fn id(mut self, id: String) -> Self {
        self.args.id = Some(id);
        self
    }

    pub fn build(self) -> BootstrapLazyArgs {
        self.args
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BootstrapLazyDto {
    pub started: bool,
    pub key_inserted: bool,
}

impl BootstrapLazyDto {
    pub fn new(started: bool, key_inserted: bool) -> Self {
        Self {
            started,
            key_inserted,
        }
    }
}
