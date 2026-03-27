mod active_elections_container;
mod aec_delivery;
mod aec_service;
mod apply_vote_helper;
mod cooldown_controller;
mod recently_confirmed_cache;
mod root_container;
mod stats;
mod vote_router;

use std::collections::HashMap;

use rsnano_types::{Amount, Block, BlockHash, BlockPriority, QualifiedRoot, SavedBlock, VoteError};

use super::{
    ReceivedVote,
    election::{ConfirmedElection, Election, ElectionBehavior},
};
pub use active_elections_container::*;
pub(crate) use aec_delivery::*;
pub use aec_service::*;
pub use cooldown_controller::AecCooldownReason;
use root_container::{Entry, RootContainer};

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveElectionsConfig {
    /// Maximum number of simultaneous active elections (AEC size)
    pub max_elections: usize,
    /// Maximum cache size for recently_confirmed
    pub confirmation_cache: usize,
}

impl Default for ActiveElectionsConfig {
    fn default() -> Self {
        Self {
            max_elections: 5000,
            confirmation_cache: 65536,
        }
    }
}

pub(crate) enum AecFact {
    ElectionStarted(BlockHash, QualifiedRoot),
    ElectionConfirmed(ConfirmedElection),
    /// Ended ether confirmed or unconfirmed
    ElectionEnded(Election),
    BlockAddedToElection(BlockHash),
    BlockDiscarded(Block),
    BlockConfirmed(SavedBlock, ConfirmedElection),
    /// old winner + new winner block
    WinnerChanged(BlockHash, Block),

    VoteProcessed(
        ReceivedVote,
        Amount,
        HashMap<BlockHash, Result<(), VoteError>>,
    ),
    Recovered,
}

#[derive(Default)]
pub(crate) struct AecFacts(Vec<AecFact>);

impl AecFacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, fact: AecFact) {
        self.0.push(fact);
    }

    pub fn extend(&mut self, facts: Self) {
        self.0.extend(facts.0);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg(test)]
    pub fn as_slice(&self) -> &[AecFact] {
        &self.0
    }
}

impl From<AecFact> for AecFacts {
    fn from(value: AecFact) -> Self {
        Self(vec![value])
    }
}

impl IntoIterator for AecFacts {
    type Item = AecFact;
    type IntoIter = std::vec::IntoIter<AecFact>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum AecInsertError {
    Stopped,
    Duplicate,

    /// This block or a fork got recently confirmed, so there is no need for a new election.
    RecentlyConfirmed,
}

#[derive(Default)]
pub struct ActiveElectionsInfo {
    pub max_elections: usize,
    pub total: usize,
    pub priority: usize,
    pub hinted: usize,
    pub optimistic: usize,
}

#[derive(Default)]
pub struct ConfirmationActiveInfo {
    pub unconfirmed_roots: Vec<QualifiedRoot>,
    pub confirmed: u64,
}

pub struct AecInsertRequest {
    pub block: SavedBlock,
    pub behavior: ElectionBehavior,
    pub priority: BlockPriority,
}

impl AecInsertRequest {
    pub fn new_hinted(block: SavedBlock, priority: BlockPriority) -> Self {
        Self {
            block,
            behavior: ElectionBehavior::Hinted,
            priority,
        }
    }

    pub fn new_optimistic(block: SavedBlock, priority: BlockPriority) -> Self {
        Self {
            block,
            behavior: ElectionBehavior::Optimistic,
            priority,
        }
    }

    pub fn new_manual(block: SavedBlock, priority: BlockPriority) -> Self {
        Self {
            block,
            behavior: ElectionBehavior::Manual,
            priority,
        }
    }

    pub fn new_priority(block: SavedBlock, priority: BlockPriority) -> Self {
        Self {
            block,
            behavior: ElectionBehavior::Priority,
            priority,
        }
    }
}

pub enum AecActivateRequest {
    Manual {
        block: SavedBlock,
        priority: BlockPriority,
    },
    Hinted {
        block: SavedBlock,
        priority: BlockPriority,
    },
    Optimistic {
        block: SavedBlock,
        priority: BlockPriority,
    },
    Priority {
        block: SavedBlock,
        priority: BlockPriority,
        bucket_index: usize,
        reserved_elections: usize,
    },
}

impl AecActivateRequest {
    pub fn manual(block: SavedBlock, priority: BlockPriority) -> Self {
        Self::Manual { block, priority }
    }

    pub fn hinted(block: SavedBlock, priority: BlockPriority) -> Self {
        Self::Hinted { block, priority }
    }

    pub fn optimistic(block: SavedBlock, priority: BlockPriority) -> Self {
        Self::Optimistic { block, priority }
    }

    pub fn priority(
        block: SavedBlock,
        priority: BlockPriority,
        bucket_index: usize,
        reserved_elections: usize,
    ) -> Self {
        Self::Priority {
            block,
            priority,
            bucket_index,
            reserved_elections,
        }
    }

    fn block_hash(&self) -> BlockHash {
        match self {
            Self::Manual { block, .. }
            | Self::Hinted { block, .. }
            | Self::Optimistic { block, .. }
            | Self::Priority { block, .. } => block.hash(),
        }
    }

    fn transitions_to_active(&self) -> bool {
        matches!(self, Self::Manual { .. })
    }

    fn into_insert_request(self) -> AecInsertRequest {
        match self {
            Self::Manual { block, priority } => AecInsertRequest::new_manual(block, priority),
            Self::Hinted { block, priority } => AecInsertRequest::new_hinted(block, priority),
            Self::Optimistic { block, priority } => {
                AecInsertRequest::new_optimistic(block, priority)
            }
            Self::Priority {
                block, priority, ..
            } => AecInsertRequest::new_priority(block, priority),
        }
    }
}

const AEC_STAT_KEY: &str = "active_elections";
