use rsnano_types::{BlockHash, BlockPriority, QualifiedRoot, SavedBlock, TimePriority};

use super::ordered_blocks::{BlockEntry, OrderedBlocks};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PriorityBucketState {
    pub active_len: usize,
    pub contains_candidate: bool,
    pub lowest: Option<(QualifiedRoot, TimePriority)>,
    pub vacancy: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PriorityBucketConfig {
    /// Maximum number of blocks to sort by priority per bucket.
    pub max_blocks: usize,

    /// Number of guaranteed slots per bucket available for election activation.
    pub reserved_elections: usize,

    // TODO remove
    /// Maximum number of slots per bucket available for election activation if the active election count is below the configured limit. (node.active_elections.size)
    pub max_elections: usize,
}

impl Default for PriorityBucketConfig {
    fn default() -> Self {
        Self {
            max_blocks: 1024 * 8,
            reserved_elections: 100,
            max_elections: 150,
        }
    }
}

/// A struct which holds an ordered set of blocks to be scheduled, ordered by their block arrival time
pub struct Bucket {
    config: PriorityBucketConfig,
    block_queue: OrderedBlocks,
}

impl Bucket {
    pub fn new(config: PriorityBucketConfig, _bucket_id: usize) -> Self {
        Self {
            config,
            block_queue: Default::default(),
        }
    }

    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.block_queue.contains(hash)
    }

    pub fn len(&self) -> usize {
        self.block_queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn reserved_elections(&self) -> usize {
        self.config.reserved_elections
    }

    pub fn blocks(&self) -> impl Iterator<Item = &SavedBlock> {
        self.block_queue.iter().map(|i| &i.block)
    }

    pub(crate) fn pop_highest_priority(&mut self) -> Option<(SavedBlock, BlockPriority)> {
        self.block_queue
            .pop_highest_prio()
            .map(|entry| (entry.block, entry.priority))
    }

    pub fn insert(
        &mut self,
        priority: BlockPriority,
        block: SavedBlock,
    ) -> Result<Eviction, BucketInsertError> {
        let hash = block.hash();
        let inserted = self.block_queue.insert(BlockEntry::new(block, priority));
        if !inserted {
            return Err(BucketInsertError::Duplicate);
        }

        if self.block_queue.len() > self.config.max_blocks {
            let removed = self.block_queue.pop_lowest_prio().unwrap();
            if removed.block.hash() == hash {
                return Err(BucketInsertError::PriorityTooLow);
            }
            Ok(Eviction::Evicted)
        } else {
            Ok(Eviction::None)
        }
    }

    pub(crate) fn available(&self, state: &PriorityBucketState) -> bool {
        let Some(highest_block) = self.block_queue.highest_prio() else {
            // No blocks enqueued
            return false;
        };

        let candidate_prio = highest_block.priority.time;
        let can_reprioritize = state
            .lowest
            .as_ref()
            .map(|(_, lowest)| candidate_prio > *lowest)
            .unwrap_or(false);

        if can_reprioritize {
            return true;
        }

        if state.active_len >= self.config.reserved_elections {
            return false;
        }

        state.vacancy > 0 // cooldown check. TODO: check for cooldown explicitly
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Eviction {
    /// Inserted WITHOUT removing a lower priority entry
    None,
    /// Inserted and a lower priority entry got removed
    Evicted,
}

#[derive(PartialEq, Eq, Debug)]
pub enum BucketInsertError {
    /// The block was already in the bucket
    Duplicate,
    /// The bucket was full and the blocks priority was too low to replace another block
    PriorityTooLow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_types::{Amount, TimePriority};

    #[test]
    fn construction() {
        let fixture = create_fixture();
        let bucket = &fixture.bucket;

        assert_eq!(bucket.len(), 0);
        assert_eq!(bucket.contains(&BlockHash::from(1)), false);
        assert_eq!(bucket.available(&PriorityBucketState::default()), false);
    }

    #[test]
    fn insert_one() {
        let mut fixture = create_fixture();
        let bucket = &mut fixture.bucket;
        let block = SavedBlock::new_test_instance();

        assert_eq!(
            bucket.insert(test_priority(1000), block.clone()),
            Ok(Eviction::None)
        );

        assert_eq!(bucket.len(), 1);
        assert_eq!(bucket.contains(&block.hash()), true);
        assert_eq!(
            bucket.available(&PriorityBucketState {
                vacancy: 1,
                ..Default::default()
            }),
            true
        );
    }

    #[test]
    fn insert_duplicate() {
        let mut fixture = create_fixture();
        let bucket = &mut fixture.bucket;
        let block = SavedBlock::new_test_instance();

        assert_eq!(
            bucket.insert(test_priority(1000), block.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(1000), block),
            Err(BucketInsertError::Duplicate)
        );
        assert_eq!(bucket.len(), 1);
    }

    #[test]
    fn insert_many() {
        let mut fixture = create_fixture();
        let bucket = &mut fixture.bucket;
        let block0 = SavedBlock::new_test_instance_with_key(1);
        let block1 = SavedBlock::new_test_instance_with_key(2);
        let block2 = SavedBlock::new_test_instance_with_key(3);
        let block3 = SavedBlock::new_test_instance_with_key(4);
        assert_eq!(
            bucket.insert(test_priority(2000), block0.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(1001), block1.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(1000), block2.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(900), block3.clone()),
            Ok(Eviction::None)
        );

        assert_eq!(bucket.len(), 4);
        let blocks: Vec<_> = bucket.blocks().cloned().collect();
        assert_eq!(blocks.len(), 4);
        // Ensure correct order
        assert_eq!(blocks[0], block3);
        assert_eq!(blocks[1], block2);
        assert_eq!(blocks[2], block1);
        assert_eq!(blocks[3], block0);
    }

    #[test]
    fn max_blocks() {
        let mut fixture = create_fixture_with(FixtureArgs {
            config: PriorityBucketConfig {
                max_blocks: 2,
                ..Default::default()
            },
        });
        let bucket = &mut fixture.bucket;

        let block0 = SavedBlock::new_test_instance_with_key(1);
        let block1 = SavedBlock::new_test_instance_with_key(2);
        let block2 = SavedBlock::new_test_instance_with_key(3);
        let block3 = SavedBlock::new_test_instance_with_key(4);

        assert_eq!(
            bucket.insert(test_priority(2000), block0.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(900), block1.clone()),
            Ok(Eviction::None)
        );
        assert_eq!(
            bucket.insert(test_priority(3000), block2.clone()),
            Err(BucketInsertError::PriorityTooLow)
        );
        assert_eq!(
            bucket.insert(test_priority(1001), block3.clone()),
            Ok(Eviction::Evicted)
        ); // Evicts 2000
        assert_eq!(bucket.contains(&block0.hash()), false);
        assert_eq!(
            bucket.insert(test_priority(1000), block0.clone()),
            Ok(Eviction::Evicted)
        ); // Evicts 1001
        assert_eq!(bucket.contains(&block3.hash()), false);

        assert_eq!(bucket.len(), 2);
        let blocks: Vec<_> = bucket.blocks().cloned().collect();
        // Ensure correct order
        assert_eq!(blocks[0], block1);
        assert_eq!(blocks[1], block0);
    }

    #[derive(Default)]
    struct FixtureArgs {
        config: PriorityBucketConfig,
    }

    struct Fixture {
        bucket: Bucket,
    }

    fn create_fixture() -> Fixture {
        create_fixture_with(FixtureArgs::default())
    }

    fn create_fixture_with(args: FixtureArgs) -> Fixture {
        let bucket = Bucket::new(args.config, 1);

        Fixture { bucket }
    }

    fn test_priority(time_prio: u64) -> BlockPriority {
        BlockPriority::new(Amount::nano(1), TimePriority::new(time_prio))
    }
}
