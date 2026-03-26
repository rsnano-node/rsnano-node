use std::{
    fs::{File, Permissions, set_permissions},
    io::{Read, Write},
    ops::RangeBounds,
    os::unix::prelude::PermissionsExt,
    path::Path,
    sync::{Mutex, MutexGuard},
};

use anyhow::bail;

use rsnano_nullable_lmdb::{
    DatabaseFlags, Error, LmdbEnvironment, Transaction, WriteFlags, WriteTransaction,
};
use rsnano_types::{
    Account, DeserializationError, KeyDerivationFunction, PublicKey, RawKey, WorkNonce,
    deterministic_key, read_u64_ne,
};

use crate::{Fan, LmdbDatabase, LmdbRangeIterator};

pub struct Fans {
    pub password: Fan,
    pub wallet_key_mem: Fan,
}

impl Fans {
    pub fn new(fanout: usize) -> Self {
        Self {
            password: Fan::new(RawKey::ZERO, fanout),
            wallet_key_mem: Fan::new(RawKey::ZERO, fanout),
        }
    }
}

pub struct WalletValue {
    pub key: RawKey,
    pub work: WorkNonce,
}

impl WalletValue {
    pub const SERIALIZED_SIZE: usize = RawKey::SERIALIZED_SIZE + 8;

    pub fn new(key: RawKey, work: WorkNonce) -> Self {
        Self { key, work }
    }

    pub fn to_bytes(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buffer = [0; Self::SERIALIZED_SIZE];
        self.serialize(&mut buffer.as_mut()).unwrap();
        buffer
    }

    pub fn serialize<T>(&self, writer: &mut T) -> std::io::Result<()>
    where
        T: Write,
    {
        writer.write_all(self.key.as_bytes())?;
        writer.write_all(&u64::from(self.work).to_ne_bytes())
    }

    pub fn deserialize<T>(reader: &mut T) -> Result<Self, DeserializationError>
    where
        T: Read,
    {
        let key = RawKey::deserialize(reader)?;
        let work = read_u64_ne(reader)?;
        Ok(WalletValue::new(key, work.into()))
    }
}

#[derive(FromPrimitive)]
pub enum KeyType {
    NotAType,
    Unknown,
    Adhoc,
    Deterministic,
}

pub struct LmdbWalletStore {
    db_handle: Mutex<Option<LmdbDatabase>>,
    fans: Mutex<Fans>,
    kdf: KeyDerivationFunction,
}

impl LmdbWalletStore {
    pub const VERSION_CURRENT: u32 = 4;

    pub fn new(
        fanout: usize,
        kdf: KeyDerivationFunction,
        env: &LmdbEnvironment,
        representative: &PublicKey,
        wallet: &Path,
    ) -> anyhow::Result<Self> {
        let store = Self {
            db_handle: Mutex::new(None),
            fans: Mutex::new(Fans::new(fanout)),
            kdf,
        };
        store.initialize(env, wallet)?;
        let handle = store.db_handle();
        let mut txn = env.begin_write();
        if let Err(Error::NotFound) = txn.get(handle, Self::version_special().as_bytes()) {
            store.version_put(&mut txn, Self::VERSION_CURRENT);
            let salt = RawKey::random();
            store.entry_put_raw(
                &mut txn,
                &Self::salt_special(),
                &WalletValue::new(salt, 0.into()),
            );
            // Wallet key is a fixed random key that encrypts all entries
            let wallet_key = RawKey::random();
            let password = RawKey::ZERO;
            let mut guard = store.fans.lock().unwrap();
            guard.password.value_set(password);
            let zero = RawKey::ZERO;
            // Wallet key is encrypted by the user's password
            let encrypted = wallet_key.encrypt(&zero, &salt.initialization_vector_low());
            store.entry_put_raw(
                &mut txn,
                &Self::wallet_key_special(),
                &WalletValue::new(encrypted, 0.into()),
            );
            let wallet_key_enc = encrypted;
            guard.wallet_key_mem.value_set(wallet_key_enc);
            drop(guard);
            let check = zero.encrypt(&wallet_key, &salt.initialization_vector_low());
            store.entry_put_raw(
                &mut txn,
                &Self::check_special(),
                &WalletValue::new(check, 0.into()),
            );
            let rep = RawKey::from_bytes(*representative.as_bytes());
            store.entry_put_raw(
                &mut txn,
                &Self::representative_special(),
                &WalletValue::new(rep, 0.into()),
            );
            let seed = RawKey::random();
            store.set_seed(&mut txn, &seed);
            store.entry_put_raw(
                &mut txn,
                &Self::deterministic_index_special(),
                &WalletValue::new(RawKey::ZERO, 0.into()),
            );
        }
        {
            let key = store.entry_get_raw(&txn, &Self::wallet_key_special()).key;
            let mut guard = store.fans.lock().unwrap();
            guard.wallet_key_mem.value_set(key);
        }
        txn.commit();
        Ok(store)
    }

    pub fn new_from_json(
        fanout: usize,
        kdf: KeyDerivationFunction,
        env: &LmdbEnvironment,
        wallet: &Path,
        json: &str,
    ) -> anyhow::Result<Self> {
        let store = Self {
            db_handle: Mutex::new(None),
            fans: Mutex::new(Fans::new(fanout)),
            kdf,
        };
        store.initialize(env, wallet)?;
        let handle = store.db_handle();
        let mut txn = env.begin_write();
        match txn.get(handle, Self::version_special().as_bytes()) {
            Ok(_) => panic!("wallet store already initialized"),
            Err(Error::NotFound) => {}
            Err(e) => panic!("unexpected wallet store error: {:?}", e),
        }

        let json: serde_json::Value = serde_json::from_str(json)?;
        if let serde_json::Value::Object(map) = json {
            for (k, v) in map.iter() {
                if let serde_json::Value::String(v_str) = v {
                    let key =
                        PublicKey::decode_hex(k).ok_or_else(|| anyhow!("Invalid public key"))?;
                    let value =
                        RawKey::decode_hex(v_str).ok_or_else(|| anyhow!("Invalid raw key"))?;
                    store.entry_put_raw(&mut txn, &key, &WalletValue::new(value, 0.into()));
                } else {
                    bail!("expected string value");
                }
            }
        } else {
            bail!("invalid json")
        }

        store.ensure_key_exists(&txn, &Self::version_special())?;
        store.ensure_key_exists(&txn, &Self::wallet_key_special())?;
        store.ensure_key_exists(&txn, &Self::salt_special())?;
        store.ensure_key_exists(&txn, &Self::check_special())?;
        store.ensure_key_exists(&txn, &Self::representative_special())?;
        let mut guard = store.fans.lock().unwrap();
        guard.password.value_set(RawKey::ZERO);
        let key = store.entry_get_raw(&txn, &Self::wallet_key_special()).key;
        guard.wallet_key_mem.value_set(key);
        txn.commit();
        drop(guard);
        Ok(store)
    }

    pub fn password(&self) -> RawKey {
        self.fans.lock().unwrap().password.value()
    }

    fn ensure_key_exists(&self, txn: &dyn Transaction, key: &PublicKey) -> anyhow::Result<()> {
        txn.get(self.db_handle(), key.as_bytes())?;
        Ok(())
    }

    /// Wallet version number
    pub fn version_special() -> PublicKey {
        PublicKey::from(0)
    }

    /// Random number used to salt private key encryption
    pub fn salt_special() -> PublicKey {
        PublicKey::from(1)
    }

    /// Key used to encrypt wallet keys, encrypted itself by the user password
    pub fn wallet_key_special() -> PublicKey {
        PublicKey::from(2)
    }

    /// Check value used to see if password is valid
    pub fn check_special() -> PublicKey {
        PublicKey::from(3)
    }

    /// Representative account to be used if we open a new account
    pub fn representative_special() -> PublicKey {
        PublicKey::from(4)
    }

    /// Wallet seed for deterministic key generation
    pub fn seed_special() -> PublicKey {
        PublicKey::from(5)
    }

    /// Current key index for deterministic keys
    pub fn deterministic_index_special() -> PublicKey {
        PublicKey::from(6)
    }

    pub fn special_count() -> PublicKey {
        PublicKey::from(7)
    }

    pub fn initialize(&self, env: &LmdbEnvironment, path: &Path) -> anyhow::Result<()> {
        let path_str = path
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("invalid path"))?;

        let db = env.create_db(Some(path_str), DatabaseFlags::empty())?;
        *self.db_handle.lock().unwrap() = Some(db);
        Ok(())
    }

    fn db_handle(&self) -> LmdbDatabase {
        self.db_handle.lock().unwrap().unwrap()
    }

    pub fn entry_get_raw(&self, txn: &dyn Transaction, pub_key: &PublicKey) -> WalletValue {
        match txn.get(self.db_handle(), pub_key.as_bytes()) {
            Ok(mut bytes) => {
                WalletValue::deserialize(&mut bytes).expect("Should be a valid wallet value")
            }
            _ => WalletValue::new(RawKey::ZERO, 0.into()),
        }
    }

    pub fn entry_put_raw(
        &self,
        txn: &mut WriteTransaction,
        pub_key: &PublicKey,
        entry: &WalletValue,
    ) {
        txn.put(
            self.db_handle(),
            pub_key.as_bytes(),
            &entry.to_bytes(),
            WriteFlags::empty(),
        )
        .unwrap();
    }

    pub fn check(&self, txn: &dyn Transaction) -> RawKey {
        self.entry_get_raw(txn, &Self::check_special()).key
    }

    pub fn salt(&self, txn: &dyn Transaction) -> RawKey {
        self.entry_get_raw(txn, &Self::salt_special()).key
    }

    pub fn wallet_key(&self, txn: &dyn Transaction) -> RawKey {
        let guard = self.fans.lock().unwrap();
        self.wallet_key_locked(&guard, txn)
    }

    fn wallet_key_locked(&self, guard: &MutexGuard<Fans>, txn: &dyn Transaction) -> RawKey {
        let wallet = guard.wallet_key_mem.value();
        let password = guard.password.value();
        let iv = self.salt(txn).initialization_vector_low();
        wallet.decrypt(&password, &iv)
    }

    pub fn seed(&self, txn: &dyn Transaction) -> RawKey {
        let value = self.entry_get_raw(txn, &Self::seed_special());
        let password = self.wallet_key(txn);
        let iv = self.salt(txn).initialization_vector_high();
        value.key.decrypt(&password, &iv)
    }

    pub fn set_seed(&self, txn: &mut WriteTransaction, prv: &RawKey) {
        let password_l = self.wallet_key(txn);
        let iv = self.salt(txn).initialization_vector_high();
        let ciphertext = prv.encrypt(&password_l, &iv);
        self.entry_put_raw(
            txn,
            &Self::seed_special(),
            &WalletValue::new(ciphertext, 0.into()),
        );
        self.deterministic_clear(txn);
    }

    pub fn deterministic_key(&self, txn: &dyn Transaction, index: u32) -> RawKey {
        debug_assert!(self.valid_password(txn));
        let seed = self.seed(txn);
        deterministic_key(&seed, index)
    }

    pub fn deterministic_index_get(&self, txn: &dyn Transaction) -> u32 {
        let value = self.entry_get_raw(txn, &Self::deterministic_index_special());
        value.key.number().low_u32()
    }

    pub fn deterministic_index_set(&self, txn: &mut WriteTransaction, index: u32) {
        let index = RawKey::from(index as u64);
        let value = WalletValue::new(index, 0.into());
        self.entry_put_raw(txn, &Self::deterministic_index_special(), &value);
    }

    pub fn set_password(&self, password: RawKey) {
        self.fans.lock().unwrap().password.value_set(password);
    }

    pub fn valid_password(&self, txn: &dyn Transaction) -> bool {
        let wallet_key = self.wallet_key(txn);
        self.check_wallet_key(txn, &wallet_key)
    }

    pub fn valid_password_locked(&self, guard: &MutexGuard<Fans>, txn: &dyn Transaction) -> bool {
        let wallet_key = self.wallet_key_locked(guard, txn);
        self.check_wallet_key(txn, &wallet_key)
    }

    fn check_wallet_key(&self, txn: &dyn Transaction, wallet_key: &RawKey) -> bool {
        let zero = RawKey::ZERO;
        let iv = self.salt(txn).initialization_vector_low();
        let check = zero.encrypt(wallet_key, &iv);
        self.check(txn) == check
    }

    pub fn derive_key(&self, txn: &dyn Transaction, password: &str) -> RawKey {
        let salt = self.salt(txn);
        self.kdf.hash_password(password, salt.as_bytes())
    }

    pub fn rekey(&self, txn: &mut WriteTransaction, password: &str) -> anyhow::Result<()> {
        let mut guard = self.fans.lock().unwrap();
        if self.valid_password_locked(&guard, txn) {
            let password_new = self.derive_key(txn, password);
            let wallet_key = self.wallet_key_locked(&guard, txn);
            guard.password.value_set(password_new);
            let iv = self.salt(txn).initialization_vector_low();
            let encrypted = wallet_key.encrypt(&password_new, &iv);
            guard.wallet_key_mem.value_set(encrypted);
            self.entry_put_raw(
                txn,
                &Self::wallet_key_special(),
                &WalletValue::new(encrypted, 0.into()),
            );
            Ok(())
        } else {
            Err(anyhow!("invalid password"))
        }
    }

    pub fn iter<'tx>(
        &self,
        tx: &'tx dyn Transaction,
    ) -> impl Iterator<Item = (PublicKey, WalletValue)> + use<'tx> {
        self.iter_range(tx, Self::special_count()..)
    }

    pub fn iter_range<'txn, R>(
        &self,
        tx: &'txn dyn Transaction,
        range: R,
    ) -> impl Iterator<Item = (PublicKey, WalletValue)> + use<'txn, R>
    where
        R: RangeBounds<PublicKey> + 'static,
    {
        let cursor = tx.open_ro_cursor(self.db_handle()).unwrap();
        LmdbRangeIterator::new(
            cursor,
            range.start_bound().map(|b| b.as_bytes().to_vec()),
            range.end_bound().map(|b| b.as_bytes().to_vec()),
            read_wallet_record,
        )
    }

    pub fn find(&self, txn: &dyn Transaction, pub_key: &PublicKey) -> Option<WalletValue> {
        let mut result = self.iter_range(txn, *pub_key..);
        if let Some((key, value)) = result.next()
            && key == *pub_key
        {
            return Some(value);
        }

        None
    }

    pub fn erase(&self, txn: &mut WriteTransaction, pub_key: &PublicKey) {
        txn.delete(self.db_handle(), pub_key.as_bytes(), None)
            .unwrap();
    }

    pub fn get_key_type(&self, txn: &dyn Transaction, pub_key: &PublicKey) -> KeyType {
        let value = self.entry_get_raw(txn, pub_key);
        Self::key_type(&value)
    }

    pub fn key_type(value: &WalletValue) -> KeyType {
        let number = value.key.number();
        if number > u64::MAX.into() {
            KeyType::Adhoc
        } else if (number >> 32).low_u32() == 1 {
            KeyType::Deterministic
        } else {
            KeyType::Unknown
        }
    }

    pub fn deterministic_clear(&self, txn: &mut WriteTransaction) {
        {
            let mut it = self.iter_range(txn, PublicKey::ZERO..);
            while let Some((account, value)) = it.next() {
                if let KeyType::Deterministic = Self::key_type(&value) {
                    drop(it);
                    self.erase(txn, &account);
                    it = self.iter_range(txn, account..);
                }
            }
        }

        self.deterministic_index_set(txn, 0);
    }

    pub fn valid_public_key(&self, key: &PublicKey) -> bool {
        key.number() >= Self::special_count().number()
    }

    pub fn exists(&self, txn: &dyn Transaction, key: &PublicKey) -> bool {
        self.valid_public_key(key) && self.find(txn, key).is_some()
    }

    pub fn deterministic_insert(&self, txn: &mut WriteTransaction) -> PublicKey {
        let mut index = self.deterministic_index_get(txn);
        let mut prv = self.deterministic_key(txn, index);
        let mut result = PublicKey::from(prv);
        while self.exists(txn, &result) {
            index += 1;
            prv = self.deterministic_key(txn, index);
            result = PublicKey::from(prv);
        }

        let mut marker = 1u64;
        marker <<= 32;
        marker |= index as u64;
        self.entry_put_raw(txn, &result, &WalletValue::new(marker.into(), 0.into()));
        index += 1;
        self.deterministic_index_set(txn, index);
        result
    }

    pub fn deterministic_insert_at(&self, txn: &mut WriteTransaction, index: u32) -> PublicKey {
        let prv = self.deterministic_key(txn, index);
        let result = PublicKey::from(prv);
        let mut marker = 1u64;
        marker <<= 32;
        marker |= index as u64;
        self.entry_put_raw(txn, &result, &WalletValue::new(marker.into(), 0.into()));
        result
    }

    pub fn version(&self, txn: &dyn Transaction) -> u32 {
        let value = self.entry_get_raw(txn, &Self::version_special());
        value.key.as_bytes()[31] as u32
    }

    pub fn attempt_password(&self, txn: &dyn Transaction, password: &str) -> bool {
        let is_valid = {
            let mut guard = self.fans.lock().unwrap();
            let password_key = self.derive_key(txn, password);
            guard.password.value_set(password_key);
            self.valid_password_locked(&guard, txn)
        };

        if is_valid && self.version(txn) != 4 {
            panic!("invalid wallet store version!");
        }

        is_valid
    }

    pub fn lock(&self) {
        self.fans.lock().unwrap().password.value_set(RawKey::ZERO);
    }

    pub fn accounts(&self, txn: &dyn Transaction) -> Vec<Account> {
        self.iter(txn).map(|(key, _)| key.into()).collect()
    }

    pub fn representative(&self, txn: &dyn Transaction) -> PublicKey {
        let value = self.entry_get_raw(txn, &Self::representative_special());
        PublicKey::from_bytes(*value.key.as_bytes())
    }

    pub fn representative_set(&self, txn: &mut WriteTransaction, representative: &PublicKey) {
        let rep = RawKey::from_bytes(*representative.as_bytes());
        self.entry_put_raw(
            txn,
            &Self::representative_special(),
            &WalletValue::new(rep, 0.into()),
        );
    }

    pub fn insert_adhoc(&self, txn: &mut WriteTransaction, prv: &RawKey) -> PublicKey {
        debug_assert!(self.valid_password(txn));
        let pub_key = PublicKey::from(*prv);
        let password = self.wallet_key(txn);
        let ciphertext = prv.encrypt(&password, &pub_key.initialization_vector());
        self.entry_put_raw(txn, &pub_key, &WalletValue::new(ciphertext, 0.into()));
        pub_key
    }

    pub fn insert_watch(
        &self,
        txn: &mut WriteTransaction,
        pub_key: &PublicKey,
    ) -> anyhow::Result<()> {
        if !self.valid_public_key(pub_key) {
            bail!("invalid public key");
        }

        self.entry_put_raw(txn, pub_key, &WalletValue::new(RawKey::ZERO, 0.into()));
        Ok(())
    }

    pub fn fetch(&self, txn: &dyn Transaction, pub_key: &PublicKey) -> anyhow::Result<RawKey> {
        if !self.valid_password(txn) {
            bail!("invalid password");
        }

        let value = self.entry_get_raw(txn, pub_key);
        if value.key.is_zero() {
            bail!("pub key not found");
        }

        let prv = match Self::key_type(&value) {
            KeyType::Deterministic => {
                let index = value.key.number().low_u32();
                self.deterministic_key(txn, index)
            }
            KeyType::Adhoc => {
                // Ad-hoc keys
                let password = self.wallet_key(txn);
                value
                    .key
                    .decrypt(&password, &pub_key.initialization_vector())
            }
            _ => bail!("invalid key type"),
        };

        let compare = PublicKey::from(prv);
        if compare != *pub_key {
            bail!("expected pub key does not match");
        }
        Ok(prv)
    }

    pub fn serialize_json(&self, tx: &dyn Transaction) -> String {
        let mut map = serde_json::Map::new();

        // include special keys...
        for (k, v) in self.iter_range(tx, PublicKey::ZERO..) {
            map.insert(
                k.encode_hex(),
                serde_json::Value::String(v.key.encode_hex()),
            );
        }

        serde_json::Value::Object(map).to_string()
    }

    pub fn write_backup(&self, txn: &dyn Transaction, path: &Path) -> anyhow::Result<()> {
        let mut file = File::create(path)?;
        set_permissions(path, Permissions::from_mode(0o600))?;
        write!(file, "{}", self.serialize_json(txn))?;
        Ok(())
    }

    pub fn move_keys(
        &self,
        txn: &mut WriteTransaction,
        other: &LmdbWalletStore,
        keys: &[PublicKey],
    ) -> anyhow::Result<()> {
        debug_assert!(self.valid_password(txn));
        debug_assert!(other.valid_password(txn));
        for k in keys {
            let prv = other.fetch(txn, k)?;
            self.insert_adhoc(txn, &prv);
            other.erase(txn, k);
        }

        Ok(())
    }

    pub fn import(
        &self,
        txn: &mut WriteTransaction,
        other: &LmdbWalletStore,
    ) -> anyhow::Result<()> {
        debug_assert!(self.valid_password(txn));
        debug_assert!(other.valid_password(txn));

        enum KeyType {
            Private((PublicKey, RawKey)),
            WatchOnly(PublicKey),
        }

        let mut keys = Vec::new();
        {
            for (k, _) in other.iter(txn) {
                let prv = other.fetch(txn, &k)?;
                if !prv.is_zero() {
                    keys.push(KeyType::Private((k, prv)));
                } else {
                    keys.push(KeyType::WatchOnly(k));
                }
            }
        }

        for k in keys {
            match k {
                KeyType::Private((pub_key, priv_key)) => {
                    self.insert_adhoc(txn, &priv_key);
                    other.erase(txn, &pub_key);
                }
                KeyType::WatchOnly(pub_key) => {
                    self.insert_watch(txn, &pub_key).unwrap();
                    other.erase(txn, &pub_key);
                }
            }
        }

        Ok(())
    }

    pub fn work_get(
        &self,
        txn: &dyn Transaction,
        pub_key: &PublicKey,
    ) -> anyhow::Result<WorkNonce> {
        let entry = self.entry_get_raw(txn, pub_key);
        if !entry.key.is_zero() {
            Ok(entry.work)
        } else {
            Err(anyhow!("not found"))
        }
    }

    pub fn version_put(&self, txn: &mut WriteTransaction, version: u32) {
        let entry = RawKey::from(version as u64);
        self.entry_put_raw(
            txn,
            &Self::version_special(),
            &WalletValue::new(entry, 0.into()),
        );
    }

    pub fn work_put(&self, txn: &mut WriteTransaction, pub_key: &PublicKey, work: WorkNonce) {
        let mut entry = self.entry_get_raw(txn, pub_key);
        debug_assert!(!entry.key.is_zero());
        entry.work = work;
        self.entry_put_raw(txn, pub_key, &entry);
    }

    pub fn destroy(&self, txn: &mut WriteTransaction) {
        unsafe {
            txn.drop_db(self.db_handle()).unwrap();
        }
        *self.db_handle.lock().unwrap() = None;
    }

    pub fn is_open(&self) -> bool {
        self.db_handle.lock().unwrap().is_some()
    }
}

fn read_wallet_record(k: &[u8], mut v: &[u8]) -> (PublicKey, WalletValue) {
    let key = PublicKey::from_slice(k).expect("Should be a valid key");
    let value = WalletValue::deserialize(&mut v).expect("Should be a valid wallet value");
    (key, value)
}
