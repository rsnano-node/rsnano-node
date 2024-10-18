mod account;
mod account_balance;
mod accounts;
mod accounts_balances;
mod accounts_with_amounts;
mod address_with_port;
mod amount;
mod block;
mod blocks;
mod count;
mod destroyed;
mod error;
mod exists;
mod frontiers;
mod hash;
mod hashes;
mod key_pair;
mod locked;
mod moved;
mod public_key;
mod receivable;
mod removed;
mod started;
mod success;
mod valid;
mod wallet;
mod wallet_with_account;
mod wallet_with_count;
mod wallet_with_password;
mod weight;

pub use account::*;
pub use account_balance::*;
pub use accounts::*;
pub use accounts_balances::*;
pub use accounts_with_amounts::*;
pub use address_with_port::*;
pub use amount::*;
pub use block::*;
pub use blocks::*;
pub use count::*;
pub use destroyed::*;
pub use error::*;
pub use exists::*;
pub use frontiers::*;
pub use hash::*;
pub use hashes::*;
pub use key_pair::*;
pub use locked::*;
pub use moved::*;
pub use public_key::*;
pub use receivable::*;
pub use removed::*;
pub use started::*;
pub use success::*;
pub use valid::*;
pub use wallet::*;
pub use wallet_with_account::*;
pub use wallet_with_count::*;
pub use wallet_with_password::*;
pub use weight::*;

use rsnano_core::{BlockType, WorkVersion};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkVersionDto {
    Work1,
}

impl From<WorkVersion> for WorkVersionDto {
    fn from(value: WorkVersion) -> Self {
        match value {
            WorkVersion::Unspecified => unimplemented!(),
            WorkVersion::Work1 => WorkVersionDto::Work1,
        }
    }
}

impl From<WorkVersionDto> for WorkVersion {
    fn from(value: WorkVersionDto) -> Self {
        match value {
            WorkVersionDto::Work1 => WorkVersion::Work1,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockTypeDto {
    Send,
    Receive,
    Open,
    Change,
    State,
}

impl From<BlockType> for BlockTypeDto {
    fn from(value: BlockType) -> Self {
        match value {
            BlockType::LegacySend => BlockTypeDto::Send,
            BlockType::LegacyReceive => BlockTypeDto::Receive,
            BlockType::LegacyOpen => BlockTypeDto::Open,
            BlockType::LegacyChange => BlockTypeDto::Change,
            BlockType::State => BlockTypeDto::State,
            BlockType::Invalid | BlockType::NotABlock => unimplemented!(),
        }
    }
}

impl From<BlockTypeDto> for BlockType {
    fn from(value: BlockTypeDto) -> Self {
        match value {
            BlockTypeDto::Send => BlockType::LegacySend,
            BlockTypeDto::Receive => BlockType::LegacyReceive,
            BlockTypeDto::Open => BlockType::LegacyOpen,
            BlockTypeDto::Change => BlockType::LegacyChange,
            BlockTypeDto::State => BlockType::State,
        }
    }
}
