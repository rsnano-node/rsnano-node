use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, Instant},
};

use rsnano_ledger::{
    AnySet, DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH, DEV_GENESIS_PUB_KEY, LedgerSet,
    test_helpers::UnsavedBlockLatticeBuilder,
};
use rsnano_node::{
    Node,
    config::{DEV_NETWORK_PARAMS, NodeConfig, NodeFlags},
    unique_path,
};
use rsnano_nullable_lmdb::{LmdbEnvironment, LmdbEnvironmentFactory};
use rsnano_store_lmdb::{EnvironmentFlags, EnvironmentOptions, LmdbWalletStore};
use rsnano_types::{
    Account, Amount, Block, BlockHash, DEV_GENESIS_KEY, Epoch, EpochBlockArgs,
    KeyDerivationFunction, PrivateKey, PublicKey, RawKey, deterministic_key,
};
use rsnano_wallet::WalletsError;
use test_helpers::{System, assert_always_eq, assert_timely_eq2, assert_timely2};

struct TestFixture {
    test_dir: PathBuf,
    env: LmdbEnvironment,
}

impl TestFixture {
    pub fn new() -> Self {
        let test_dir = unique_path().unwrap();
        let mut test_file = test_dir.clone();
        test_file.push("wallet.ldb");

        let options = EnvironmentOptions {
            max_dbs: 32,
            map_size: 1024 * 1024,
            flags: EnvironmentFlags::NO_SUB_DIR
                | EnvironmentFlags::NO_TLS
                | EnvironmentFlags::NO_META_SYNC
                | EnvironmentFlags::NO_SYNC,
            path: test_file,
        };
        let env = LmdbEnvironmentFactory::default().create(options).unwrap();

        Self { test_dir, env }
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.test_dir.clone());
    }
}

const TEST_KDF_WORK: u32 = 8;

#[test]
fn no_special_keys_accounts() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key = PrivateKey::from(42);
    assert!(!wallet.exists(&txn, &key.public_key()));
    wallet.insert_adhoc(&mut txn, &key.raw_key());
    assert!(wallet.exists(&txn, &key.public_key()));

    for i in 0..LmdbWalletStore::special_count().number().as_u64() {
        assert!(!wallet.exists(&txn, &i.into()))
    }
}

#[test]
fn no_key() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let txn = fixture.env.begin_write();
    assert!(wallet.fetch(&txn, &PublicKey::from(42)).is_err());
    assert!(wallet.valid_password(&txn));
}

#[test]
fn fetch_locked() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    assert!(wallet.valid_password(&txn));
    let key1 = PrivateKey::from(42);
    assert_eq!(
        wallet.insert_adhoc(&mut txn, &key1.raw_key()),
        key1.public_key()
    );
    let key2 = wallet.deterministic_insert(&mut txn);
    assert!(!key2.is_zero());
    wallet.set_password(RawKey::from(1));
    assert!(wallet.fetch(&txn, &key1.public_key()).is_err());
    assert!(wallet.fetch(&txn, &key2).is_err());
}

#[test]
fn retrieval() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key1 = PrivateKey::from(42);
    wallet.insert_adhoc(&mut txn, &key1.raw_key());
    let prv1 = wallet.fetch(&txn, &key1.public_key()).unwrap();
    assert_eq!(prv1, key1.raw_key());
    wallet.set_password(RawKey::from(123));
    assert!(wallet.fetch(&txn, &key1.public_key()).is_err());
    assert!(!wallet.valid_password(&txn));
}

#[test]
fn empty_iteration() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let txn = fixture.env.begin_write();
    assert!(wallet.iter(&txn).next().is_none());
}

#[test]
fn one_item_iteration() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key1 = PrivateKey::from(42);
    wallet.insert_adhoc(&mut txn, &key1.raw_key());
    for (k, v) in wallet.iter(&txn) {
        assert_eq!(k, key1.public_key());
        let password = wallet.wallet_key(&txn);
        let key = v.key.decrypt(&password, &k.initialization_vector());
        assert_eq!(key, key1.raw_key());
    }
}

#[test]
fn two_item_iteration() {
    let fixture = TestFixture::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let mut pubs = HashSet::new();
    let mut prvs = HashSet::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    {
        let wallet = LmdbWalletStore::new(
            0,
            kdf,
            &fixture.env,
            &DEV_GENESIS_PUB_KEY,
            &PathBuf::from("0"),
        )
        .unwrap();
        let mut txn = fixture.env.begin_write();
        wallet.insert_adhoc(&mut txn, &key1.raw_key());
        wallet.insert_adhoc(&mut txn, &key2.raw_key());
        for (k, v) in wallet.iter(&txn) {
            pubs.insert(k);
            let password = wallet.wallet_key(&txn);
            let key = v.key.decrypt(&password, &k.initialization_vector());
            prvs.insert(key);
        }
    }
    assert_eq!(pubs.len(), 2);
    assert_eq!(prvs.len(), 2);
    assert!(pubs.contains(&key1.public_key()));
    assert!(prvs.contains(&key1.raw_key()));
    assert!(pubs.contains(&key2.public_key()));
    assert!(prvs.contains(&key2.raw_key()));
}

#[test]
fn insufficient_spend_one() {
    let mut system = System::new();
    let node = system.make_node();
    let key1 = PrivateKey::new();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key1.account(),
            Amount::raw(500),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    let error = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key1.account(),
            Amount::MAX,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap_err();
    assert_eq!(error, WalletsError::Generic);
}

#[test]
fn spend_all_one() {
    let mut system = System::new();
    let node = system.make_node();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::MAX,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    let any = node.ledger.any();
    let info2 = any.get_account(&DEV_GENESIS_ACCOUNT).unwrap();
    assert_ne!(info2.head, *DEV_GENESIS_HASH);
    let block = any.get_block(&info2.head).unwrap();
    assert_eq!(block.previous(), *DEV_GENESIS_HASH);
    assert_eq!(block.balance(), Amount::ZERO);
}

#[test]
fn send_async() {
    let mut system = System::new();
    let node = system.make_node();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    let block = node.wallets.send(
        wallet_id,
        *DEV_GENESIS_ACCOUNT,
        key2.account(),
        Amount::MAX,
        0.into(),
        true,
        None,
    );

    assert_timely2(|| node.balance(&DEV_GENESIS_ACCOUNT).is_zero());
    assert!(block.wait().is_ok());
}

#[test]
fn spend() {
    let mut system = System::new();
    let node = system.make_node();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    // Sending from empty accounts should always be an error.
    // Accounts need to be opened with an open block, not a send block.
    assert!(
        node.wallets
            .send(
                wallet_id,
                Account::ZERO,
                key2.account(),
                Amount::ZERO,
                0.into(),
                true,
                None
            )
            .wait()
            .is_err()
    );

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::MAX,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();
    assert_eq!(node.balance(&DEV_GENESIS_ACCOUNT), Amount::ZERO);
}

#[test]
fn partial_spend() {
    let mut system = System::new();
    let node = system.make_node();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::raw(500),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_eq!(
        node.balance(&DEV_GENESIS_ACCOUNT),
        Amount::MAX - Amount::raw(500)
    );
}

#[test]
fn spend_no_previous() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    {
        node.insert_into_wallet(&DEV_GENESIS_KEY);
        for _ in 0..50 {
            let key = PrivateKey::new();
            node.wallets
                .insert_adhoc2(&wallet_id, &key.raw_key(), false)
                .unwrap();
        }
    }
    let key2 = PrivateKey::new();
    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::raw(500),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_eq!(
        node.balance(&DEV_GENESIS_ACCOUNT),
        Amount::MAX - Amount::raw(500)
    );
}

#[test]
fn find_none() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let txn = fixture.env.begin_write();
    assert!(wallet.find(&txn, &PublicKey::from(1000)).is_none());
}

#[test]
fn find_existing() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key1 = PrivateKey::new();
    assert_eq!(wallet.exists(&txn, &key1.public_key()), false);
    wallet.insert_adhoc(&mut txn, &key1.raw_key());
    assert_eq!(wallet.exists(&txn, &key1.public_key()), true);
    wallet.find(&txn, &key1.public_key()).unwrap();
}

#[test]
fn rekey() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let store = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let password = store.password();
    assert!(password.is_zero());
    let mut txn = fixture.env.begin_write();
    let key1 = PrivateKey::new();
    store.insert_adhoc(&mut txn, &key1.raw_key());
    assert_eq!(
        store.fetch(&txn, &key1.public_key()).unwrap(),
        key1.raw_key()
    );
    store.rekey(&mut txn, "1").unwrap();
    let password = store.password();
    let password1 = store.derive_key(&txn, "1");
    assert_eq!(password1, password);
    let prv2 = store.fetch(&txn, &key1.public_key()).unwrap();
    assert_eq!(prv2, key1.raw_key());
    store.set_password(RawKey::from(2));
    assert!(store.rekey(&mut txn, "2").is_err());
}

#[test]
fn hash_password() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let store = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let txn = fixture.env.begin_write();
    let hash1 = store.derive_key(&txn, "");
    let hash2 = store.derive_key(&txn, "");
    assert_eq!(hash1, hash2);
    let hash3 = store.derive_key(&txn, "a");
    assert_ne!(hash1, hash3);
}

#[test]
fn reopen_default_password() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    {
        let store = LmdbWalletStore::new(
            0,
            kdf.clone(),
            &fixture.env,
            &DEV_GENESIS_PUB_KEY,
            &PathBuf::from("0"),
        )
        .unwrap();
        let txn = fixture.env.begin_write();
        assert!(store.valid_password(&txn));
        txn.commit();
    }
    {
        let store = LmdbWalletStore::new(
            0,
            kdf.clone(),
            &fixture.env,
            &DEV_GENESIS_PUB_KEY,
            &PathBuf::from("0"),
        )
        .unwrap();
        let txn = fixture.env.begin_write();
        assert!(store.valid_password(&txn));
    }
    {
        let store = LmdbWalletStore::new(
            0,
            kdf.clone(),
            &fixture.env,
            &DEV_GENESIS_PUB_KEY,
            &PathBuf::from("0"),
        )
        .unwrap();
        let mut txn = fixture.env.begin_write();
        store.rekey(&mut txn, "").unwrap();
        assert!(store.valid_password(&txn));
        txn.commit();
    }
    {
        let store = LmdbWalletStore::new(
            0,
            kdf.clone(),
            &fixture.env,
            &DEV_GENESIS_PUB_KEY,
            &PathBuf::from("0"),
        )
        .unwrap();
        let txn = fixture.env.begin_write();
        assert_eq!(store.valid_password(&txn), false);
        store.attempt_password(&txn, " ");
        assert_eq!(store.valid_password(&txn), false);
        store.attempt_password(&txn, "");
        assert!(store.valid_password(&txn));
    }
}

#[test]
fn representative() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let store = LmdbWalletStore::new(
        0,
        kdf,
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    assert_eq!(store.exists(&txn, &store.representative(&txn)), false);
    assert_eq!(store.representative(&txn), *DEV_GENESIS_PUB_KEY);
    let key = PrivateKey::new();
    store.representative_set(&mut txn, &key.public_key());
    assert_eq!(store.representative(&txn), key.public_key());
    assert_eq!(store.exists(&txn, &store.representative(&txn)), false);
    store.insert_adhoc(&mut txn, &key.raw_key());
    assert_eq!(store.exists(&txn, &store.representative(&txn)), true);
}

#[test]
fn serialize_json_empty() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let store1 = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let serialized = {
        let txn = fixture.env.begin_write();
        store1.serialize_json(&txn)
    };
    let store2 =
        LmdbWalletStore::new_from_json(0, kdf, &fixture.env, &PathBuf::from("1"), &serialized)
            .unwrap();
    let txn = fixture.env.begin_write();
    let password1 = store1.wallet_key(&txn);
    let password2 = store2.wallet_key(&txn);
    assert_eq!(password1, password2);
    assert_eq!(store1.salt(&txn), store2.salt(&txn));
    assert_eq!(store1.check(&txn), store2.check(&txn));
    assert_eq!(store1.representative(&txn), store2.representative(&txn));
    assert!(store1.iter(&txn).next().is_none());
    assert!(store2.iter(&txn).next().is_none());
}

#[test]
fn serialize_json_one() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let store1 = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let key = PrivateKey::new();
    let serialized = {
        let mut txn = fixture.env.begin_write();
        store1.insert_adhoc(&mut txn, &key.raw_key());
        let json = store1.serialize_json(&txn);
        txn.commit();
        json
    };

    let store2 =
        LmdbWalletStore::new_from_json(0, kdf, &fixture.env, &PathBuf::from("1"), &serialized)
            .unwrap();
    let txn = fixture.env.begin_write();
    let password1 = store1.wallet_key(&txn);
    let password2 = store2.wallet_key(&txn);
    assert_eq!(password1, password2);
    assert_eq!(store1.salt(&txn), store2.salt(&txn));
    assert_eq!(store1.check(&txn), store2.check(&txn));
    assert_eq!(store1.representative(&txn), store2.representative(&txn));
    assert!(store2.exists(&txn, &key.public_key()));
    let prv = store2.fetch(&txn, &key.public_key()).unwrap();
    assert_eq!(prv, key.raw_key());
}

#[test]
fn serialize_json_password() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet1 = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let key = PrivateKey::new();
    let serialized = {
        let mut txn = fixture.env.begin_write();
        wallet1.rekey(&mut txn, "password").unwrap();
        wallet1.insert_adhoc(&mut txn, &key.raw_key());
        let json = wallet1.serialize_json(&txn);
        txn.commit();
        json
    };
    let wallet2 =
        LmdbWalletStore::new_from_json(0, kdf, &fixture.env, &PathBuf::from("1"), &serialized)
            .unwrap();
    let txn = fixture.env.begin_write();
    assert_eq!(wallet2.valid_password(&txn), false);
    assert!(wallet2.attempt_password(&txn, "password"));
    assert_eq!(wallet2.valid_password(&txn), true);
    let password1 = wallet1.wallet_key(&txn);
    let password2 = wallet2.wallet_key(&txn);
    assert_eq!(password1, password2);
    assert_eq!(wallet1.salt(&txn), wallet2.salt(&txn));
    assert_eq!(wallet1.check(&txn), wallet2.check(&txn));
    assert_eq!(wallet1.representative(&txn), wallet2.representative(&txn));
    assert!(wallet2.exists(&txn, &key.public_key()));
    let prv = wallet2.fetch(&txn, &key.public_key()).unwrap();
    assert_eq!(prv, key.raw_key());
}

#[test]
fn wallet_store_move() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet1 = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let key = PrivateKey::new();
    {
        let mut txn = fixture.env.begin_write();
        wallet1.insert_adhoc(&mut txn, &key.raw_key());
        txn.commit();
    }
    let wallet2 = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("1"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key2 = PrivateKey::new();
    wallet2.insert_adhoc(&mut txn, &key2.raw_key());
    assert_eq!(wallet1.exists(&txn, &key2.public_key()), false);
    wallet1
        .move_keys(&mut txn, &wallet2, &[key2.public_key()])
        .unwrap();
    assert_eq!(wallet1.exists(&txn, &key2.public_key()), true);
    assert_eq!(wallet2.exists(&txn, &key2.public_key()), false);
}

#[test]
fn wallet_store_import() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    let key1 = PrivateKey::new();
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &key1.raw_key(), false)
        .unwrap();
    let json = node1.wallets.serialize(wallet_id1).unwrap();
    node2.wallets.import_replace(wallet_id2, &json, "").unwrap();
    assert!(node2.wallets.exists(&key1.public_key()));
}

#[test]
fn wallet_store_fail_import_bad_password() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    let key1 = PrivateKey::new();
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &key1.raw_key(), false)
        .unwrap();
    let json = node1.wallets.serialize(wallet_id1).unwrap();
    node2
        .wallets
        .import_replace(wallet_id2, &json, "1")
        .unwrap_err();
}

#[test]
fn wallet_store_fail_import_corrupt() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .import_replace(wallet_id1, "", "1")
        .unwrap_err();
}

// Test work is precached when a key is inserted
#[test]
fn work() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let start = Instant::now();
    loop {
        let work = node1.wallets.work_get(&wallet_id1, &DEV_GENESIS_PUB_KEY);
        if DEV_NETWORK_PARAMS
            .work
            .difficulty(&(*DEV_GENESIS_HASH).into(), work)
            >= DEV_NETWORK_PARAMS.work.threshold_base()
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(20) {
            panic!("timeout");
        }
    }
}

#[test]
fn work_generate() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    let account1 = node1.wallets.get_accounts(1)[0];
    let key = PrivateKey::new();
    let _block = node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key.account(),
            Amount::raw(100),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely2(|| node1.ledger.any().account_balance(&DEV_GENESIS_ACCOUNT) != Amount::MAX);

    let start = Instant::now();
    loop {
        let work1 = node1.wallets.work_get(&wallet_id, &account1.into());
        let root = node1.ledger.any().latest_root(&account1);
        if DEV_NETWORK_PARAMS.work.difficulty(&root, work1)
            >= DEV_NETWORK_PARAMS.work.threshold_base()
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            panic!("timeout");
        }
    }
}

#[test]
fn work_cache_delayed() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    let account1 = node1.wallets.get_accounts(1)[0];
    let key = PrivateKey::new();
    let _block1 = node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key.account(),
            Amount::raw(100),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    let block2 = node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key.account(),
            Amount::raw(100),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_eq!(node1.wallets.delayed_work_count(), 1);
    let threshold = node1.network_params.work.threshold_base();
    let start = Instant::now();
    loop {
        let work1 = node1.wallets.work_get(&wallet_id, &account1.into());

        if DEV_NETWORK_PARAMS
            .work
            .difficulty(&block2.hash().into(), work1)
            >= threshold
        {
            break;
        }

        if start.elapsed() > Duration::from_secs(10) {
            panic!("timeout");
        }
    }
}

#[test]
fn insert_locked() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    {
        node1.wallets.rekey(&wallet_id, "1").unwrap();
        assert_eq!(
            node1.wallets.enter_password(wallet_id, "").unwrap_err(),
            WalletsError::InvalidPassword
        );
    }
    let err = node1
        .wallets
        .insert_adhoc2(&wallet_id, &RawKey::from(42), true)
        .unwrap_err();
    assert_eq!(err, WalletsError::WalletLocked);
}

#[test]
fn deterministic_keys() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();
    let mut txn = fixture.env.begin_write();
    let key1 = wallet.deterministic_key(&txn, 0);
    let key2 = wallet.deterministic_key(&txn, 0);
    assert_eq!(key1, key2);
    let key3 = wallet.deterministic_key(&txn, 1);
    assert_ne!(key1, key3);
    assert_eq!(wallet.deterministic_index_get(&txn), 0);
    wallet.deterministic_index_set(&mut txn, 1);
    assert_eq!(wallet.deterministic_index_get(&txn), 1);
    let key4 = wallet.deterministic_insert(&mut txn);
    let key5 = wallet.fetch(&txn, &key4).unwrap();
    assert_eq!(key5, key3);
    assert_eq!(wallet.deterministic_index_get(&txn), 2);
    wallet.deterministic_index_set(&mut txn, 1);
    assert_eq!(wallet.deterministic_index_get(&txn), 1);
    wallet.erase(&mut txn, &key4);
    assert_eq!(wallet.exists(&txn, &key4), false);
    let key8 = wallet.deterministic_insert(&mut txn);
    assert_eq!(key8, key4);
    let key6 = wallet.deterministic_insert(&mut txn);
    let key7 = wallet.fetch(&txn, &key6).unwrap();
    assert_ne!(key7, key5);
    assert_eq!(wallet.deterministic_index_get(&txn), 3);
    let key9 = PrivateKey::new();
    wallet.insert_adhoc(&mut txn, &key9.raw_key());
    assert!(wallet.exists(&txn, &key9.public_key()));
    wallet.deterministic_clear(&mut txn);
    assert_eq!(wallet.deterministic_index_get(&txn), 0);
    assert_eq!(wallet.exists(&txn, &key4), false);
    assert_eq!(wallet.exists(&txn, &key6), false);
    assert_eq!(wallet.exists(&txn, &key8), false);
    assert_eq!(wallet.exists(&txn, &key9.public_key()), true);
}

#[test]
fn reseed() {
    let fixture = TestFixture::new();
    let kdf = KeyDerivationFunction::new(TEST_KDF_WORK);
    let wallet = LmdbWalletStore::new(
        0,
        kdf.clone(),
        &fixture.env,
        &DEV_GENESIS_PUB_KEY,
        &PathBuf::from("0"),
    )
    .unwrap();

    let mut txn = fixture.env.begin_write();
    let seed1 = RawKey::from(1);
    let seed2 = RawKey::from(2);
    wallet.set_seed(&mut txn, &seed1);
    let seed3 = wallet.seed(&txn);
    assert_eq!(seed3, seed1);
    let key1 = wallet.deterministic_insert(&mut txn);
    wallet.set_seed(&mut txn, &seed2);
    assert_eq!(wallet.deterministic_index_get(&txn), 0);
    let seed4 = wallet.seed(&txn);
    assert_eq!(seed4, seed2);
    let key2 = wallet.deterministic_insert(&mut txn);
    assert_ne!(key2, key1);
    wallet.set_seed(&mut txn, &seed1);
    let seed5 = wallet.seed(&txn);
    assert_eq!(seed5, seed1);
    let key3 = wallet.deterministic_insert(&mut txn);
    assert_eq!(key1, key3);
}

#[test]
fn insert_deterministic_locked() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    {
        node1.wallets.rekey(&wallet_id, "1").unwrap();
        assert_eq!(
            node1.wallets.enter_password(wallet_id, "").unwrap_err(),
            WalletsError::InvalidPassword
        );
    }
    let err = node1
        .wallets
        .deterministic_insert2(&wallet_id, true)
        .unwrap_err();
    assert_eq!(err, WalletsError::WalletLocked);
}

#[test]
fn no_work() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
        .unwrap();
    let key2 = PrivateKey::new();
    let block = node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::MAX,
            0.into(),
            false,
            None,
        )
        .wait()
        .unwrap();

    assert_ne!(block.work(), 0.into());
    assert!(
        DEV_NETWORK_PARAMS.work.difficulty_block(&block)
            >= DEV_NETWORK_PARAMS.work.threshold(block.details())
    );
    let cached_work = node1.wallets.work_get(&wallet_id, &DEV_GENESIS_PUB_KEY);
    assert_eq!(cached_work, 0.into());
}

#[test]
fn send_race() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    let key2 = PrivateKey::new();
    for i in 1..60 {
        node1
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                key2.account(),
                Amount::nano(1000),
                0.into(),
                true,
                None,
            )
            .wait()
            .unwrap();
        assert_eq!(
            node1.balance(&DEV_GENESIS_ACCOUNT),
            Amount::MAX - Amount::nano(1000) * i
        )
    }
}

#[test]
fn password_race() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    std::thread::scope(|s| {
        s.spawn(|| {
            for i in 0..100 {
                node1.wallets.rekey(&wallet_id, i.to_string()).unwrap();
            }
        });
        s.spawn(|| {
            // Password should always be valid, the rekey operation should be atomic.
            assert!(node1.wallets.valid_password(&wallet_id).is_ok());
        });
    });
}

#[test]
fn password_race_corrupted_seed() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1.wallets.rekey(&wallet_id, "4567").unwrap();
    let seed = node1.wallets.get_seed(wallet_id).unwrap();
    assert!(node1.wallets.attempt_password(&wallet_id, "4567").is_ok());
    std::thread::scope(|s| {
        s.spawn(|| {
            for _ in 0..10 {
                let _ = node1.wallets.rekey(&wallet_id, "0000");
            }
        });
        s.spawn(|| {
            for _ in 0..10 {
                let _ = node1.wallets.rekey(&wallet_id, "1234");
            }
        });
        s.spawn(|| {
            for _ in 0..10 {
                let _ = node1.wallets.attempt_password(&wallet_id, "1234");
            }
        });
    });

    if node1.wallets.attempt_password(&wallet_id, "1234").is_ok() {
        assert_eq!(node1.wallets.get_seed(wallet_id).unwrap(), seed);
    } else if node1.wallets.attempt_password(&wallet_id, "0000").is_ok() {
        assert_eq!(node1.wallets.get_seed(wallet_id).unwrap(), seed);
    } else if node1.wallets.attempt_password(&wallet_id, "4567").is_ok() {
        assert_eq!(node1.wallets.get_seed(wallet_id).unwrap(), seed);
    } else {
        unreachable!()
    }
}

#[test]
fn change_seed() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    let wallet = node1.wallets.get_wallet(&wallet_id).unwrap();
    node1.wallets.enter_initial_password(&wallet);
    let seed1 = RawKey::from(1);
    let index = 4;
    let prv = deterministic_key(&seed1, index);
    let pub_key = PublicKey::from(prv);
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
        .unwrap();

    let block = node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            pub_key.into(),
            Amount::raw(100),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely2(|| node1.block_exists(&block.hash()));
    node1.wallets.change_seed(wallet_id, &seed1, 0).unwrap();
    assert_eq!(node1.wallets.get_seed(wallet_id).unwrap(), seed1);
    assert!(node1.wallets.exists(&pub_key));
}

#[test]
fn epoch_2_validation() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];

    // Upgrade the genesis account to epoch 2
    upgrade_genesis_epoch(&node, Epoch::Epoch1);
    upgrade_genesis_epoch(&node, Epoch::Epoch2);

    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
        .unwrap();

    // Test send and receive blocks
    // An epoch 2 receive block should be generated with lower difficulty with high probability
    let mut tries = 0;
    let max_tries = 20;
    let amount = node.config.receive_minimum;
    while tries < max_tries {
        tries += 1;
        let send = node
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                *DEV_GENESIS_ACCOUNT,
                amount,
                1.into(),
                true,
                None,
            )
            .wait()
            .unwrap();

        assert_eq!(send.epoch(), Epoch::Epoch2);
        assert_eq!(send.source_epoch(), Epoch::Epoch0); // Not used for send state blocks

        let receive = node
            .wallets
            .receive(
                wallet_id,
                send.hash(),
                *DEV_GENESIS_PUB_KEY,
                amount,
                *DEV_GENESIS_ACCOUNT,
                1.into(),
                true,
            )
            .wait()
            .unwrap();

        if DEV_NETWORK_PARAMS.work.difficulty_block(&receive) < DEV_NETWORK_PARAMS.work.base {
            assert!(
                DEV_NETWORK_PARAMS.work.difficulty_block(&receive)
                    >= DEV_NETWORK_PARAMS.work.epoch_2_receive
            );
            assert_eq!(receive.epoch(), Epoch::Epoch2);
            assert_eq!(receive.source_epoch(), Epoch::Epoch2);
            break;
        }
    }
    assert!(tries < max_tries);

    // Test a change block
    node.wallets
        .change(
            &wallet_id,
            *DEV_GENESIS_ACCOUNT,
            *DEV_GENESIS_PUB_KEY,
            1.into(),
            true,
        )
        .wait()
        .unwrap();
}

/// Receiving from an upgraded account uses the lower threshold and upgrades the receiving account
#[test]
fn epoch_2_receive_propagation() {
    let mut tries = 0;
    let max_tries = 20;
    while tries < max_tries {
        tries += 1;
        let mut system = System::new();
        let node = system
            .build_node()
            .flags(NodeFlags {
                disable_request_loop: true,
                ..Default::default()
            })
            .finish();
        let wallet_id = node.wallets.wallet_ids()[0];

        // Upgrade the genesis account to epoch 1
        upgrade_genesis_epoch(&node, Epoch::Epoch1);

        let key = PrivateKey::new();

        // Send and open the account
        node.wallets
            .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
            .unwrap();
        node.wallets
            .insert_adhoc2(&wallet_id, &key.raw_key(), false)
            .unwrap();
        let amount = node.config.receive_minimum;
        let send1 = node
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                key.account(),
                amount,
                1.into(),
                true,
                None,
            )
            .wait()
            .unwrap();

        node.wallets
            .receive(
                wallet_id,
                send1.hash(),
                *DEV_GENESIS_PUB_KEY,
                amount,
                key.account(),
                1.into(),
                true,
            )
            .wait()
            .unwrap();

        // Upgrade the genesis account to epoch 2
        upgrade_genesis_epoch(&node, Epoch::Epoch2);

        // Send a block
        let send2 = node
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                key.account(),
                amount,
                1.into(),
                true,
                None,
            )
            .wait()
            .unwrap();

        let receive2 = node
            .wallets
            .receive(
                wallet_id,
                send2.hash(),
                *DEV_GENESIS_PUB_KEY,
                amount,
                key.account(),
                1.into(),
                true,
            )
            .wait()
            .unwrap();
        if DEV_NETWORK_PARAMS.work.difficulty_block(&receive2) < DEV_NETWORK_PARAMS.work.base {
            assert!(
                DEV_NETWORK_PARAMS.work.difficulty_block(&receive2)
                    >= DEV_NETWORK_PARAMS.work.epoch_2_receive
            );
            assert_eq!(
                node.ledger
                    .any()
                    .get_block(&receive2.hash())
                    .unwrap()
                    .epoch(),
                Epoch::Epoch2
            );
            assert_eq!(receive2.source_epoch(), Epoch::Epoch2);
            break;
        }
    }
    assert!(tries < max_tries);
}

/// Opening an upgraded account uses the lower threshold
#[test]
fn epoch_2_receive_unopened() {
    // Ensure the lower receive work is used when receiving
    let mut tries = 0;
    let max_tries = 20;
    while tries < max_tries {
        tries += 1;
        let mut system = System::new();
        let node = system
            .build_node()
            .flags(NodeFlags {
                disable_request_loop: true,
                ..Default::default()
            })
            .finish();
        let wallet_id = node.wallets.wallet_ids()[0];

        // Upgrade the genesis account to epoch 1
        upgrade_genesis_epoch(&node, Epoch::Epoch1);

        let key = PrivateKey::new();

        // Send
        node.wallets
            .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
            .unwrap();
        let amount = node.config.receive_minimum;

        let send1 = node
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                key.account(),
                amount,
                1.into(),
                true,
                None,
            )
            .wait()
            .unwrap();

        // Upgrade unopened account to epoch_2
        let epoch2_unopened: Block = EpochBlockArgs {
            epoch_signer: &DEV_GENESIS_KEY,
            account: key.account(),
            previous: BlockHash::ZERO,
            representative: PublicKey::ZERO,
            balance: Amount::ZERO,
            link: *node
                .network_params
                .ledger
                .epochs
                .link(Epoch::Epoch2)
                .unwrap(),
            work: node.work_generate_dev(&key),
        }
        .into();
        node.process(epoch2_unopened);

        node.wallets
            .insert_adhoc2(&wallet_id, &key.raw_key(), false)
            .unwrap();

        let receive1 = node
            .wallets
            .receive(
                wallet_id,
                send1.hash(),
                key.public_key(),
                amount,
                key.account(),
                1.into(),
                true,
            )
            .wait()
            .unwrap();

        if DEV_NETWORK_PARAMS.work.difficulty_block(&receive1) < DEV_NETWORK_PARAMS.work.base {
            assert!(
                DEV_NETWORK_PARAMS.work.difficulty_block(&receive1)
                    >= DEV_NETWORK_PARAMS.work.epoch_2_receive
            );
            assert_eq!(
                node.ledger
                    .any()
                    .get_block(&receive1.hash())
                    .unwrap()
                    .epoch(),
                Epoch::Epoch2
            );
            assert_eq!(receive1.source_epoch(), Epoch::Epoch1);
            break;
        }
    }
    assert!(tries < max_tries);
}

#[test]
fn search_receivable() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(NodeConfig {
            enable_voting: false,
            ..System::default_config_without_backlog_scan()
        })
        .flags(NodeFlags {
            disable_search_pending: true,
            ..Default::default()
        })
        .finish();

    let wallet_id = node.wallets.wallet_ids()[0];
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, node.config.receive_minimum);
    node.process(send.clone());
    node.wallets.search_receivable(&wallet_id).wait().unwrap();
    assert_always_eq(Duration::from_millis(300), || node.ledger.block_count(), 2);

    node.confirm(send.hash());
    node.wallets.search_receivable(&wallet_id).wait().unwrap();
    assert_timely_eq2(|| node.balance(&DEV_GENESIS_ACCOUNT), Amount::MAX);
    let receive_hash = node
        .ledger
        .any()
        .account_head(&DEV_GENESIS_ACCOUNT)
        .unwrap();
    let receive = node.block(&receive_hash).unwrap();
    assert_eq!(receive.height(), 3);
    assert_eq!(receive.source().unwrap(), send.hash());
}

fn upgrade_genesis_epoch(node: &Node, epoch: Epoch) {
    let any = node.ledger.any();
    let latest = any.account_head(&DEV_GENESIS_ACCOUNT).unwrap();
    let balance = any.account_balance(&DEV_GENESIS_ACCOUNT);

    let epoch: Block = EpochBlockArgs {
        epoch_signer: &DEV_GENESIS_KEY,
        account: *DEV_GENESIS_ACCOUNT,
        previous: latest,
        representative: *DEV_GENESIS_PUB_KEY,
        balance,
        link: node.ledger.epoch_link(epoch).unwrap(),
        work: node.work_generate_dev(latest),
    }
    .into();
    node.ledger.process_one(&epoch).unwrap();
}
