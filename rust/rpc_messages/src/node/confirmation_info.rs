use crate::RpcCommand;
use rsnano_core::QualifiedRoot;
use rsnano_core::{Account, Amount, BlockHash, JsonBlock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

impl RpcCommand {
    pub fn confirmation_info(args: ConfirmationInfoArgs) -> Self {
        Self::ConfirmationInfo(args)
    }
}

impl From<QualifiedRoot> for ConfirmationInfoArgs {
    fn from(value: QualifiedRoot) -> Self {
        Self::builder(value).build()
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConfirmationInfoArgs {
    pub root: QualifiedRoot,
    pub contents: Option<bool>,
    pub representatives: Option<bool>,
}

impl ConfirmationInfoArgs {
    pub fn builder(root: QualifiedRoot) -> ConfirmationInfoArgsBuilder {
        ConfirmationInfoArgsBuilder {
            args: ConfirmationInfoArgs {
                root,
                contents: None,
                representatives: None,
            },
        }
    }
}

pub struct ConfirmationInfoArgsBuilder {
    args: ConfirmationInfoArgs,
}

impl ConfirmationInfoArgsBuilder {
    pub fn without_contents(mut self) -> Self {
        self.args.contents = Some(false);
        self
    }

    pub fn include_representatives(mut self) -> Self {
        self.args.representatives = Some(true);
        self
    }

    pub fn build(self) -> ConfirmationInfoArgs {
        self.args
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConfirmationInfoDto {
    pub announcements: u32,
    pub voters: usize,
    pub last_winner: BlockHash,
    pub total_tally: Amount,
    pub final_tally: Amount,
    pub blocks: HashMap<BlockHash, ConfirmationBlockInfoDto>,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConfirmationBlockInfoDto {
    pub tally: Amount,
    pub contents: Option<JsonBlock>,
    pub representatives: Option<HashMap<Account, Amount>>,
}

impl ConfirmationInfoDto {
    pub fn new(
        announcements: u32,
        voters: usize,
        last_winner: BlockHash,
        total_tally: Amount,
        final_tally: Amount,
        blocks: HashMap<BlockHash, ConfirmationBlockInfoDto>,
    ) -> Self {
        Self {
            announcements,
            voters,
            last_winner,
            total_tally,
            final_tally,
            blocks,
        }
    }
}
