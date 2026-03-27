use std::{cmp::Ordering, collections::BTreeSet};

use rsnano_types::{BlockHash, BlockPriority, QualifiedRoot, TimePriority};
use rustc_hash::FxHashMap;

use super::{AecInsertRequest, vote_router::VoteRouter};
use crate::consensus::{
    election::{Election, ElectionBehavior},
    election_schedulers::priority::{bucket_count, bucket_index},
};

pub(crate) struct Entry {
    pub root: QualifiedRoot,
    pub election: Election,
    pub priority: BlockPriority,
}

impl Entry {
    pub fn bucket(&self) -> usize {
        bucket_index(self.election.behavior(), self.priority.balance)
    }
}

/// Ordered by descending time priority
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct BucketEntry {
    root: QualifiedRoot,
    priority: BlockPriority,
}

impl Ord for BucketEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.priority.time.cmp(&self.priority.time) {
            Ordering::Equal => match other.priority.balance.cmp(&self.priority.balance) {
                Ordering::Equal => other.root.cmp(&self.root),
                result => result,
            },
            result => result,
        }
    }
}

impl PartialOrd for BucketEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Contains elections and their qualified roots
pub(crate) struct RootContainer {
    by_root: FxHashMap<QualifiedRoot, Entry>,
    buckets: Vec<BTreeSet<BucketEntry>>,
    pub vote_router: VoteRouter,
}

impl Default for RootContainer {
    fn default() -> Self {
        Self {
            by_root: Default::default(),
            vote_router: Default::default(),
            buckets: vec![BTreeSet::new(); bucket_count()],
        }
    }
}

impl RootContainer {
    pub const ELEMENT_SIZE: usize = size_of::<QualifiedRoot>() * 2 + size_of::<Election>();

    pub fn insert(&mut self, entry: Entry) {
        let root = entry.root.clone();
        let hash = entry.election.winner().hash();
        let bucket_entry = BucketEntry {
            root: entry.root.clone(),
            priority: entry.priority,
        };
        self.buckets[entry.bucket()].insert(bucket_entry);
        self.by_root.insert(root.clone(), entry);
        self.vote_router.connect(hash, root.clone());
    }

    pub fn get(&self, root: &QualifiedRoot) -> Option<&Entry> {
        self.by_root.get(root)
    }

    pub fn get_mut(&mut self, root: &QualifiedRoot) -> Option<&mut Entry> {
        self.by_root.get_mut(root)
    }

    pub fn election_for_root(&self, root: &QualifiedRoot) -> Option<&Election> {
        self.get(root).map(|i| &i.election)
    }

    pub fn election_for_root_mut(&mut self, root: &QualifiedRoot) -> Option<&mut Election> {
        self.get_mut(root).map(|i| &mut i.election)
    }

    pub fn election_for_block(&self, block_hash: &BlockHash) -> Option<&Election> {
        let root = self.vote_router.qualified_root(block_hash)?;
        self.election_for_root(root)
    }

    pub fn election_for_block_mut(&mut self, block_hash: &BlockHash) -> Option<&mut Election> {
        let root = self.vote_router.qualified_root(block_hash)?.clone();
        self.get_mut(&root).map(|i| &mut i.election)
    }

    pub fn try_upgrade_to_priority_election(
        &mut self,
        request: &AecInsertRequest,
    ) -> (bool, Option<ElectionBehavior>) {
        let root = request.block.qualified_root();

        let Some(entry) = self.get_mut(&root) else {
            return (false, None);
        };

        let previous_behavior = entry.election.behavior();
        if request.behavior != ElectionBehavior::Priority {
            return (false, Some(previous_behavior));
        }

        let priority = entry.priority;
        let upgraded = entry.election.maybe_upgrade_to(ElectionBehavior::Priority);
        if !upgraded {
            return (false, Some(previous_behavior));
        }

        self.buckets[bucket_index(previous_behavior, priority.balance)].remove(&BucketEntry {
            root: root.clone(),
            priority,
        });

        self.buckets[bucket_index(ElectionBehavior::Priority, priority.balance)].insert(
            BucketEntry {
                root: root.clone(),
                priority,
            },
        );
        (true, Some(previous_behavior))
    }

    pub fn drain_filter(&mut self, mut predicate: impl FnMut(&Entry) -> bool) -> Vec<Entry> {
        let to_remove: Vec<_> = self
            .by_root
            .values()
            .filter_map(|i| {
                if predicate(i) {
                    Some(i.root.clone())
                } else {
                    None
                }
            })
            .collect();

        let mut removed = Vec::new();
        for root in to_remove {
            if let Some(entry) = self.erase(&root) {
                removed.push(entry);
            }
        }

        removed
    }

    pub fn erase(&mut self, root: &QualifiedRoot) -> Option<Entry> {
        let erased = self.by_root.remove(root);
        if let Some(entry) = &erased {
            self.vote_router.disconnect_election(&entry.election);
            self.buckets[entry.bucket()].remove(&BucketEntry {
                root: entry.root.clone(),
                priority: entry.priority,
            });
        }
        erased
    }

    pub fn clear(&mut self) {
        self.by_root.clear();
        for bucket in self.buckets.iter_mut() {
            bucket.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.by_root.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        RoundRobinIterator::new(self)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.by_root.values_mut()
    }

    pub fn iter_bucket(&self, bucket_id: usize) -> impl Iterator<Item = &Entry> {
        self.buckets[bucket_id]
            .iter()
            .map(|i| self.by_root.get(&i.root).unwrap())
    }

    pub fn bucket_len(&self, bucket_id: usize) -> usize {
        self.buckets[bucket_id].len()
    }

    pub fn lowest_priority(&self, bucket_id: usize) -> Option<(QualifiedRoot, TimePriority)> {
        self.buckets[bucket_id]
            .last()
            .map(|i| (i.root.clone(), i.priority.time))
    }
}

struct RoundRobinIterator<'a> {
    roots: &'a RootContainer,
    bucket_iters: Vec<std::collections::btree_set::Iter<'a, BucketEntry>>,
    current: usize,
    yielded: bool,
}

impl<'a> RoundRobinIterator<'a> {
    fn new(aec: &'a RootContainer) -> Self {
        let mut bucket_iters = Vec::with_capacity(bucket_count());
        for bucket in aec.buckets.iter().rev() {
            if !bucket.is_empty() {
                bucket_iters.push(bucket.iter())
            }
        }
        Self {
            roots: aec,
            bucket_iters,
            current: 0,
            yielded: false,
        }
    }
}

impl<'a> Iterator for RoundRobinIterator<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.bucket_iters.len() {
            let item = self.bucket_iters[self.current].next();
            if item.is_some() {
                self.yielded = true;
            }

            self.current += 1;
            if self.current >= self.bucket_iters.len() && self.yielded {
                self.current = 0;
                self.yielded = false;
            }

            if let Some(item) = item {
                return self.roots.by_root.get(&item.root);
            }
        }

        None
    }
}
