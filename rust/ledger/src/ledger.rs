use super::DependentBlocksFinder;
use crate::{
    block_insertion::{BlockInserter, BlockValidatorFactory},
    ledger_set_confirmed::LedgerSetConfirmed,
    AnyReceivableIterator, BlockRollbackPerformer, DependentBlocks, GenerateCacheFlags,
    LedgerCache, LedgerConstants, LedgerSetAny, RepWeights, RepresentativeBlockFinder, WriteQueue,
};
use rand::{thread_rng, Rng};
use rsnano_core::{
    utils::{seconds_since_epoch, ContainerInfo, ContainerInfoComponent},
    Account, AccountInfo, Amount, BlockEnum, BlockHash, BlockSubType, ConfirmationHeightInfo,
    Epoch, Link, PendingInfo, PendingKey, Root,
};
use rsnano_store_lmdb::{
    ConfiguredAccountDatabaseBuilder, ConfiguredBlockDatabaseBuilder,
    ConfiguredConfirmationHeightDatabaseBuilder, ConfiguredPeersDatabaseBuilder,
    ConfiguredPendingDatabaseBuilder, ConfiguredPrunedDatabaseBuilder, LmdbAccountStore,
    LmdbBlockStore, LmdbConfirmationHeightStore, LmdbEnv, LmdbFinalVoteStore,
    LmdbOnlineWeightStore, LmdbPeerStore, LmdbPendingStore, LmdbPrunedStore, LmdbReadTransaction,
    LmdbRepWeightStore, LmdbStore, LmdbVersionStore, LmdbWriteTransaction, Transaction,
};
use std::{
    collections::{HashMap, VecDeque},
    mem::size_of,
    net::SocketAddrV6,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::SystemTime,
};

#[derive(PartialEq, Eq, Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum BlockStatus {
    Progress,      // Hasn't been seen before, signed correctly
    BadSignature,  // Signature was bad, forged or transmission error
    Old,           // Already seen and was valid
    NegativeSpend, // Malicious attempt to spend a negative amount
    Fork,          // Malicious fork based on previous
    /// Source block doesn't exist, has already been received, or requires an account upgrade (epoch blocks)
    Unreceivable,
    GapPrevious,         // Block marked as previous is unknown
    GapSource,           // Block marked as source is unknown
    GapEpochOpenPending, // Block marked as pending blocks required for epoch open block are unknown
    OpenedBurnAccount,   // Block attempts to open the burn account
    /// Balance and amount delta don't match
    BalanceMismatch,
    RepresentativeMismatch, // Representative is changed when it is not allowed
    BlockPosition,          // This block cannot follow the previous block
    InsufficientWork, // Insufficient work for this block, even though it passed the minimal validation
}

pub trait LedgerObserver: Send + Sync {
    fn blocks_cemented(&self, _cemented_count: u64) {}
    fn block_rolled_back(&self, _block_type: BlockSubType) {}
    fn block_rolled_back2(&self, _block: &BlockEnum, _is_epoch: bool) {}
    fn block_added(&self, _block: &BlockEnum, _is_epoch: bool) {}
}

pub struct NullLedgerObserver {}

impl NullLedgerObserver {
    pub fn new() -> Self {
        Self {}
    }
}

impl LedgerObserver for NullLedgerObserver {}

pub struct Ledger {
    pub store: Arc<LmdbStore>,
    pub cache: Arc<LedgerCache>,
    pub constants: LedgerConstants,
    pub observer: Arc<dyn LedgerObserver>,
    pruning: AtomicBool,
    bootstrap_weight_max_blocks: AtomicU64,
    pub check_bootstrap_weights: AtomicBool,
    pub bootstrap_weights: Mutex<HashMap<Account, Amount>>,
    pub write_queue: Arc<WriteQueue>,
}

pub struct NullLedgerBuilder {
    blocks: ConfiguredBlockDatabaseBuilder,
    accounts: ConfiguredAccountDatabaseBuilder,
    pending: ConfiguredPendingDatabaseBuilder,
    pruned: ConfiguredPrunedDatabaseBuilder,
    peers: ConfiguredPeersDatabaseBuilder,
    confirmation_height: ConfiguredConfirmationHeightDatabaseBuilder,
    min_rep_weight: Amount,
}

impl NullLedgerBuilder {
    fn new() -> Self {
        Self {
            blocks: ConfiguredBlockDatabaseBuilder::new(),
            accounts: ConfiguredAccountDatabaseBuilder::new(),
            pending: ConfiguredPendingDatabaseBuilder::new(),
            pruned: ConfiguredPrunedDatabaseBuilder::new(),
            peers: ConfiguredPeersDatabaseBuilder::new(),
            confirmation_height: ConfiguredConfirmationHeightDatabaseBuilder::new(),
            min_rep_weight: Amount::zero(),
        }
    }

    pub fn block(mut self, block: &BlockEnum) -> Self {
        self.blocks = self.blocks.block(block);
        self
    }

    pub fn blocks<'a>(mut self, blocks: impl IntoIterator<Item = &'a BlockEnum>) -> Self {
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

    pub fn pruned(mut self, hash: &BlockHash) -> Self {
        self.pruned = self.pruned.pruned(hash);
        self
    }

    pub fn finish(self) -> Ledger {
        let env = Arc::new(
            LmdbEnv::new_null_with()
                .configured_database(self.blocks.build())
                .configured_database(self.accounts.build())
                .configured_database(self.pending.build())
                .configured_database(self.pruned.build())
                .configured_database(self.confirmation_height.build())
                .configured_database(self.peers.build())
                .build(),
        );

        let store = LmdbStore {
            env: env.clone(),
            account: Arc::new(LmdbAccountStore::new(env.clone()).unwrap()),
            block: Arc::new(LmdbBlockStore::new(env.clone()).unwrap()),
            confirmation_height: Arc::new(LmdbConfirmationHeightStore::new(env.clone()).unwrap()),
            final_vote: Arc::new(LmdbFinalVoteStore::new(env.clone()).unwrap()),
            online_weight: Arc::new(LmdbOnlineWeightStore::new(env.clone()).unwrap()),
            peer: Arc::new(LmdbPeerStore::new(env.clone()).unwrap()),
            pending: Arc::new(LmdbPendingStore::new(env.clone()).unwrap()),
            pruned: Arc::new(LmdbPrunedStore::new(env.clone()).unwrap()),
            rep_weight: Arc::new(LmdbRepWeightStore::new(env.clone()).unwrap()),
            version: Arc::new(LmdbVersionStore::new(env.clone()).unwrap()),
        };
        Ledger::new(
            Arc::new(store),
            LedgerConstants::unit_test(),
            self.min_rep_weight,
        )
        .unwrap()
    }
}

impl Ledger {
    pub fn new_null() -> Self {
        Self::new(
            Arc::new(LmdbStore::create_null()),
            LedgerConstants::unit_test(),
            Amount::zero(),
        )
        .unwrap()
    }

    pub fn new_null_builder() -> NullLedgerBuilder {
        NullLedgerBuilder::new()
    }

    pub fn new(
        store: Arc<LmdbStore>,
        constants: LedgerConstants,
        min_rep_weight: Amount,
    ) -> anyhow::Result<Self> {
        Self::with_cache(store, constants, &GenerateCacheFlags::new(), min_rep_weight)
    }

    pub fn with_cache(
        store: Arc<LmdbStore>,
        constants: LedgerConstants,
        generate_cache: &GenerateCacheFlags,
        min_rep_weight: Amount,
    ) -> anyhow::Result<Self> {
        let mut ledger = Self {
            cache: Arc::new(LedgerCache::new(
                Arc::clone(&store.rep_weight),
                min_rep_weight,
            )),
            store,
            constants,
            observer: Arc::new(NullLedgerObserver::new()),
            pruning: AtomicBool::new(false),
            bootstrap_weight_max_blocks: AtomicU64::new(1),
            check_bootstrap_weights: AtomicBool::new(true),
            bootstrap_weights: Mutex::new(HashMap::new()),
            write_queue: Arc::new(WriteQueue::new(false)),
        };

        ledger.initialize(generate_cache)?;

        Ok(ledger)
    }

    pub fn set_observer(&mut self, observer: Arc<dyn LedgerObserver>) {
        self.observer = observer;
    }

    pub fn read_txn(&self) -> LmdbReadTransaction {
        self.store.tx_begin_read()
    }

    pub fn rw_txn(&self) -> LmdbWriteTransaction {
        self.store.tx_begin_write()
    }

    fn initialize(&mut self, generate_cache: &GenerateCacheFlags) -> anyhow::Result<()> {
        if self.store.account.begin(&self.read_txn()).is_end() {
            self.add_genesis_block(&mut self.rw_txn());
        }

        if generate_cache.reps || generate_cache.account_count || generate_cache.block_count {
            self.store.account.for_each_par(&|_txn, mut i, n| {
                let mut block_count = 0;
                let mut account_count = 0;
                let rep_weights =
                    RepWeights::new(Arc::clone(&self.store.rep_weight), Amount::zero());
                while !i.eq(&n) {
                    let info = i.current().unwrap().1;
                    block_count += info.block_count;
                    account_count += 1;
                    rep_weights.representation_put(info.representative, info.balance);
                    i.next();
                }
                self.cache
                    .block_count
                    .fetch_add(block_count, Ordering::SeqCst);
                self.cache
                    .account_count
                    .fetch_add(account_count, Ordering::SeqCst);
                self.cache.rep_weights.copy_from(&rep_weights);
            });
        }

        if generate_cache.cemented_count {
            self.store
                .confirmation_height
                .for_each_par(&|_txn, mut i, n| {
                    let mut cemented_count = 0;
                    while !i.eq(&n) {
                        cemented_count += i.current().unwrap().1.height;
                        i.next();
                    }
                    self.cache
                        .cemented_count
                        .fetch_add(cemented_count, Ordering::SeqCst);
                });
        }

        let transaction = self.store.tx_begin_read();
        self.cache
            .pruned_count
            .fetch_add(self.store.pruned.count(&transaction), Ordering::SeqCst);

        Ok(())
    }

    fn add_genesis_block(&self, txn: &mut LmdbWriteTransaction) {
        let genesis_block = self.constants.genesis.deref();
        let genesis_hash = genesis_block.hash();
        let genesis_account = self.constants.genesis_account;
        self.store.block.put(txn, genesis_block);

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
                representative: genesis_account,
                open_block: genesis_hash,
                balance: u128::MAX.into(),
                modified: seconds_since_epoch(),
                block_count: 1,
                epoch: Epoch::Epoch0,
            },
        );
        self.store.rep_weight.put(txn, genesis_account, Amount::MAX);
    }

    pub fn any(&self) -> LedgerSetAny {
        LedgerSetAny::new(&self.store)
    }

    pub fn confirmed(&self) -> LedgerSetConfirmed {
        LedgerSetConfirmed::new(&self.store)
    }

    pub fn pruning_enabled(&self) -> bool {
        self.pruning.load(Ordering::SeqCst)
    }

    pub fn enable_pruning(&self) {
        self.pruning.store(true, Ordering::SeqCst);
    }

    pub fn bootstrap_weight_max_blocks(&self) -> u64 {
        self.bootstrap_weight_max_blocks.load(Ordering::SeqCst)
    }

    pub fn set_bootstrap_weight_max_blocks(&self, max: u64) {
        self.bootstrap_weight_max_blocks
            .store(max, Ordering::SeqCst)
    }

    pub fn account_receivable(
        &self,
        txn: &dyn Transaction,
        account: &Account,
        only_confirmed: bool,
    ) -> Amount {
        let mut result = Amount::zero();

        for (key, info) in
            self.any()
                .account_receivable_upper_bound(txn, *account, BlockHash::zero())
        {
            if !only_confirmed
                || self
                    .confirmed()
                    .block_exists_or_pruned(txn, &key.send_block_hash)
            {
                result += info.amount;
            }
        }

        result
    }

    pub fn block_text(&self, hash: &BlockHash) -> anyhow::Result<String> {
        let txn = self.store.tx_begin_read();
        match self.any().get_block(&txn, hash) {
            Some(block) => block.to_json(),
            None => Ok(String::new()),
        }
    }

    pub fn hash_root_random(&self, txn: &dyn Transaction) -> Option<(BlockHash, Root)> {
        if !self.pruning_enabled() {
            self.store
                .block
                .random(txn)
                .map(|block| (block.hash(), block.root().into()))
        } else {
            let mut hash = BlockHash::zero();
            let count = self.cache.block_count.load(Ordering::SeqCst);
            let region = thread_rng().gen_range(0..count);
            // Pruned cache cannot guarantee that pruned blocks are already commited
            if region < self.pruned_count() {
                hash = self.store.pruned.random(txn).unwrap_or_default();
            }
            if hash.is_zero() {
                self.store
                    .block
                    .random(txn)
                    .map(|block| (block.hash(), block.root()))
            } else {
                Some((hash, Root::zero()))
            }
        }
    }

    /// Returns the cached vote weight for the given representative.
    /// If the weight is below the cache limit it returns 0.
    /// During bootstrap it returns the preconfigured bootstrap weights.
    pub fn weight(&self, account: &Account) -> Amount {
        if self.check_bootstrap_weights.load(Ordering::SeqCst) {
            if self.cache.block_count.load(Ordering::SeqCst) < self.bootstrap_weight_max_blocks() {
                let weights = self.bootstrap_weights.lock().unwrap();
                if let Some(&weight) = weights.get(account) {
                    return weight;
                }
            } else {
                self.check_bootstrap_weights.store(false, Ordering::SeqCst);
            }
        }

        self.cache.rep_weights.representation_get(account)
    }

    /// Returns the exact vote weight for the given representative by doing a database lookup
    pub fn weight_exact(&self, txn: &dyn Transaction, representative: Account) -> Amount {
        self.store
            .rep_weight
            .get(txn, representative)
            .unwrap_or_default()
    }

    /// Return latest root for account, account number if there are no blocks for this account
    pub fn latest_root(&self, txn: &dyn Transaction, account: &Account) -> Root {
        match self.account_info(txn, account) {
            Some(info) => info.head.into(),
            None => account.into(),
        }
    }

    pub fn version(&self, txn: &dyn Transaction, hash: &BlockHash) -> Epoch {
        self.any()
            .get_block(txn, hash)
            .map(|block| block.epoch())
            .unwrap_or(Epoch::Epoch0)
    }

    pub fn is_epoch_link(&self, link: &Link) -> bool {
        self.constants.epochs.is_epoch_link(link)
    }

    /// Given the block hash of a send block, find the associated receive block that receives that send.
    /// The send block hash is not checked in any way, it is assumed to be correct.
    /// Return the receive block on success and None on failure
    pub fn find_receive_block_by_send_hash(
        &self,
        txn: &dyn Transaction,
        destination: &Account,
        send_block_hash: &BlockHash,
    ) -> Option<BlockEnum> {
        // get the cemented frontier
        let info = self.store.confirmation_height.get(txn, destination)?;
        let mut possible_receive_block = self.any().get_block(txn, &info.frontier);

        // walk down the chain until the source field of a receive block matches the send block hash
        while let Some(current) = possible_receive_block {
            if current.is_receive() && Some(*send_block_hash) == current.source() {
                // we have a match
                return Some(current);
            }

            possible_receive_block = self.any().get_block(txn, &current.previous());
        }

        None
    }

    pub fn epoch_link(&self, epoch: Epoch) -> Option<Link> {
        self.constants.epochs.link(epoch).cloned()
    }

    pub fn update_account(
        &self,
        txn: &mut LmdbWriteTransaction,
        account: &Account,
        old_info: &AccountInfo,
        new_info: &AccountInfo,
    ) {
        if !new_info.head.is_zero() {
            if old_info.head.is_zero() && new_info.open_block == new_info.head {
                self.cache.account_count.fetch_add(1, Ordering::SeqCst);
            }
            if !old_info.head.is_zero() && old_info.epoch != new_info.epoch {
                // store.account ().put won't erase existing entries if they're in different tables
                self.store.account.del(txn, account);
            }
            self.store.account.put(txn, account, new_info);
        } else {
            debug_assert!(!self.store.confirmation_height.exists(txn, account));
            self.store.account.del(txn, account);
            debug_assert!(self.cache.account_count.load(Ordering::SeqCst) > 0);
            self.cache.account_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn pruning_action(
        &self,
        txn: &mut LmdbWriteTransaction,
        hash: &BlockHash,
        batch_size: u64,
    ) -> u64 {
        let mut pruned_count = 0;
        let mut hash = *hash;
        let genesis_hash = self.constants.genesis.hash();

        while !hash.is_zero() && hash != genesis_hash {
            if let Some(block) = self.any().get_block(txn, &hash) {
                assert!(self.confirmed().block_exists_or_pruned(txn, &hash));
                self.store.block.del(txn, &hash);
                self.store.pruned.put(txn, &hash);
                hash = block.previous();
                pruned_count += 1;
                self.cache.pruned_count.fetch_add(1, Ordering::SeqCst);
                if pruned_count % batch_size == 0 {
                    txn.commit();
                    txn.renew();
                }
            } else if self.store.pruned.exists(txn, &hash) {
                hash = BlockHash::zero();
            } else {
                panic!("Error finding block for pruning");
            }
        }

        pruned_count
    }

    pub fn bootstrap_weight_reached(&self) -> bool {
        self.cache.block_count.load(Ordering::SeqCst) >= self.bootstrap_weight_max_blocks()
    }

    pub fn dependent_blocks(&self, txn: &dyn Transaction, block: &BlockEnum) -> DependentBlocks {
        DependentBlocksFinder::new(self, txn).find_dependent_blocks(block)
    }

    pub fn dependents_confirmed(&self, txn: &dyn Transaction, block: &BlockEnum) -> bool {
        self.dependent_blocks(txn, block)
            .iter()
            .all(|hash| self.is_dependency_confirmed(txn, hash))
    }

    fn is_dependency_confirmed(&self, txn: &dyn Transaction, dependency: &BlockHash) -> bool {
        if !dependency.is_zero() {
            self.confirmed().block_exists_or_pruned(txn, dependency)
        } else {
            true
        }
    }

    /// Rollback blocks until `block' doesn't exist or it tries to penetrate the confirmation height
    pub fn rollback(
        &self,
        txn: &mut LmdbWriteTransaction,
        block: &BlockHash,
    ) -> anyhow::Result<Vec<BlockEnum>> {
        BlockRollbackPerformer::new(self, txn).roll_back(block)
    }

    /// Returns the latest block with representative information
    pub fn representative_block_hash(&self, txn: &dyn Transaction, hash: &BlockHash) -> BlockHash {
        let hash = RepresentativeBlockFinder::new(txn, self.store.as_ref()).find_rep_block(*hash);
        debug_assert!(hash.is_zero() || self.store.block.exists(txn, &hash));
        hash
    }

    pub fn process(
        &self,
        txn: &mut LmdbWriteTransaction,
        block: &mut BlockEnum,
    ) -> Result<(), BlockStatus> {
        let validator = BlockValidatorFactory::new(self, txn, block).create_validator();
        let instructions = validator.validate()?;
        BlockInserter::new(self, txn, block, &instructions).insert();
        Ok(())
    }

    pub fn get_block(&self, txn: &dyn Transaction, hash: &BlockHash) -> Option<BlockEnum> {
        self.store.block.get(txn, hash)
    }

    pub fn account_info(
        &self,
        transaction: &dyn Transaction,
        account: &Account,
    ) -> Option<AccountInfo> {
        self.store.account.get(transaction, account)
    }

    pub fn get_confirmation_height(
        &self,
        txn: &dyn Transaction,
        account: &Account,
    ) -> Option<ConfirmationHeightInfo> {
        self.store.confirmation_height.get(txn, account)
    }

    pub fn confirm(&self, txn: &mut LmdbWriteTransaction, hash: BlockHash) -> VecDeque<BlockEnum> {
        let mut result = VecDeque::new();
        let mut stack = Vec::new();
        stack.push(hash);
        while let Some(&hash) = stack.last() {
            let block = self.any().get_block(txn, &hash).unwrap();
            let dependents = self.dependent_blocks(txn, &block);
            for dependent in dependents.iter() {
                if !self.confirmed().block_exists_or_pruned(txn, dependent) {
                    stack.push(*dependent);
                }
            }

            if stack.last() == Some(&hash) {
                stack.pop();
                if !self.confirmed().block_exists_or_pruned(txn, &hash) {
                    self.confirm_block(txn, &block);
                    result.push_back(block);
                }
            } else {
                // unconfirmed dependencies were added
            }
        }
        result
    }

    fn confirm_block(&self, txn: &mut LmdbWriteTransaction, block: &BlockEnum) {
        debug_assert!(
            (self
                .store
                .confirmation_height
                .get(txn, &block.account())
                .is_none()
                && block.sideband().unwrap().height == 1)
                || self
                    .store
                    .confirmation_height
                    .get(txn, &block.account())
                    .unwrap()
                    .height
                    + 1
                    == block.sideband().unwrap().height
        );
        let info = ConfirmationHeightInfo::new(block.sideband().unwrap().height, block.hash());
        self.store
            .confirmation_height
            .put(txn, &block.account(), &info);
        self.cache.cemented_count.fetch_add(1, Ordering::SeqCst);
        self.observer.blocks_cemented(1);
    }

    /// Returns whether there are any receivable entries for 'account'
    pub fn receivable_any(&self, txn: &dyn Transaction, account: Account) -> bool {
        self.any()
            .account_receivable_upper_bound(txn, account, BlockHash::zero())
            .next()
            .is_some()
    }

    pub fn cemented_count(&self) -> u64 {
        self.cache.cemented_count.load(Ordering::SeqCst)
    }

    pub fn block_count(&self) -> u64 {
        self.cache.block_count.load(Ordering::SeqCst)
    }

    pub fn account_count(&self) -> u64 {
        self.cache.account_count.load(Ordering::SeqCst)
    }

    pub fn pruned_count(&self) -> u64 {
        self.cache.pruned_count.load(Ordering::SeqCst)
    }

    pub fn collect_container_info(&self, name: impl Into<String>) -> ContainerInfoComponent {
        ContainerInfoComponent::Composite(
            name.into(),
            vec![
                ContainerInfoComponent::Leaf(ContainerInfo {
                    name: "bootstrap_weights".to_string(),
                    count: self.bootstrap_weights.lock().unwrap().len(),
                    sizeof_element: size_of::<Account>() + size_of::<Amount>(),
                }),
                self.cache.rep_weights.collect_container_info("rep_weights"),
            ],
        )
    }
}
