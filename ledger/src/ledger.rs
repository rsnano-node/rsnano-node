use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddrV6,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use tracing::debug;

use rsnano_nullable_lmdb::{LmdbEnvironment, Transaction, WriteTransaction};
#[cfg(feature = "ledger_snapshots")]
use rsnano_store_lmdb::forks_store::ConfiguredForksDatabaseBuilder;
use rsnano_store_lmdb::{
    ConfiguredAccountDatabaseBuilder, ConfiguredBlockDatabaseBuilder,
    ConfiguredConfirmationHeightDatabaseBuilder, ConfiguredPeersDatabaseBuilder,
    ConfiguredPendingDatabaseBuilder, LmdbStore, MemoryStats,
};
#[cfg(feature = "ledger_snapshots")]
use rsnano_types::SnapshotNumber;
use rsnano_types::{
    Account, AccountInfo, Amount, Block, BlockHash, ConfirmationHeightInfo, Epoch, Link,
    PendingInfo, PendingKey, PublicKey, QualifiedRoot, Root, SavedBlock, UnixTimestamp,
};
use rsnano_utils::{
    container_info::{ContainerInfo, ContainerInfoProvider},
    stats::{DetailType, StatType, Stats},
};
use rsnano_work::WorkThresholds;

use crate::{
    BlockRollbackPerformer, BorrowingAnySet, BorrowingConfirmedSet, GenerateCacheFlags,
    LedgerConstants, LedgerSet, OwningAnySet, OwningConfirmedSet, OwningUnconfirmedSet,
    RepWeightCache, RepWeightsUpdater, RollbackError,
    block_cementer::BlockCementer,
    block_insertion::{BlockInserter, BlockValidatorFactory},
    vote_verifier::VoteVerifier,
};
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};

#[derive(PartialEq, Eq, Debug, Clone, Copy, EnumCount, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum BlockError {
    /// Signature was bad, forged or transmission error
    BadSignature,
    /// Already seen and was valid
    Old,
    /// Malicious attempt to spend a negative amount
    NegativeSpend,
    /// Malicious fork based on previous
    Fork,
    /// Source block doesn't exist, has already been received, or requires an account upgrade (epoch blocks)
    Unreceivable,
    /// Block marked as previous is unknown
    GapPrevious,
    /// Block marked as source is unknown
    GapSource,
    /// Block marked as pending blocks required for epoch open block are unknown
    GapEpochOpenPending,
    /// Block attempts to open the burn account
    OpenedBurnAccount,
    /// Balance and amount delta don't match
    BalanceMismatch,
    /// Representative is changed when it is not allowed
    RepresentativeMismatch,
    /// This block cannot follow the previous block
    BlockPosition,
    /// Insufficient work for this block, even though it passed the minimal validation
    InsufficientWork,
    /// The account got updated while this block was processed. This block is ether old or a fork.
    Conflict,
}

impl BlockError {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockError::BadSignature => "Bad signature",
            BlockError::Old => "Old",
            BlockError::NegativeSpend => "Negative spend",
            BlockError::Fork => "Fork",
            BlockError::Unreceivable => "Unreceivable",
            BlockError::GapPrevious => "Gap previous",
            BlockError::GapSource => "Gap source",
            BlockError::GapEpochOpenPending => "Gap epoch open pendign",
            BlockError::OpenedBurnAccount => "Opened burn account",
            BlockError::BalanceMismatch => "Balance mismatch",
            BlockError::RepresentativeMismatch => "Representative mismatch",
            BlockError::BlockPosition => "Block position",
            BlockError::InsufficientWork => "Insufficient work",
            BlockError::Conflict => "Conflict",
        }
    }
}

impl From<BlockError> for DetailType {
    fn from(value: BlockError) -> Self {
        match value {
            BlockError::BadSignature => Self::BadSignature,
            BlockError::Old => Self::Old,
            BlockError::NegativeSpend => Self::NegativeSpend,
            BlockError::Fork => Self::Fork,
            BlockError::Unreceivable => Self::Unreceivable,
            BlockError::GapPrevious => Self::GapPrevious,
            BlockError::GapSource => Self::GapSource,
            BlockError::GapEpochOpenPending => Self::GapEpochOpenPending,
            BlockError::OpenedBurnAccount => Self::OpenedBurnAccount,
            BlockError::BalanceMismatch => Self::BalanceMismatch,
            BlockError::RepresentativeMismatch => Self::RepresentativeMismatch,
            BlockError::BlockPosition => Self::BlockPosition,
            BlockError::InsufficientWork => Self::InsufficientWork,
            BlockError::Conflict => Self::Conflict,
        }
    }
}

pub struct Ledger {
    pub store: LmdbStore,
    pub rep_weights_updater: RepWeightsUpdater,
    pub rep_weights: Arc<RepWeightCache>,
    pub constants: LedgerConstants,
    pub(crate) stats: Arc<Stats>,
    rollback_listener: OutputListenerMt<BlockHash>,
}

pub struct NullLedgerBuilder {
    blocks: ConfiguredBlockDatabaseBuilder,
    accounts: ConfiguredAccountDatabaseBuilder,
    pending: ConfiguredPendingDatabaseBuilder,
    peers: ConfiguredPeersDatabaseBuilder,
    #[cfg(feature = "ledger_snapshots")]
    forks: ConfiguredForksDatabaseBuilder,
    confirmation_height: ConfiguredConfirmationHeightDatabaseBuilder,
    min_rep_weight: Amount,
}

impl NullLedgerBuilder {
    fn new() -> Self {
        Self {
            blocks: ConfiguredBlockDatabaseBuilder::new(),
            accounts: ConfiguredAccountDatabaseBuilder::new(),
            pending: ConfiguredPendingDatabaseBuilder::new(),
            peers: ConfiguredPeersDatabaseBuilder::new(),
            #[cfg(feature = "ledger_snapshots")]
            forks: ConfiguredForksDatabaseBuilder::new(),
            confirmation_height: ConfiguredConfirmationHeightDatabaseBuilder::new(),
            min_rep_weight: Amount::ZERO,
        }
    }

    pub fn block(mut self, block: &SavedBlock) -> Self {
        self.blocks = self.blocks.block(block);
        self
    }

    pub fn blocks<'a>(mut self, blocks: impl IntoIterator<Item = &'a SavedBlock>) -> Self {
        for b in blocks.into_iter() {
            self.blocks = self.blocks.block(b);
        }
        self
    }

    pub fn peers(mut self, peers: impl IntoIterator<Item = (SocketAddrV6, SystemTime)>) -> Self {
        for (peer, time) in peers.into_iter() {
            self.peers = self.peers.peer(peer, time)
        }
        self
    }

    pub fn confirmation_height(mut self, account: &Account, info: &ConfirmationHeightInfo) -> Self {
        self.confirmation_height = self.confirmation_height.height(account, info);
        self
    }

    pub fn account_info(mut self, account: &Account, info: &AccountInfo) -> Self {
        self.accounts = self.accounts.account(account, info);
        self
    }

    pub fn pending(mut self, key: &PendingKey, info: &PendingInfo) -> Self {
        self.pending = self.pending.pending(key, info);
        self
    }

    #[cfg(feature = "ledger_snapshots")]
    pub fn fork(mut self, root: &QualifiedRoot, snapshot_number: SnapshotNumber) -> Self {
        self.forks = self.forks.fork(root, snapshot_number);
        self
    }

    pub fn frontiers(self, frontiers: impl IntoIterator<Item = (Account, BlockHash)>) -> Self {
        let mut builder = self;

        for (account, frontier) in frontiers {
            builder = builder
                .account_info(&account, &AccountInfo::new_test_instance())
                .confirmation_height(
                    &account,
                    &ConfirmationHeightInfo {
                        height: 0,
                        frontier,
                    },
                );
        }

        builder
    }

    #[cfg(feature = "ledger_snapshots")]
    pub fn forks(self, forks: impl IntoIterator<Item = (QualifiedRoot, SnapshotNumber)>) -> Self {
        let mut builder = self;

        for (root, snapshot_number) in forks {
            builder = builder.fork(&root, snapshot_number);
        }

        builder
    }

    pub fn finish(self) -> Ledger {
        let (block_index, block_data) = self.blocks.build();
        let mut env_builder = LmdbEnvironment::null_builder()
            .configured_database(block_index)
            .configured_database(block_data)
            .configured_database(self.accounts.build())
            .configured_database(self.pending.build())
            .configured_database(self.confirmation_height.build())
            .configured_database(self.peers.build());

        #[cfg(feature = "ledger_snapshots")]
        {
            env_builder = env_builder.configured_database(self.forks.build())
        }
        let env = env_builder.build();

        Ledger::new(
            env,
            LedgerConstants::unit_test(),
            self.min_rep_weight,
            Arc::new(RepWeightCache::new()),
            Arc::new(Stats::default()),
            1,
        )
        .unwrap()
    }
}

impl Ledger {
    pub fn new_null() -> Self {
        Self::new(
            LmdbEnvironment::new_null(),
            LedgerConstants::unit_test(),
            Amount::ZERO,
            Arc::new(RepWeightCache::new()),
            Arc::new(Stats::default()),
            1,
        )
        .unwrap()
    }

    pub fn new_null_builder() -> NullLedgerBuilder {
        NullLedgerBuilder::new()
    }

    pub(crate) fn new(
        env: LmdbEnvironment,
        constants: LedgerConstants,
        min_rep_weight: Amount,
        rep_weights: Arc<RepWeightCache>,
        stats: Arc<Stats>,
        thread_count: usize,
    ) -> anyhow::Result<Self> {
        let mut store = LmdbStore::new(env)?;
        store.cache = rep_weights.ledger_cache.clone();

        let rep_weights_updater =
            RepWeightsUpdater::new(store.rep_weight.clone(), min_rep_weight, &rep_weights);

        let mut ledger = Self {
            rep_weights,
            rep_weights_updater,
            store,
            constants,
            stats,
            rollback_listener: Default::default(),
        };

        ledger.initialize(thread_count, &GenerateCacheFlags::new())?;

        Ok(ledger)
    }

    fn initialize(
        &mut self,
        thread_count: usize,
        generate_cache: &GenerateCacheFlags,
    ) -> anyhow::Result<()> {
        if self
            .store
            .account
            .iter(&self.store.begin_read())
            .next()
            .is_none()
        {
            let mut txn = self.store.begin_write();
            self.add_genesis_block(&mut txn);
            txn.commit();
        }

        if generate_cache.reps || generate_cache.account_count || generate_cache.block_count {
            self.store
                .account
                .for_each_par(&self.store.env, thread_count, |iter| {
                    let mut block_count = 0;
                    let mut account_count = 0;
                    let mut rep_weights: HashMap<PublicKey, Amount> = HashMap::new();

                    for (_, info) in iter {
                        block_count += info.block_count;
                        account_count += 1;
                        if !info.balance.is_zero() {
                            let total = rep_weights.entry(info.representative).or_default();
                            *total += info.balance;
                        }
                    }
                    self.store
                        .cache
                        .block_count
                        .fetch_add(block_count, Ordering::SeqCst);

                    self.store
                        .cache
                        .account_count
                        .fetch_add(account_count, Ordering::SeqCst);

                    self.rep_weights_updater.copy_from(&rep_weights);
                });
        }

        if generate_cache.confirmed_count {
            self.store
                .confirmation_height
                .for_each_par(&self.store.env, thread_count, |iter| {
                    let mut confirmed_count = 0;
                    for (_, info) in iter {
                        confirmed_count += info.height;
                    }
                    self.store
                        .cache
                        .confirmed_count
                        .fetch_add(confirmed_count, Ordering::SeqCst);
                });
        }

        Ok(())
    }

    fn add_genesis_block(&self, txn: &mut WriteTransaction) {
        let genesis_hash = self.constants.genesis_block.hash();
        let genesis_account = self.constants.genesis_account;
        self.store.block.put(txn, &self.constants.genesis_block);

        self.store.confirmation_height.put(
            txn,
            &genesis_account,
            &ConfirmationHeightInfo::new(1, genesis_hash),
        );

        self.store.account.put(
            txn,
            &genesis_account,
            &AccountInfo {
                head: genesis_hash,
                representative: genesis_account.into(),
                open_block: genesis_hash,
                balance: u128::MAX.into(),
                modified: UnixTimestamp::ZERO,
                block_count: 1,
                epoch: Epoch::Epoch0,
            },
        );
        self.store
            .rep_weight
            .put(txn, genesis_account.into(), Amount::MAX);
    }

    pub fn any(&self) -> OwningAnySet<'_> {
        OwningAnySet::new(&self.store, &self.constants)
    }

    pub fn confirmed(&self) -> OwningConfirmedSet<'_> {
        let tx = self.store.begin_read();
        OwningConfirmedSet::new(&self.store, tx)
    }

    pub fn unconfirmed(&self) -> impl LedgerSet + use<'_> {
        let tx = self.store.begin_read();
        OwningUnconfirmedSet::new(&self.store, tx)
    }

    pub fn bootstrap_weight_max_blocks(&self) -> u64 {
        self.rep_weights.bootstrap_weight_max_blocks()
    }

    /// Returns the cached vote weight for the given representative.
    /// If the weight is below the cache limit it returns 0.
    /// During bootstrap it returns the preconfigured bootstrap weights.
    pub fn weight(&self, rep: &PublicKey) -> Amount {
        self.rep_weights.weight(rep)
    }

    pub fn is_epoch_link(&self, link: &Link) -> bool {
        self.constants.epochs.is_epoch_link(link)
    }

    pub fn epoch_signer(&self, link: &Link) -> Option<Account> {
        self.constants.epochs.epoch_signer(link)
    }

    pub fn epoch_link(&self, epoch: Epoch) -> Option<Link> {
        self.constants.epochs.link(epoch).cloned()
    }

    pub(crate) fn update_account(
        &self,
        txn: &mut WriteTransaction,
        account: &Account,
        old_info: &AccountInfo,
        new_info: &AccountInfo,
    ) {
        if !new_info.head.is_zero() {
            if old_info.head.is_zero() && new_info.open_block == new_info.head {
                self.store
                    .cache
                    .account_count
                    .fetch_add(1, Ordering::SeqCst);
            }
            if !old_info.head.is_zero() && old_info.epoch != new_info.epoch {
                // store.account ().put won't erase existing entries if they're in different tables
                self.store.account.del(txn, account);
            }
            self.store.account.put(txn, account, new_info);
        } else {
            debug_assert!(!self.store.confirmation_height.exists(txn, account));
            self.store.account.del(txn, account);
            debug_assert!(self.store.cache.account_count.load(Ordering::SeqCst) > 0);
            self.store
                .cache
                .account_count
                .fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn track_rollbacks(&self) -> Arc<OutputTrackerMt<BlockHash>> {
        self.rollback_listener.track()
    }

    /// Rollback blocks until `block' doesn't exist or it tries to penetrate the confirmation height
    pub fn roll_back(&self, block: &BlockHash) -> Result<usize, RollbackError> {
        self.rollback_listener.emit(*block);
        let result = self.roll_back_batch(&[*block], usize::MAX, |_| true);
        let rolled_back = result[0].rolled_back.len();
        result[0].error.map_or(Ok(rolled_back), |e| Err(e))
    }

    pub fn roll_back_batch<'a, T, F>(
        &self,
        targets: T,
        max_rollbacks: usize,
        mut can_roll_back: F,
    ) -> RollbackResults
    where
        T: IntoIterator<Item = &'a BlockHash>,
        F: FnMut(&BlockHash) -> bool,
    {
        self.stats
            .inc(StatType::BoundedBacklog, DetailType::PerformingRollbacks);

        let mut rolled_back_count = 0;
        let mut results = RollbackResults::new();
        {
            let mut txn = self.store.begin_write();

            for hash in targets {
                // Skip the rollback if the block is being used by the node, this should be race free as it's checked while holding the ledger write lock
                if !can_roll_back(hash) {
                    self.stats
                        .inc(StatType::BoundedBacklog, DetailType::RollbackSkipped);
                    results.push(RollbackResult {
                        target_hash: *hash,
                        target_root: QualifiedRoot::ZERO,
                        rolled_back: Vec::new(),
                        error: Some(RollbackError::Rejected),
                    });
                    continue;
                }

                // Here we check that the block is still OK to rollback, there could be a delay between gathering the targets and performing the rollbacks
                if let Some(block) = self.store.block.get(&txn, hash) {
                    debug!(
                        "Rolling back: {}, account: {}",
                        hash,
                        block.account().encode_account()
                    );

                    let (rollback_list, error) = self.roll_back_with_tx(&mut txn, &block.hash());
                    if error.is_none() {
                        self.stats
                            .inc(StatType::BoundedBacklog, DetailType::Rollback);
                    } else {
                        self.stats
                            .inc(StatType::BoundedBacklog, DetailType::RollbackFailed);
                    }

                    rolled_back_count += rollback_list.len();
                    results.push(RollbackResult {
                        target_hash: *hash,
                        target_root: block.qualified_root(),
                        rolled_back: rollback_list,
                        error,
                    });

                    // Return early if we reached the maximum number of rollbacks
                    if rolled_back_count >= max_rollbacks {
                        break;
                    }
                } else {
                    self.stats
                        .inc(StatType::BoundedBacklog, DetailType::RollbackMissingBlock);
                    rolled_back_count += 1;
                    results.push(RollbackResult {
                        target_hash: *hash,
                        target_root: QualifiedRoot::ZERO,
                        rolled_back: Vec::new(),
                        error: Some(RollbackError::BlockNotFound),
                    });
                }
            }
            txn.commit();
        }

        results
    }

    fn roll_back_with_tx(
        &self,
        tx: &mut WriteTransaction,
        block: &BlockHash,
    ) -> (Vec<SavedBlock>, Option<RollbackError>) {
        let mut performer = BlockRollbackPerformer::new(self, tx);
        match performer.roll_back(block) {
            Ok(()) => (performer.rolled_back, None),
            Err(e) => (performer.rolled_back, Some(e)),
        }
    }

    pub fn process_one(&self, block: &Block) -> Result<SavedBlock, BlockError> {
        let mut result = self.process_batch(std::iter::once(block));
        let mut drain = result.processed.drain(..);
        match drain.next().unwrap() {
            (Ok(_), Some(block)) => Ok(block),
            (Ok(_), None) => unreachable!(),
            (Err(e), _) => Err(e),
        }
    }

    pub fn process_batch<'a>(
        &self,
        batch: impl IntoIterator<Item = &'a Block>,
    ) -> BatchProcessResult {
        let mut validation_results = Vec::new();

        // Validate blocks
        {
            let tx = self.store.begin_read();
            for block in batch.into_iter() {
                let any = BorrowingAnySet {
                    constants: &self.constants,
                    store: &self.store,
                    tx: &tx,
                };
                let validator =
                    BlockValidatorFactory::new(&any, &self.constants, block).create_validator();
                let result = validator.validate();
                validation_results.push((result, block));
            }
        }

        // Insert blocks
        let mut processed = Vec::with_capacity(validation_results.len());
        {
            let mut txn = self.store.begin_write();
            for (result, block) in validation_results {
                match result {
                    Ok(instructions) => {
                        if let Some(saved_block) =
                            BlockInserter::new(self, &mut txn, block, &instructions).insert()
                        {
                            processed.push((Ok(()), Some(saved_block.clone())));
                        } else {
                            let err = BlockError::Conflict;
                            processed.push((Err(err), None));
                        }
                    }
                    Err(err) => {
                        processed.push((Err(err), None));
                    }
                }
            }
            txn.commit();
        }

        BatchProcessResult { processed }
    }

    pub fn roll_back_competitors<'a, T, F>(&self, blocks: T, mut rolled_back_callback: F)
    where
        T: IntoIterator<Item = &'a Block>,
        F: FnMut(RollbackResults),
    {
        let mut rolled_back = RollbackResults::new();
        {
            let mut txn = self.store.begin_write();
            for block in blocks {
                if txn.is_refresh_needed() {
                    txn.commit();
                    if !rolled_back.is_empty() {
                        rolled_back_callback(rolled_back);
                        rolled_back = RollbackResults::new();
                    }
                    txn = self.store.begin_write();
                }
                let rolled_back_blocks = self.rollback_competitor(&mut txn, block);
                if !rolled_back_blocks.is_empty() {
                    rolled_back.push(RollbackResult {
                        target_hash: block.hash(),
                        target_root: block.qualified_root(),
                        rolled_back: rolled_back_blocks,
                        error: None,
                    });
                }
            }
            txn.commit();
        }
        if !rolled_back.is_empty() {
            rolled_back_callback(rolled_back);
        }
    }

    fn rollback_competitor(
        &self,
        tx: &mut WriteTransaction,
        fork_block: &Block,
    ) -> Vec<SavedBlock> {
        let mut rollback_list = Vec::new();
        let hash = fork_block.hash();
        if let Some(successor) =
            self.block_successor_by_qualified_root(tx, &fork_block.qualified_root())
        {
            if successor != hash {
                // Replace our block with the winner and roll back any dependent blocks
                debug!("Rolling back: {} and replacing with: {}", successor, hash);
                let (list, error) = self.roll_back_with_tx(tx, &successor);
                rollback_list = list;
                match error {
                    None => {
                        self.stats.inc(StatType::Ledger, DetailType::Rollback);
                        debug!("Blocks rolled back: {}", rollback_list.len());
                    }
                    Some(e) => {
                        self.stats.inc(StatType::Ledger, DetailType::RollbackFailed);
                        debug!(error = ?e, "Failed to roll back");
                    }
                };
            }
        }
        rollback_list
    }

    fn block_successor_by_qualified_root(
        &self,
        tx: &dyn Transaction,
        root: &QualifiedRoot,
    ) -> Option<BlockHash> {
        if !root.previous.is_zero() {
            self.store.successors.get(tx, &root.previous)
        } else {
            self.store
                .account
                .get(tx, &root.root.into())
                .map(|i| i.open_block)
        }
    }

    pub fn confirm(&self, hash: BlockHash) -> Vec<SavedBlock> {
        let txn = self.store.begin_write();
        let (txn, blocks) = self.confirm_max(txn, hash, 1024 * 128);
        txn.commit();
        blocks
    }

    /// Both stack and result set are bounded to limit maximum memory usage
    /// Callers must ensure that the target block was confirmed, and if not, call this function multiple times
    fn confirm_max(
        &self,
        txn: WriteTransaction,
        target_hash: BlockHash,
        max_blocks: usize,
    ) -> (WriteTransaction, Vec<SavedBlock>) {
        BlockCementer::new(&self.store, &self.constants, &self.stats).confirm(
            txn,
            target_hash,
            max_blocks,
        )
    }

    pub fn confirm_batch<'a, O>(
        &self,
        batch: impl IntoIterator<Item = &'a BlockHash>,
        stopped: &AtomicBool,
        max_blocks: usize,
        cementing_observer: &mut O,
    ) where
        O: CementingObserver,
    {
        let mut confirmed = Vec::new();
        let mut blocks_confirmed = 0;
        {
            let mut txn = self.store.begin_write();

            for confirmation_root in batch.into_iter() {
                let mut success = false;
                loop {
                    if txn.is_refresh_needed() {
                        txn = self.store.env.refresh(txn);
                    }

                    // Cementing deep dependency chains might take a long time, allow for graceful shutdown, ignore notifications
                    if stopped.load(Ordering::Relaxed) {
                        txn.commit();
                        return;
                    }

                    // Issue notifications here, so that `confirmed` set is not too large before we add more blocks
                    if blocks_confirmed >= max_blocks {
                        txn.commit();
                        blocks_confirmed = 0;
                        self.stats
                            .inc(StatType::ConfirmingSet, DetailType::NotifyIntermediate);
                        cementing_observer.batch_confirmed(confirmed);
                        confirmed = Vec::new();
                        txn = self.store.env.begin_write();
                    }

                    self.stats
                        .inc(StatType::ConfirmingSet, DetailType::Cementing);

                    // The block might be rolled back before it's fully confirmed
                    if !self.store.block.exists(&txn, confirmation_root) {
                        self.stats
                            .inc(StatType::ConfirmingSet, DetailType::MissingBlock);
                        break;
                    }

                    let (t, added) = self.confirm_max(txn, *confirmation_root, max_blocks);
                    txn = t;

                    if !added.is_empty() {
                        // Confirming this block may implicitly confirm more
                        self.stats.add(
                            StatType::ConfirmingSet,
                            DetailType::Cemented,
                            added.len() as u64,
                        );
                        blocks_confirmed += added.len();
                        for block in added {
                            confirmed.push((block, *confirmation_root));
                        }
                    } else if BorrowingConfirmedSet::new(&self.store, &txn)
                        .block_exists(&confirmation_root)
                    {
                        self.stats
                            .inc(StatType::ConfirmingSet, DetailType::AlreadyCemented);
                        cementing_observer.already_confirmed(confirmation_root);
                    }

                    success = {
                        if let Some(block) = self.store.block.get(&txn, confirmation_root) {
                            if let Some(conf_info) =
                                self.store.confirmation_height.get(&txn, &block.account())
                            {
                                block.height() <= conf_info.height
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    if success {
                        break;
                    }
                }

                if success {
                    self.stats
                        .inc(StatType::ConfirmingSet, DetailType::CementedHash);
                } else {
                    self.stats
                        .inc(StatType::ConfirmingSet, DetailType::CementingFailed);

                    // Requeue failed blocks for processing later
                    // Add them to the deferred set while still holding the exclusive database write transaction to avoid block processor races
                    cementing_observer.cementing_failed(confirmation_root);
                }
            }
            txn.commit();
        }

        if !confirmed.is_empty() {
            cementing_observer.batch_confirmed(confirmed);
        }
    }

    pub fn verify_votes(
        &self,
        candidates: VecDeque<(Root, BlockHash)>,
        is_final: bool,
    ) -> VecDeque<(Root, BlockHash)> {
        let verifier = VoteVerifier {
            constants: &self.constants,
            store: &self.store,
        };
        verifier.verify_votes(candidates, is_final)
    }

    pub fn block_count(&self) -> u64 {
        self.store.cache.block_count.load(Ordering::SeqCst)
    }

    pub fn simulate_block_count(&self, value: u64) {
        self.store.cache.block_count.store(value, Ordering::SeqCst)
    }

    pub fn confirmed_count(&self) -> u64 {
        self.store.cache.confirmed_count.load(Ordering::SeqCst)
    }

    pub fn simulate_confirmed_count(&self, value: u64) {
        self.store
            .cache
            .confirmed_count
            .store(value, Ordering::SeqCst)
    }

    pub fn account_count(&self) -> u64 {
        self.store.cache.account_count.load(Ordering::SeqCst)
    }

    pub fn backlog_count(&self) -> u64 {
        let blocks = self.block_count();
        let confirmed = self.confirmed_count();
        if blocks > confirmed {
            blocks - confirmed
        } else {
            0
        }
    }

    pub fn genesis(&self) -> &SavedBlock {
        &self.constants.genesis_block
    }

    pub fn work_thresholds(&self) -> &WorkThresholds {
        &self.constants.work
    }

    pub fn version(&self) -> u32 {
        let tx = self.store.begin_read();
        self.store.version.get(&tx).unwrap_or_default() as u32
    }

    pub fn store_vendor(&self) -> String {
        // hard coded version! TODO: read version from Cargo
        format!("lmdb-rkv {}.{}.{}", 0, 14, 0)
    }

    pub fn memory_stats(&self) -> anyhow::Result<MemoryStats> {
        self.store.memory_stats()
    }

    #[cfg(feature = "ledger_snapshots")]
    pub fn mark_fork(&self, root: &QualifiedRoot, snapshot_number: SnapshotNumber) {
        let mut tx = self.store.begin_write();
        self.store.forks.put(&mut tx, root, snapshot_number);
        tx.commit();
    }

    #[cfg(feature = "ledger_snapshots")]
    pub fn roll_back_forks_older_than(&self, snapshot_number: SnapshotNumber) {
        use tracing::warn;

        let forks_to_roll_back = self.find_forks_to_roll_back(snapshot_number);

        warn!("Rolling back these forks:");
        for fork in &forks_to_roll_back {
            warn!("fork hash: {:?}", fork);
        }

        for (fork_hash, _) in &forks_to_roll_back {
            if let Err(e) = self.roll_back(fork_hash) {
                use tracing::warn;
                warn!("Could not roll back fork: {e:?}")
            }
        }

        let mut txn = self.store.begin_write();
        for (_, root) in forks_to_roll_back {
            self.store.forks.del(&mut txn, &root);
        }
        txn.commit();
    }

    #[cfg(feature = "ledger_snapshots")]
    fn find_forks_to_roll_back(&self, snapshot_number: u32) -> Vec<(BlockHash, QualifiedRoot)> {
        let tx = self.store.begin_read();
        let any = BorrowingAnySet {
            constants: &self.constants,
            store: &self.store,
            tx: &tx,
        };

        self.store
            .forks
            .iter(&tx)
            .filter_map(|(root, snap_no)| {
                if snap_no < snapshot_number {
                    use crate::AnySet;
                    any.block_successor_by_qualified_root(&root)
                        .map(|h| (h, root))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        self.store.env.sync().expect("sync failed");
    }
}

impl ContainerInfoProvider for Ledger {
    fn container_info(&self) -> ContainerInfo {
        ContainerInfo::builder()
            .node("rep_weights", self.rep_weights.container_info())
            .finish()
    }
}

pub struct BatchProcessResult {
    pub processed: Vec<(Result<(), BlockError>, Option<SavedBlock>)>,
}

pub trait CementingObserver {
    fn already_confirmed(&mut self, hash: &BlockHash);
    fn cementing_failed(&mut self, hash: &BlockHash);
    fn batch_confirmed(&mut self, batch: Vec<(SavedBlock, BlockHash)>);
}

#[derive(Clone, Default)]
pub struct RollbackResults(Vec<RollbackResult>);

impl Deref for RollbackResults {
    type Target = Vec<RollbackResult>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RollbackResults {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl RollbackResults {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn affected_accounts(&self) -> impl Iterator<Item = Account> + use<'_> {
        self.iter().flat_map(|i| i.affected_accounts())
    }

    pub fn hashes(&self) -> impl Iterator<Item = BlockHash> + use<'_> {
        self.iter().flat_map(|i| i.hashes())
    }

    pub fn roots(&self) -> impl Iterator<Item = Root> + use<'_> {
        self.iter().flat_map(|i| i.roots())
    }
}

#[derive(Clone)]
pub struct RollbackResult {
    pub target_hash: BlockHash,
    pub target_root: QualifiedRoot,
    pub rolled_back: Vec<SavedBlock>,
    pub error: Option<RollbackError>,
}

impl RollbackResult {
    pub fn affected_accounts(&self) -> impl Iterator<Item = Account> + use<'_> {
        self.rolled_back.iter().map(|b| b.account())
    }

    pub fn hashes(&self) -> impl Iterator<Item = BlockHash> + use<'_> {
        self.rolled_back.iter().map(|b| b.hash())
    }

    pub fn roots(&self) -> impl Iterator<Item = Root> + use<'_> {
        self.rolled_back.iter().map(|b| b.root())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_variant_to_static_str() {
        let s: &'static str = BlockError::GapSource.into();
        assert_eq!(s, "gap_source");
    }
}
