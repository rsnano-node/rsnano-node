use std::{
    collections::{HashMap, HashSet},
    fs::Permissions,
    mem::size_of,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

use rand::{Rng, seq::IndexedRandom};
use tracing::{debug, info, warn};

use rsnano_ledger::{AnySet, Ledger, LedgerSet};
use rsnano_nullable_clock::SteadyClock;
use rsnano_nullable_lmdb::{
    DatabaseFlags, LmdbDatabase, LmdbEnvironment, Transaction, WriteFlags, WriteTransaction,
};
use rsnano_store_lmdb::{KeyType, LmdbIterator, LmdbWalletStore};
use rsnano_types::{
    Account, Amount, Block, BlockDetails, BlockHash, Epoch, KeyDerivationFunction, Link,
    NetworkType, PendingKey, PrivateKey, PublicKey, RawKey, Root, SavedBlock, StateBlockArgs,
    WalletId, WorkNonce, WorkRequest,
};
use rsnano_utils::{
    CancellationToken,
    container_info::{ContainerInfo, ContainerInfoProvider},
    ticker::Tickable,
};
use rsnano_work::WorkThresholds;

use super::{
    BlockPromise, MultiBlockPromise, Wallet, WalletsConfig, WalletsError,
    delayed_work_queue::DelayedWorkQueue,
};

enum PreparedSend {
    Cached(SavedBlock),
    New(Block, BlockDetails),
}

pub struct Wallets {
    db: Option<LmdbDatabase>,
    send_action_ids_handle: Option<LmdbDatabase>,
    env: Arc<LmdbEnvironment>,
    wallets: Mutex<HashMap<WalletId, Arc<Wallet>>>,
    wallets_config: WalletsConfig,
    ledger: Arc<Ledger>,
    work_thresholds: WorkThresholds,
    delayed_work: Mutex<DelayedWorkQueue>,
    kdf: KeyDerivationFunction,
    work_queue: Mutex<Option<mpsc::Sender<WorkRequest>>>,
    block_queue: Mutex<Option<mpsc::Sender<Block>>>,
    waiting_for_work: Mutex<HashMap<Root, WorkItem>>,
    waiting_for_processor: Mutex<HashMap<BlockHash, (BlockPromise, bool, Arc<Wallet>)>>,
    clock: Arc<SteadyClock>,
}

enum WorkItem {
    BlockWork(Block, BlockPromise, bool, Arc<Wallet>),
    Cache(WalletId, Account),
}

impl Wallets {
    pub fn new(
        wallets_config: WalletsConfig,
        env: Arc<LmdbEnvironment>,
        ledger: Arc<Ledger>,
        work: WorkThresholds,
        clock: Arc<SteadyClock>,
    ) -> Self {
        let kdf = KeyDerivationFunction::new(wallets_config.kdf_work);

        Self {
            db: None,
            send_action_ids_handle: None,
            wallets: Mutex::new(HashMap::new()),
            env,
            wallets_config,
            ledger: Arc::clone(&ledger),
            work_thresholds: work,
            delayed_work: Mutex::new(DelayedWorkQueue::default()),
            kdf: kdf.clone(),
            work_queue: Mutex::new(None),
            block_queue: Mutex::new(None),
            waiting_for_work: Mutex::new(HashMap::new()),
            waiting_for_processor: Mutex::new(HashMap::new()),
            clock,
        }
    }

    pub fn new_null() -> Self {
        let network = NetworkType::NanoLiveNetwork;
        let env = Arc::new(LmdbEnvironment::new_null());
        let ledger = Arc::new(Ledger::new_null());
        let wallets_config = WalletsConfig::default();
        let work = WorkThresholds::default_for(network);
        let clock = Arc::new(SteadyClock::new_null());
        Self::new(wallets_config, env, ledger, work, clock)
    }

    pub fn stop(&self) {
        drop(self.work_queue.lock().unwrap().take());
        drop(self.block_queue.lock().unwrap().take());
        self.env.sync().expect("sync failed");
    }

    pub fn set_work_queue(&self, tx_work: mpsc::Sender<WorkRequest>) {
        *self.work_queue.lock().unwrap() = Some(tx_work);
    }

    pub fn set_block_queue(&self, tx_block: mpsc::Sender<Block>) {
        *self.block_queue.lock().unwrap() = Some(tx_block);
    }

    pub fn delayed_work_count(&self) -> usize {
        self.delayed_work.lock().unwrap().len()
    }

    fn random_representative(&self) -> PublicKey {
        self.wallets_config
            .preconfigured_representatives
            .choose(&mut rand::rng())
            .cloned()
            .unwrap_or(self.ledger.constants.genesis_account.into())
    }

    pub fn enter_initial_password(&self, wallet: &Arc<Wallet>) {
        let password = wallet.store.password();
        if password.is_zero() {
            let mut txn = self.env.begin_write();
            if wallet.store.valid_password(&txn) {
                // Newly created wallets have a zero key
                let _ = wallet.store.rekey(&mut txn, "");
            } else {
                let _ = self.enter_password_wallet(wallet, &txn, "");
            }
            txn.commit();
        }
    }

    fn enter_password_wallet(
        &self,
        wallet: &Arc<Wallet>,
        wallet_tx: &dyn Transaction,
        password: &str,
    ) -> Result<(), ()> {
        if !wallet.store.attempt_password(wallet_tx, password) {
            warn!("Invalid password, wallet locked");
            Err(())
        } else {
            info!("Wallet unlocked");
            Ok(())
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        {
            let mut guard = self.wallets.lock().unwrap();
            self.db = Some(self.env.create_db(None, DatabaseFlags::empty())?);
            self.send_action_ids_handle = Some(
                self.env
                    .create_db(Some("send_action_ids"), DatabaseFlags::empty())?,
            );

            let wallet_ids = {
                let txn = self.env.begin_write();
                let ids = self.get_wallet_ids_with_tx(&txn);
                txn.commit();
                ids
            };

            for id in wallet_ids {
                assert!(!guard.contains_key(&id));
                let representative = self.random_representative();
                let text = PathBuf::from(id.encode_hex());
                let wallet = Wallet::new(
                    id,
                    &self.env,
                    self.wallets_config.password_fanout,
                    self.kdf.clone(),
                    representative,
                    &text,
                )?;

                guard.insert(id, Arc::new(wallet));
            }

            info!("Found {} wallet(s)", guard.len());
            for i in guard.keys() {
                info!("Wallet: {}", i);
            }

            for (_, wallet) in guard.iter() {
                self.enter_initial_password(wallet);
            }
        }

        Ok(())
    }

    fn iter_wallets<'tx>(&self, tx: &'tx dyn Transaction) -> impl Iterator<Item = WalletId> + 'tx {
        let cursor = tx
            .open_ro_cursor(self.db.unwrap())
            .expect("Could not read from wallets db");

        LmdbIterator::new(cursor, |k, _| {
            // wallet tables are identified by their wallet id hex string which is 64 bytes
            let key = if k.len() == 64 {
                WalletId::decode_hex(std::str::from_utf8(k).unwrap()).unwrap()
            } else {
                WalletId::ZERO
            };
            (key, ())
        })
        .filter_map(|(k, _)| if k.is_zero() { None } else { Some(k) })
    }

    pub fn wallet_ids(&self) -> Vec<WalletId> {
        let txn = self.env.begin_read();
        let ids = self.get_wallet_ids_with_tx(&txn);
        txn.commit();
        ids
    }

    pub fn get_wallet_ids(&self) -> Vec<WalletId> {
        let txn = self.env.begin_read();
        let ids = self.iter_wallets(&txn).collect::<Vec<_>>();
        txn.commit();
        ids
    }

    pub fn get_wallet_ids_with_tx(&self, tx: &dyn Transaction) -> Vec<WalletId> {
        self.iter_wallets(tx).collect()
    }

    pub fn get_block_hash(
        &self,
        txn: &dyn Transaction,
        id: &str,
    ) -> anyhow::Result<Option<BlockHash>> {
        match txn.get(self.send_action_ids_handle.unwrap(), id.as_bytes()) {
            Ok(bytes) => Ok(Some(
                BlockHash::from_slice(bytes).ok_or_else(|| anyhow!("invalid block hash"))?,
            )),
            Err(rsnano_nullable_lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_block_hash(
        &self,
        txn: &mut WriteTransaction,
        id: &str,
        hash: &BlockHash,
    ) -> anyhow::Result<()> {
        txn.put(
            self.send_action_ids_handle.unwrap(),
            id.as_bytes(),
            hash.as_bytes(),
            WriteFlags::empty(),
        )?;
        Ok(())
    }

    pub fn clear_send_ids(&self) {
        let mut txn = self.env.begin_write();
        txn.clear_db(self.send_action_ids_handle.unwrap()).unwrap();
        txn.commit();
    }

    pub fn get_all_pub_keys(&self) -> Vec<PublicKey> {
        let mut wallet_keys = Vec::new();
        {
            let wallets_guard = self.wallets.lock().unwrap();
            let txn = self.env.begin_read();
            for (_, wallet) in wallets_guard.iter() {
                for (pub_key, _) in wallet.store.iter(&txn) {
                    wallet_keys.push(pub_key);
                }
            }
            txn.commit();
        }

        wallet_keys
    }

    pub fn get_all_private_keys(&self) -> Vec<PrivateKey> {
        let mut all_priv_keys: Vec<PrivateKey> = Vec::new();
        {
            let txn = self.env.begin_read();
            let lock = self.wallets.lock().unwrap();
            for (_, wallet) in lock.iter() {
                if wallet.store.valid_password(&txn) {
                    for (pub_key, _) in wallet.store.iter(&txn) {
                        if let Ok(prv_key) = wallet.store.fetch(&txn, &pub_key) {
                            all_priv_keys.push(prv_key.into());
                        }
                    }
                }
            }
            txn.commit();
        }
        all_priv_keys
    }

    pub fn get_wallet(&self, wallet_id: &WalletId) -> Option<Arc<Wallet>> {
        self.wallets.lock().unwrap().get(wallet_id).cloned()
    }

    fn get_wallet_guard<'a>(
        guard: &'a HashMap<WalletId, Arc<Wallet>>,
        wallet_id: &WalletId,
    ) -> Result<&'a Arc<Wallet>, WalletsError> {
        guard.get(wallet_id).ok_or(WalletsError::WalletNotFound)
    }

    pub fn insert_watch(
        &self,
        wallet_id: &WalletId,
        accounts: &[Account],
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }

        for account in accounts {
            if wallet
                .store
                .insert_watch(&mut txn, &account.into())
                .is_err()
            {
                return Err(WalletsError::BadPublicKey);
            }
        }
        txn.commit();

        Ok(())
    }

    pub fn valid_password(&self, wallet_id: &WalletId) -> Result<bool, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_read();
        let valid = wallet.store.valid_password(&txn);
        txn.commit();
        Ok(valid)
    }

    pub fn attempt_password(
        &self,
        wallet_id: &WalletId,
        password: impl AsRef<str>,
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_write();
        if wallet.store.attempt_password(&txn, password.as_ref()) {
            txn.commit();
            Ok(())
        } else {
            Err(WalletsError::InvalidPassword)
        }
    }

    pub fn lock(&self, wallet_id: &WalletId) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        wallet.store.lock();
        Ok(())
    }

    pub fn rekey(
        &self,
        wallet_id: &WalletId,
        password: impl AsRef<str>,
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }

        let result = wallet
            .store
            .rekey(&mut txn, password.as_ref())
            .map_err(|_| WalletsError::Generic);
        txn.commit();
        result
    }

    pub fn exists(&self, pub_key: &PublicKey) -> bool {
        let guard = self.wallets.lock().unwrap();
        let txn = self.env.begin_read();
        let exists = guard
            .values()
            .any(|wallet| wallet.store.exists(&txn, pub_key));
        txn.commit();
        exists
    }

    pub fn reload(&self) {
        let mut guard = self.wallets.lock().unwrap();
        let mut stored_items = HashSet::new();

        let wallet_ids = {
            let txn = self.env.begin_write();
            let ids = self.get_wallet_ids_with_tx(&txn);
            txn.commit();
            ids
        };

        for id in wallet_ids {
            guard.entry(id).or_insert_with(|| {
                // New wallet
                let text = PathBuf::from(id.encode_hex());
                let representative = self.random_representative();
                Wallet::new(
                    id,
                    &self.env,
                    self.wallets_config.password_fanout,
                    self.kdf.clone(),
                    representative,
                    &text,
                )
                .expect("wallet creation should succeed")
                .into()
            });

            // List of wallets on disk
            stored_items.insert(id);
        }
        // Delete non existing wallets from memory
        let mut deleted_items = Vec::new();
        for &id in guard.keys() {
            if !stored_items.contains(&id) {
                deleted_items.push(id);
            }
        }
        for i in &deleted_items {
            guard.remove(i);
        }
    }

    pub fn wallet_exists(&self, wallet_id: &WalletId) -> bool {
        self.wallets.lock().unwrap().contains_key(wallet_id)
    }

    pub fn destroy(&self, id: &WalletId) {
        let mut guard = self.wallets.lock().unwrap();
        let mut txn = self.env.begin_write();
        let wallet = guard.remove(id).unwrap();
        wallet.store.destroy(&mut txn);
        txn.commit();
    }

    pub fn remove_key(
        &self,
        wallet_id: &WalletId,
        pub_key: &PublicKey,
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        if wallet.store.find(&txn, pub_key).is_none() {
            return Err(WalletsError::AccountNotFound);
        }
        wallet.store.erase(&mut txn, pub_key);
        txn.commit();
        Ok(())
    }

    pub fn work_set(
        &self,
        wallet_id: &WalletId,
        pub_key: &PublicKey,
        work: WorkNonce,
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if wallet.store.find(&txn, pub_key).is_none() {
            return Err(WalletsError::AccountNotFound);
        }
        wallet.store.work_put(&mut txn, pub_key, work);
        txn.commit();
        Ok(())
    }

    pub fn move_accounts(
        &self,
        source_id: &WalletId,
        target_id: &WalletId,
        accounts: &[PublicKey],
    ) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let source = Self::get_wallet_guard(&guard, source_id)?;
        let target = Self::get_wallet_guard(&guard, target_id)?;
        let txn = self.env.begin_read();
        let is_locked = !source.store.valid_password(&txn) || !target.store.valid_password(&txn);
        txn.commit();

        if is_locked {
            return Err(WalletsError::WalletLocked);
        }

        let mut txn = self.env.begin_write();
        let result = target
            .store
            .move_keys(&mut txn, &source.store, accounts)
            .map_err(|_| WalletsError::AccountNotFound);
        txn.commit();
        result
    }

    pub fn backup(&self, path: &Path) -> anyhow::Result<()> {
        let guard = self.wallets.lock().unwrap();
        let txn = self.env.begin_read();
        for (id, wallet) in guard.iter() {
            std::fs::create_dir_all(path)?;
            std::fs::set_permissions(path, Permissions::from_mode(0o700))?;
            let mut backup_path = PathBuf::from(path);
            backup_path.push(format!("{}.json", id));
            wallet.store.write_backup(&txn, &backup_path)?;
        }
        txn.commit();
        Ok(())
    }

    pub fn deterministic_index_get(&self, wallet_id: &WalletId) -> Result<u32, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_read();
        let index = wallet.store.deterministic_index_get(&txn);
        txn.commit();
        Ok(index)
    }

    fn prepare_send(
        &self,
        tx: &dyn Transaction,
        wallet: &Arc<Wallet>,
        source: Account,
        destination: Account,
        amount: Amount,
        mut work: WorkNonce,
    ) -> anyhow::Result<PreparedSend> {
        let any = self.ledger.any();
        if !wallet.store.valid_password(tx) {
            bail!("invalid password");
        }
        let balance = any.account_balance(&source);

        if balance.is_zero() || balance < amount {
            bail!("insufficient balance");
        }

        let info = any.get_account(&source).unwrap();
        let prv_key_raw = wallet.store.fetch(tx, &source.into()).unwrap();
        if work.is_zero() {
            work = wallet
                .store
                .work_get(tx, &source.into())
                .unwrap_or_default();
        }
        let priv_key = PrivateKey::from(prv_key_raw);
        let state_block: Block = StateBlockArgs {
            key: &priv_key,
            previous: info.head,
            representative: info.representative,
            balance: balance - amount,
            link: destination.into(),
            work,
        }
        .into();
        let details = BlockDetails::new(info.epoch, true, false, false);
        Ok(PreparedSend::New(state_block, details))
    }

    fn prepare_send_with_id(
        &self,
        txn: &mut WriteTransaction,
        id: &str,
        wallet: &Arc<Wallet>,
        source: Account,
        destination: Account,
        amount: Amount,
        mut work: WorkNonce,
    ) -> anyhow::Result<PreparedSend> {
        let any = self.ledger.any();

        let block = self
            .get_block_hash(txn, id)?
            .map(|hash| any.get_block(&hash).expect("block should exist"));

        if let Some(block) = block {
            Ok(PreparedSend::Cached(block))
        } else {
            if !wallet.store.valid_password(txn) {
                bail!("invalid password");
            }

            let balance = any.account_balance(&source);

            if balance.is_zero() || balance < amount {
                bail!("insufficient balance");
            }

            let info = any.get_account(&source).unwrap();
            let prv_key_raw = wallet.store.fetch(txn, &source.into()).unwrap();
            if work.is_zero() {
                work = wallet
                    .store
                    .work_get(txn, &source.into())
                    .unwrap_or_default();
            }
            let priv_key = PrivateKey::from(prv_key_raw);
            let state_block: Block = StateBlockArgs {
                key: &priv_key,
                previous: info.head,
                representative: info.representative,
                balance: balance - amount,
                link: destination.into(),
                work,
            }
            .into();
            let details = BlockDetails::new(info.epoch, true, false, false);
            self.set_block_hash(txn, id, &state_block.hash())?;
            Ok(PreparedSend::New(state_block, details))
        }
    }

    pub fn work_get(&self, wallet_id: &WalletId, pub_key: &PublicKey) -> WorkNonce {
        let guard = self.wallets.lock().unwrap();
        let Some(wallet) = guard.get(wallet_id) else {
            return 1.into();
        };
        let txn = self.env.begin_read();
        let work = wallet.store.work_get(&txn, pub_key).unwrap_or(1.into());
        txn.commit();
        work
    }

    pub fn work_get2(
        &self,
        wallet_id: &WalletId,
        pub_key: &PublicKey,
    ) -> Result<WorkNonce, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let txn = self.env.begin_read();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        if wallet.store.find(&txn, pub_key).is_none() {
            return Err(WalletsError::AccountNotFound);
        }
        Ok(wallet.store.work_get(&txn, pub_key).unwrap_or(1.into()))
    }

    pub fn get_accounts(&self, max_results: usize) -> Vec<Account> {
        let mut accounts = Vec::new();
        let guard = self.wallets.lock().unwrap();
        let txn = self.env.begin_read();
        for wallet in guard.values() {
            for (pub_key, _) in wallet.store.iter(&txn) {
                if accounts.len() >= max_results {
                    break;
                }

                accounts.push(pub_key.into());
            }
        }
        txn.commit();
        accounts
    }

    pub fn get_accounts_of_wallet(
        &self,
        wallet_id: &WalletId,
    ) -> Result<Vec<Account>, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_read();
        let mut accounts = Vec::new();
        for (account, _) in wallet.store.iter(&txn) {
            accounts.push(account.into());
        }
        txn.commit();
        Ok(accounts)
    }

    pub fn fetch(&self, wallet_id: &WalletId, pub_key: &PublicKey) -> Result<RawKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_read();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        if wallet.store.find(&txn, pub_key).is_none() {
            return Err(WalletsError::AccountNotFound);
        }
        let result = wallet
            .store
            .fetch(&txn, pub_key)
            .map_err(|_| WalletsError::Generic);
        txn.commit();
        result
    }

    pub fn import(&self, wallet_id: WalletId, json: &str) -> anyhow::Result<()> {
        let _guard = self.wallets.lock().unwrap();
        let _wallet = Wallet::new_from_json(
            wallet_id,
            &self.env,
            self.wallets_config.password_fanout,
            self.kdf.clone(),
            &PathBuf::from(wallet_id.to_string()),
            json,
        )?;
        Ok(())
    }

    pub fn import_replace(
        &self,
        wallet_id: WalletId,
        json: &str,
        password: &str,
    ) -> anyhow::Result<()> {
        let guard = self.wallets.lock().unwrap();
        let existing = guard
            .get(&wallet_id)
            .ok_or_else(|| anyhow!("wallet not found"))?;
        let id = WalletId::from_bytes(rand::rng().random());
        let temp = LmdbWalletStore::new_from_json(
            1,
            self.kdf.clone(),
            &self.env,
            &PathBuf::from(id.to_string()),
            json,
        )?;

        let mut txn = self.env.begin_write();
        let result = if temp.attempt_password(&txn, password) {
            existing.store.import(&mut txn, &temp)
        } else {
            Err(anyhow!("bad password"))
        };
        temp.destroy(&mut txn);
        txn.commit();
        result
    }

    pub fn get_seed(&self, wallet_id: WalletId) -> Result<RawKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, &wallet_id)?;
        let txn = self.env.begin_read();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        let seed = wallet.store.seed(&txn);
        txn.commit();
        Ok(seed)
    }

    pub fn key_type(&self, wallet_id: WalletId, pub_key: &PublicKey) -> KeyType {
        let guard = self.wallets.lock().unwrap();
        match guard.get(&wallet_id) {
            Some(wallet) => {
                let txn = self.env.begin_read();
                let key_type = wallet.store.get_key_type(&txn, pub_key);
                txn.commit();
                key_type
            }
            None => KeyType::Unknown,
        }
    }

    pub fn get_representative(&self, wallet_id: WalletId) -> Result<PublicKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, &wallet_id)?;
        let txn = self.env.begin_read();
        Ok(wallet.store.representative(&txn))
    }

    pub fn decrypt(&self, wallet_id: WalletId) -> Result<Vec<(PublicKey, RawKey)>, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, &wallet_id)?;
        let txn = self.env.begin_read();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }

        let mut result = Vec::new();
        for (account, _) in wallet.store.iter(&txn) {
            let key = wallet
                .store
                .fetch(&txn, &account)
                .map_err(|_| WalletsError::Generic)?;
            result.push((account, key));
        }
        txn.commit();

        Ok(result)
    }

    pub fn serialize(&self, wallet_id: WalletId) -> Result<String, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Self::get_wallet_guard(&guard, &wallet_id)?;
        let txn = self.env.begin_read();
        let json = wallet.store.serialize_json(&txn);
        txn.commit();
        Ok(json)
    }

    pub fn wallet_count(&self) -> usize {
        self.wallets.lock().unwrap().len()
    }

    fn enqueue_block_work(
        &self,
        request: WorkRequest,
        block: Block,
        block_promise: BlockPromise,
        generate_work: bool,
        wallet: Arc<Wallet>,
    ) {
        let guard = self.work_queue.lock().unwrap();
        let Some(work_queue) = guard.as_ref() else {
            block_promise.set_result(Err(WalletsError::Generic));
            return;
        };

        let root = block.root();
        let old = self.waiting_for_work.lock().unwrap().insert(
            root,
            WorkItem::BlockWork(block, block_promise, generate_work, wallet),
        );
        if let Some(old) = old {
            warn!("Work item for same root replaced!");
            if let WorkItem::BlockWork(_, promise, _, _) = old {
                promise.set_result(Err(WalletsError::Generic));
            }
        }

        let result = work_queue.send(request);
        if result.is_err()
            && let Some(WorkItem::BlockWork(_, block_promise, _, _)) =
                self.waiting_for_work.lock().unwrap().remove(&root)
        {
            block_promise.set_result(Err(WalletsError::Generic));
        }
    }

    fn enqueue_cache_work(&self, request: WorkRequest, wallet_id: WalletId, account: Account) {
        let guard = self.work_queue.lock().unwrap();
        let Some(work_queue) = guard.as_ref() else {
            return;
        };

        let root = request.root;

        self.waiting_for_work
            .lock()
            .unwrap()
            .insert(root, WorkItem::Cache(wallet_id, account));

        let result = work_queue.send(request);
        if result.is_err() {
            self.waiting_for_work.lock().unwrap().remove(&root);
        }
    }

    fn enqueue_block(
        &self,
        block: Block,
        block_promise: BlockPromise,
        generate_work: bool,
        wallet: Arc<Wallet>,
    ) {
        let hash = block.hash();
        let queue = self.block_queue.lock().unwrap();
        if let Some(queue) = queue.as_ref() {
            self.waiting_for_processor
                .lock()
                .unwrap()
                .insert(hash, (block_promise.clone(), generate_work, wallet));

            if queue.send(block).is_err() {
                self.waiting_for_processor.lock().unwrap().remove(&hash);
                block_promise.set_result(Err(WalletsError::Generic));
            }
        } else {
            block_promise.set_result(Err(WalletsError::Generic));
        }
    }

    fn tick(&self) {
        let now = self.clock.now();
        let mut delayed = self.delayed_work.lock().unwrap();
        while let Some((wallet_id, account, root)) = delayed.pop(now) {
            self.enqueue_cache_work(
                WorkRequest {
                    root,
                    difficulty: self.work_thresholds.threshold_base(),
                },
                wallet_id,
                account,
            );
        }
    }

    pub fn deterministic_insert(
        &self,
        wallet: &Arc<Wallet>,
        tx: &mut WriteTransaction,
        generate_work: bool,
    ) -> PublicKey {
        if !wallet.store.valid_password(tx) {
            return PublicKey::ZERO;
        }
        let key = wallet.store.deterministic_insert(tx);

        info!(account=%key.as_account().encode_account(), "Deterministically inserted new account");

        if generate_work {
            self.work_ensure(wallet, key.into(), key.into());
        }
        key
    }

    pub fn deterministic_insert_at(
        &self,
        wallet_id: &WalletId,
        index: u32,
        generate_work: bool,
    ) -> Result<PublicKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Wallets::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        let account = wallet.store.deterministic_insert_at(&mut txn, index);
        txn.commit();

        info!(account=%account.as_account().encode_account(), "Deterministically inserted new account");

        if generate_work {
            self.work_ensure(wallet, account.into(), account.into());
        }
        Ok(account)
    }

    pub fn deterministic_insert2(
        &self,
        wallet_id: &WalletId,
        generate_work: bool,
    ) -> Result<PublicKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Wallets::get_wallet_guard(&guard, wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        let key = self.deterministic_insert(wallet, &mut txn, generate_work);
        txn.commit();
        Ok(key)
    }

    pub fn insert_adhoc(
        &self,
        wallet: &Arc<Wallet>,
        key: &RawKey,
        generate_work: bool,
    ) -> PublicKey {
        let mut tx = self.env.begin_write();
        if !wallet.store.valid_password(&tx) {
            return PublicKey::ZERO;
        }
        let key = wallet.store.insert_adhoc(&mut tx, key);
        if generate_work {
            self.work_ensure(
                wallet,
                key.into(),
                self.ledger.any().latest_root(&key.into()),
            );
        }
        tx.commit();

        key
    }

    pub fn insert_adhoc2(
        &self,
        wallet_id: &WalletId,
        key: &RawKey,
        generate_work: bool,
    ) -> Result<PublicKey, WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Wallets::get_wallet_guard(&guard, wallet_id)?;
        let txn = self.env.begin_read();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }
        txn.commit();
        Ok(self.insert_adhoc(wallet, key, generate_work))
    }

    pub fn change_seed(
        &self,
        wallet_id: WalletId,
        prv_key: &RawKey,
        mut count: u32,
    ) -> Result<(u32, Account), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Wallets::get_wallet_guard(&guard, &wallet_id)?;
        let mut txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return Err(WalletsError::WalletLocked);
        }

        info!("Changing wallet seed");
        wallet.store.set_seed(&mut txn, prv_key);
        let mut first_account = self.deterministic_insert(wallet, &mut txn, true);
        if count == 0 {
            count = wallet.deterministic_check(&txn, 0, &self.ledger);
            info!("Auto-detected {} accounts to generate", count);
        }
        for _ in 0..count {
            // Disable work generation to prevent weak CPU nodes stuck
            first_account = self.deterministic_insert(wallet, &mut txn, false);
        }
        info!("Completed changing wallet seed and generating accounts");

        let restored_count = wallet.store.deterministic_index_get(&txn);
        txn.commit();
        Ok((restored_count, first_account.into()))
    }

    pub fn send(
        &self,
        wallet_id: WalletId,
        source: Account,
        destination: Account,
        amount: Amount,
        work: WorkNonce,
        generate_work: bool,
        id: Option<String>,
    ) -> BlockPromise {
        let guard = self.wallets.lock().unwrap();
        let wallet = match Wallets::get_wallet_guard(&guard, &wallet_id) {
            Ok(w) => w,
            Err(e) => return BlockPromise::new_failed(e),
        };
        let txn = self.env.begin_write();
        if !wallet.store.valid_password(&txn) {
            return BlockPromise::new_failed(WalletsError::WalletLocked);
        }
        if wallet.store.find(&txn, &source.into()).is_none() {
            return BlockPromise::new_failed(WalletsError::AccountNotFound);
        }
        txn.commit();

        let promise = BlockPromise::new();

        let result = match &id {
            Some(id) => {
                let mut txn = self.env.begin_write();
                let result = self.prepare_send_with_id(
                    &mut txn,
                    id,
                    wallet,
                    source,
                    destination,
                    amount,
                    work,
                );
                txn.commit();
                result
            }
            None => {
                let txn = self.env.begin_read();
                self.prepare_send(&txn, wallet, source, destination, amount, work)
            }
        };

        match result {
            Ok(PreparedSend::Cached(block)) => {
                promise.set_result(Ok(block));
            }
            Ok(PreparedSend::New(block, details)) => {
                self.action_complete(
                    wallet.clone(),
                    block,
                    source,
                    generate_work,
                    &details,
                    promise.clone(),
                );
            }
            Err(_) => {
                promise.set_result(Err(WalletsError::Generic));
            }
        };

        promise
    }

    pub fn change(
        &self,
        wallet_id: &WalletId,
        source: Account,
        representative: PublicKey,
        mut work: WorkNonce,
        generate_work: bool,
    ) -> BlockPromise {
        let guard = self.wallets.lock().unwrap();
        let wallet = match Wallets::get_wallet_guard(&guard, wallet_id) {
            Ok(w) => w,
            Err(e) => {
                return BlockPromise::new_failed(e);
            }
        };
        let epoch: Epoch;
        let block: Block;
        {
            let wallet_tx = self.env.begin_read();
            let any = self.ledger.any();
            if !wallet.store.valid_password(&wallet_tx) {
                warn!(
                    "Changing representative for account {} failed, wallet locked",
                    source.encode_account()
                );
                return BlockPromise::new_failed(WalletsError::WalletLocked);
            }

            let existing = wallet.store.find(&wallet_tx, &source.into());
            if existing.is_some() && any.account_head(&source).is_some() {
                info!(
                    "Changing representative for account {} to {}",
                    source.encode_account(),
                    representative.as_account().encode_account()
                );
                let info = any.get_account(&source).unwrap();
                let prv = wallet.store.fetch(&wallet_tx, &source.into()).unwrap();
                if work.is_zero() {
                    work = wallet
                        .store
                        .work_get(&wallet_tx, &source.into())
                        .unwrap_or_default();
                }
                let priv_key = PrivateKey::from(prv);
                block = StateBlockArgs {
                    key: &priv_key,
                    previous: info.head,
                    representative,
                    balance: info.balance,
                    link: Link::ZERO,
                    work,
                }
                .into();
                epoch = info.epoch;
            } else {
                warn!(
                    "Changing representative for account {} failed, wallet locked or account not found",
                    source.encode_account()
                );
                return BlockPromise::new_failed(WalletsError::AccountNotFound);
            }
        }

        let details = BlockDetails::new(epoch, false, false, false);
        let promise = BlockPromise::new();
        self.action_complete(
            wallet.clone(),
            block.clone(),
            source,
            generate_work,
            &details,
            promise.clone(),
        );

        promise
    }

    pub fn receive(
        &self,
        wallet_id: WalletId,
        send_hash: BlockHash,
        representative: PublicKey,
        amount: Amount,
        account: Account,
        mut work: WorkNonce,
        generate_work: bool,
    ) -> BlockPromise {
        let wallet = {
            let guard = self.wallets.lock().unwrap();
            match Wallets::get_wallet_guard(&guard, &wallet_id) {
                Ok(wallet) => wallet.clone(),
                Err(e) => return BlockPromise::new_failed(e),
            }
        };

        if amount < self.wallets_config.receive_minimum {
            warn!(
                "Not receiving block {} due to minimum receive threshold",
                send_hash
            );
            return BlockPromise::new_failed(WalletsError::Generic);
        }

        let mut block: Option<Block> = None;
        let mut epoch = Epoch::Epoch0;
        let any = self.ledger.any();
        let wallet_tx = self.env.begin_read();
        if any.block_exists(&send_hash) {
            if let Some(pending_info) = any.get_pending(&PendingKey::new(account, send_hash)) {
                if let Ok(prv) = wallet.store.fetch(&wallet_tx, &account.into()) {
                    info!(
                        "Receiving block {} from account {}, amount {}",
                        send_hash,
                        account.encode_account(),
                        pending_info.amount.number()
                    );
                    if work.is_zero() {
                        work = wallet
                            .store
                            .work_get(&wallet_tx, &account.into())
                            .unwrap_or_default();
                    }
                    let priv_key = PrivateKey::from(prv);
                    if let Some(info) = any.get_account(&account) {
                        block = Some(
                            StateBlockArgs {
                                key: &priv_key,
                                previous: info.head,
                                representative: info.representative,
                                balance: info.balance + pending_info.amount,
                                link: send_hash.into(),
                                work,
                            }
                            .into(),
                        );
                        epoch = std::cmp::max(info.epoch, pending_info.epoch);
                    } else {
                        block = Some(
                            StateBlockArgs {
                                key: &priv_key,
                                previous: BlockHash::ZERO,
                                representative,
                                balance: pending_info.amount,
                                link: send_hash.into(),
                                work,
                            }
                            .into(),
                        );
                        epoch = pending_info.epoch;
                    }
                } else {
                    warn!(
                        "Unable to receive, wallet locked, block {} to account: {}",
                        send_hash,
                        account.encode_account()
                    );
                }
            } else {
                // Ledger doesn't have this marked as available to receive anymore
                warn!("Not receiving block {}, block already received", send_hash);
            }
        } else {
            // Ledger doesn't have this block anymore.
            warn!(
                "Not receiving block {}, block no longer exists or pruned",
                send_hash
            );
        }
        wallet_tx.commit();

        let Some(block) = block else {
            return BlockPromise::new_failed(WalletsError::Generic);
        };
        let details = BlockDetails::new(epoch, false, true, false);

        let block_promise = BlockPromise::new();

        self.action_complete(
            wallet,
            block.clone(),
            account,
            generate_work,
            &details,
            block_promise.clone(),
        );

        block_promise
    }

    pub fn create(&self, wallet_id: WalletId) {
        let mut guard = self.wallets.lock().unwrap();
        debug_assert!(!guard.contains_key(&wallet_id));
        let wallet = {
            let Ok(wallet) = Wallet::new(
                wallet_id,
                &self.env,
                self.wallets_config.password_fanout,
                self.kdf.clone(),
                self.random_representative(),
                &PathBuf::from(wallet_id.to_string()),
            ) else {
                return;
            };
            Arc::new(wallet)
        };
        guard.insert(wallet_id, Arc::clone(&wallet));
        self.enter_initial_password(&wallet);
    }

    pub fn enter_password(&self, wallet_id: WalletId, password: &str) -> Result<(), WalletsError> {
        let guard = self.wallets.lock().unwrap();
        let wallet = Wallets::get_wallet_guard(&guard, &wallet_id)?;
        let tx = self.env.begin_write();
        let result = self
            .enter_password_wallet(wallet, &tx, password)
            .map_err(|_| WalletsError::InvalidPassword);
        if result.is_ok() {
            info!("Wallet unlocked");
        } else {
            warn!("Invalid password, wallet locked");
        }
        result
    }

    pub fn ensure_wallet_is_unlocked(&self, wallet_id: WalletId, password: &str) -> bool {
        let guard = self.wallets.lock().unwrap();
        let Some(existing) = guard.get(&wallet_id) else {
            return false;
        };
        let txn = self.env.begin_write();
        let mut valid = existing.store.valid_password(&txn);
        if !valid {
            valid = self.enter_password_wallet(existing, &txn, password).is_ok();
        }
        txn.commit();

        valid
    }

    pub fn set_representative(
        &self,
        wallet_id: WalletId,
        rep: PublicKey,
        update_existing_accounts: bool,
    ) -> MultiBlockPromise {
        let mut accounts = Vec::new();
        {
            let guard = self.wallets.lock().unwrap();
            let wallet = match Wallets::get_wallet_guard(&guard, &wallet_id) {
                Ok(w) => w,
                Err(err) => {
                    return MultiBlockPromise::new_failed(err);
                }
            };

            {
                let mut txn = self.env.begin_write();
                if update_existing_accounts && !wallet.store.valid_password(&txn) {
                    return MultiBlockPromise::new_failed(WalletsError::WalletLocked);
                }

                wallet.store.representative_set(&mut txn, &rep);
                txn.commit();
            }

            // Change representative for all wallet accounts
            if update_existing_accounts {
                let txn = self.env.begin_read();
                let any = self.ledger.any();
                for (account, _) in wallet.store.iter(&txn) {
                    if let Some(info) = any.get_account(&account.into())
                        && info.representative != rep
                    {
                        accounts.push(account);
                    }
                }
                txn.commit();
            }
        }

        let mut block_promises = Vec::new();
        for account in accounts {
            block_promises.push(self.change(&wallet_id, account.into(), rep, 0.into(), false));
        }

        MultiBlockPromise::new(block_promises)
    }

    pub fn search_receivable_all(&self) -> MultiBlockPromise {
        let wallet_ids = self.wallet_ids();
        let mut result = MultiBlockPromise::empty();
        for id in wallet_ids {
            result.append(self.search_receivable(&id));
        }
        result
    }

    pub fn search_receivable(&self, wallet_id: &WalletId) -> MultiBlockPromise {
        let wallet = match self.get_wallet(wallet_id) {
            Some(w) => w,
            None => return MultiBlockPromise::new_failed(WalletsError::WalletNotFound),
        };

        let txn = self.env.begin_read();
        if !wallet.store.valid_password(&txn) {
            info!(
                "Unable to search receivable blocks, wallet is locked. Blocks won't be auto-received until the wallet is unlocked"
            );
            return MultiBlockPromise::new_failed(WalletsError::WalletLocked);
        }

        debug!("Beginning receivable block search");

        let mut block_promises = Vec::new();
        for (account, wallet_value) in wallet.store.iter(&txn) {
            let any = self.ledger.any();
            // Don't search pending for watch-only accounts
            if !wallet_value.key.is_zero() {
                for (key, info) in
                    any.account_receivable_upper_bound(account.into(), BlockHash::ZERO)
                {
                    let hash = key.send_block_hash;
                    let amount = info.amount;
                    if self.wallets_config.receive_minimum <= amount {
                        info!(
                            "Found a receivable block {} for account {}",
                            hash,
                            info.source.encode_account()
                        );
                        if any.confirmed().block_exists(&hash) {
                            let representative = wallet.store.representative(&txn);
                            // Receive confirmed block
                            let promise = self.receive(
                                *wallet_id,
                                hash,
                                representative,
                                amount,
                                account.into(),
                                0.into(),
                                true,
                            );
                            block_promises.push(promise);
                        }
                    }
                }
            }
        }

        txn.commit();

        debug!("Receivable block search phase completed");
        MultiBlockPromise::new(block_promises)
    }

    pub fn provide_work(&self, root: &Root, work: Option<WorkNonce>) {
        let mut guard = self.waiting_for_work.lock().unwrap();
        let Some(item) = guard.remove(root) else {
            return;
        };
        drop(guard);

        match item {
            WorkItem::BlockWork(mut block, block_promise, generate_work, wallet) => {
                if let Some(work) = work {
                    block.set_work(work);
                    self.enqueue_block(block, block_promise, generate_work, wallet);
                } else {
                    block_promise.set_result(Err(WalletsError::Generic));
                }
            }
            WorkItem::Cache(wallet_id, account) => {
                if let Some(work) = work {
                    if let Some(wallet) = self.get_wallet(&wallet_id) {
                        let pub_key = PublicKey::from(account);
                        let mut txn = self.env.begin_write();
                        if wallet.live() && wallet.store.exists(&txn, &pub_key) {
                            let latest = self.ledger.any().latest_root(&account);
                            if latest == *root {
                                wallet.work_put(&mut txn, &pub_key, work);
                            } else {
                                warn!("Cached work no longer valid, discarding");
                            }
                        }
                        txn.commit();
                    }
                } else {
                    warn!("Cached work generation failed/aborted");
                }
            }
        }
    }

    pub fn block_processed(&self, hash: &BlockHash, result: Option<SavedBlock>) {
        let Some((promise, generate_work, wallet)) =
            self.waiting_for_processor.lock().unwrap().remove(hash)
        else {
            return;
        };

        if generate_work && let Some(block) = &result {
            // Pregenerate work for next block based on the block just created
            self.work_ensure(&wallet, block.account(), block.hash().into());
        }

        promise.set_result(result.ok_or(WalletsError::Generic));
    }

    fn action_complete(
        &self,
        wallet: Arc<Wallet>,
        block: Block,
        account: Account,
        generate_work: bool,
        details: &BlockDetails,
        block_promise: BlockPromise,
    ) {
        // Unschedule any work caching for this account
        self.delayed_work.lock().unwrap().remove(&account);
        let required_difficulty = self.work_thresholds.threshold(details);
        if self.work_thresholds.difficulty_block(&block) < required_difficulty {
            info!(
                "Cached or provided work for block {} account {} is invalid, regenerating...",
                block.hash(),
                account.encode_account()
            );

            let work_request = WorkRequest::new(block.root(), required_difficulty);
            self.enqueue_block_work(work_request, block, block_promise, generate_work, wallet);
        } else {
            self.enqueue_block(block, block_promise, generate_work, wallet);
        }
    }

    fn work_ensure(&self, wallet: &Arc<Wallet>, account: Account, root: Root) {
        let precache_delay = self.wallets_config.cached_work_generation_delay;
        let now = self.clock.now();

        self.delayed_work
            .lock()
            .unwrap()
            .insert(*wallet.id(), account, root, now + precache_delay);
    }
}

impl Drop for Wallets {
    fn drop(&mut self) {
        self.stop();
    }
}

impl ContainerInfoProvider for Wallets {
    fn container_info(&self) -> ContainerInfo {
        [(
            "items",
            self.wallet_count(),
            size_of::<usize>() * size_of::<WalletId>(),
        )]
        .into()
    }
}

pub struct WalletsTicker(pub Arc<Wallets>);

impl Tickable for WalletsTicker {
    fn tick(&mut self, _: &CancellationToken) {
        self.0.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_types::PendingInfo;
    use std::time::Duration;

    #[test]
    fn enqueue_work_request() {
        let account_key = PrivateKey::from_bytes(&[42; 32]);
        let (ledger, send_hash, amount) = ledger_with_pending_receive(&account_key);

        let fixture = Fixture::new(FixtureArgs {
            ledger: Some(ledger),
            ..Default::default()
        });
        let wallets = &fixture.wallets;

        let wallet_id = WalletId::from(1);
        wallets.create(wallet_id);
        wallets
            .insert_adhoc2(&wallet_id, &account_key.raw_key(), false)
            .unwrap();

        wallets.receive(
            wallet_id,
            send_hash,
            PublicKey::from(200),
            amount,
            account_key.account(),
            WorkNonce::new(0),
            false,
        );

        let request = fixture.pop_work_request();

        assert_eq!(
            request,
            WorkRequest::new(
                account_key.account().into(),
                wallets.work_thresholds.threshold(&BlockDetails::new(
                    Epoch::Epoch2,
                    false,
                    true,
                    false
                ))
            ),
        );
    }

    #[test]
    fn fail_when_no_work_queue_provided() {
        let account_key = PrivateKey::from_bytes(&[42; 32]);
        let (ledger, send_hash, amount) = ledger_with_pending_receive(&account_key);

        let fixture = Fixture::new(FixtureArgs {
            ledger: Some(ledger),
            disable_work_queue: true,
        });
        let wallets = &fixture.wallets;

        let wallet_id = WalletId::from(1);
        wallets.create(wallet_id);
        wallets
            .insert_adhoc2(&wallet_id, &account_key.raw_key(), false)
            .unwrap();

        let promise = wallets.receive(
            wallet_id,
            send_hash,
            PublicKey::from(200),
            amount,
            account_key.account(),
            WorkNonce::new(0),
            false,
        );
        promise
            .wait()
            .expect_err("Should fail, because there is no work queue");
    }

    fn ledger_with_pending_receive(
        receiver_account: impl Into<Account>,
    ) -> (Ledger, BlockHash, Amount) {
        let send = SavedBlock::new_test_instance();
        let amount = Amount::nano(1);

        let ledger = Ledger::new_null_builder()
            .block(&send)
            .pending(
                &PendingKey::new(receiver_account.into(), send.hash()),
                &PendingInfo {
                    source: send.account(),
                    amount,
                    epoch: Epoch::Epoch2,
                },
            )
            .finish();

        (ledger, send.hash(), amount)
    }

    #[derive(Default)]
    struct FixtureArgs {
        ledger: Option<Ledger>,
        disable_work_queue: bool,
    }

    struct Fixture {
        wallets: Arc<Wallets>,
        rx_work: mpsc::Receiver<WorkRequest>,
    }

    impl Fixture {
        fn new(args: FixtureArgs) -> Self {
            let network = NetworkType::NanoLiveNetwork;
            let env = Arc::new(LmdbEnvironment::new_null());
            let wallets_config = WalletsConfig::default();
            let work = WorkThresholds::default_for(network);
            let ledger = Arc::new(args.ledger.unwrap_or_else(|| Ledger::new_null()));
            let clock = Arc::new(SteadyClock::new_null());

            let wallets = Arc::new(Wallets::new(wallets_config, env, ledger, work, clock));

            let (tx_work, rx_work) = mpsc::channel();
            if !args.disable_work_queue {
                wallets.set_work_queue(tx_work);
            }

            Self { wallets, rx_work }
        }

        fn pop_work_request(&self) -> WorkRequest {
            self.rx_work
                .recv_timeout(Duration::from_secs(3))
                .expect("A work request should've been enqueued")
        }
    }
}
