use std::collections::VecDeque;

use rsnano_types::{Account, Block, BlockHash};
use rustc_hash::FxHashMap;

use rsnano_utils::container_info::{ContainerInfo, ContainerInfoProvider};

pub(super) struct ProcessingFinished {
    pub account: Account,
    /// `Some(hash)` means more blocks remain (now queued as ready to process).
    /// `None` means all blocks were processed; the account should be re-enqueued for download.
    pub next_block_hash: Option<BlockHash>,
}

/// Manages downloaded blocks from receipt through block-processor hand-off.
///
/// Blocks survive a block/unblock cycle: when an account is blocked its blocks
/// stay in the cache and are processed immediately once the account is unblocked,
/// without re-downloading.
#[derive(Default)]
pub(super) struct BlockHandoffQueue {
    ready_to_process: FxHashMap<BlockHash, Account>,
    processing: FxHashMap<BlockHash, Account>,
    block_cache: FxHashMap<Account, VecDeque<Block>>,
    cached_block_count: usize,
}

impl BlockHandoffQueue {
    pub fn contains(&self, account: &Account) -> bool {
        self.block_cache.contains_key(account)
    }

    /// Stores downloaded blocks and marks the account as ready to process.
    pub fn enqueue(&mut self, account: Account, blocks: VecDeque<Block>) {
        debug_assert!(!self.block_cache.contains_key(&account));
        let first_hash = blocks
            .front()
            .map(|b| b.hash())
            .expect("should at least have one block");
        self.cached_block_count += blocks.len();
        self.block_cache.insert(account, blocks);
        self.ready_to_process.insert(first_hash, account);
    }

    /// Removes the account from the active tracking sets (ready_to_process / processing)
    /// while keeping its blocks in the cache. Used when blocking an account that already
    /// has cached blocks, so those blocks survive until the account is unblocked.
    pub fn suspend(&mut self, hash: &BlockHash) -> Option<Account> {
        let mut account = self.ready_to_process.remove(&hash);
        if account.is_none() {
            account = self.processing.remove(&hash);
        }
        account
    }

    pub fn has_blocks_for(&self, account: &Account) -> bool {
        self.block_cache.contains_key(account)
    }

    pub fn processing(&self) -> Vec<BlockHash> {
        self.processing.keys().cloned().collect()
    }

    /// Re-inserts the account into ready_to_process using its cached blocks.
    /// Returns the first block hash, or `None` if the account has no cached blocks.
    pub fn resume(&mut self, account: Account) -> Option<BlockHash> {
        let hash = self.first_block_hash(&account)?;
        self.ready_to_process.insert(hash, account);
        Some(hash)
    }

    pub fn first_block_hash(&self, account: &Account) -> Option<BlockHash> {
        self.block_cache.get(account)?.front().map(|b| b.hash())
    }

    /// Atomically picks the next block ready for processing and moves it
    /// from `ready_to_process` into `processing`. The caller must either commit
    /// the hand-off (e.g. push to the block processor queue) or call
    /// `revert_processing_started` if it cannot proceed; otherwise the block
    /// will be stranded in `processing`.
    pub fn take_next_block_for_processing(&mut self) -> Option<Block> {
        let &account = self.ready_to_process.values().next()?;
        let block = self.block_cache.get(&account)?.front()?.clone();
        let hash = block.hash();
        self.ready_to_process.remove(&hash);
        self.processing.insert(hash, account);
        Some(block)
    }

    /// Reverts a previous `take_next_block_for_processing` by moving the block
    /// back from `processing` into `ready_to_process`.
    pub fn revert_processing_started(&mut self, block_hash: &BlockHash) -> bool {
        let Some(account) = self.processing.remove(block_hash) else {
            return false;
        };
        self.ready_to_process.insert(*block_hash, account);
        true
    }

    pub fn processing_finished(&mut self, block_hash: &BlockHash) -> Option<ProcessingFinished> {
        let account = self.processing.remove(block_hash)?;
        let next_block_hash = {
            let blocks = self.block_cache.get_mut(&account).unwrap();
            let first_block = blocks.pop_front().unwrap();
            debug_assert_eq!(first_block.hash(), *block_hash);
            self.cached_block_count -= 1;
            blocks.front().map(|b| b.hash())
        };
        if let Some(hash) = next_block_hash {
            self.ready_to_process.insert(hash, account);
        } else {
            self.block_cache.remove(&account);
        }
        Some(ProcessingFinished {
            account,
            next_block_hash,
        })
    }

    pub fn processing_failed(&mut self, block_hash: &BlockHash) -> Option<Account> {
        if let Some(account) = self.processing.remove(block_hash) {
            self.block_cache.remove(&account);
            Some(account)
        } else {
            None
        }
    }

    pub fn reprocess(&mut self, block_hash: &BlockHash) -> bool {
        let Some(account) = self.processing.remove(block_hash) else {
            return false;
        };
        self.ready_to_process.insert(*block_hash, account);
        true
    }

    /// Removes all blocks for the account from all internal structures.
    /// Returns the number of removed (discarded) blocks.
    pub fn remove(&mut self, account: &Account) -> usize {
        let Some(blocks) = self.block_cache.remove(account) else {
            return 0;
        };
        self.suspend(&blocks.front().unwrap().hash());
        let count = blocks.len();
        self.cached_block_count -= count;
        count
    }

    pub fn ready_to_process_len(&self) -> usize {
        self.ready_to_process.len()
    }

    pub fn processing_len(&self) -> usize {
        self.processing.len()
    }

    pub fn len(&self) -> usize {
        self.ready_to_process.len() + self.processing.len()
    }

    pub fn cached_block_count(&self) -> usize {
        self.cached_block_count
    }
}

impl ContainerInfoProvider for BlockHandoffQueue {
    fn container_info(&self) -> ContainerInfo {
        [
            ("ready_to_process", self.ready_to_process.len(), 0),
            ("processing", self.processing.len(), 0),
            ("cached_blocks", self.cached_block_count, 0),
        ]
        .into()
    }
}
