use rsnano_core::{Amount, BlockHash, WalletId, DEV_GENESIS_KEY};
use rsnano_ledger::{DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH};
use rsnano_node::wallets::WalletsExt;
use rsnano_rpc_messages::ReceiveArgs;
use std::time::Duration;
use test_helpers::{assert_timely_msg, setup_rpc_client_and_server, System};

#[test]
fn receive() {
    let mut system = System::new();
    let node = system.make_node();

    let wallet = WalletId::zero();
    node.wallets.create(wallet);
    node.wallets
        .insert_adhoc2(&wallet, &DEV_GENESIS_KEY.private_key(), false)
        .unwrap();

    let key1 = rsnano_core::KeyPair::new();
    node.wallets
        .insert_adhoc2(&wallet, &key1.private_key(), false)
        .unwrap();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let send1 = node
        .wallets
        .send_action2(
            &wallet,
            *DEV_GENESIS_ACCOUNT,
            key1.public_key().into(),
            node.config.receive_minimum,
            node.work_generate_dev((*DEV_GENESIS_HASH).into()),
            true,
            None,
        )
        .unwrap();

    assert_timely_msg(
        Duration::from_secs(5),
        || {
            node.ledger
                .any()
                .account_balance(&node.ledger.read_txn(), &(*DEV_GENESIS_ACCOUNT))
                != Some(Amount::MAX)
        },
        "Genesis account balance not updated",
    );

    assert_timely_msg(
        Duration::from_secs(10),
        || {
            !node
                .ledger
                .any()
                .get_account(&node.ledger.read_txn(), &key1.public_key().into())
                .is_some()
        },
        "Destination account should not exist yet",
    );

    let send2 = node
        .wallets
        .send_action2(
            &wallet,
            *DEV_GENESIS_ACCOUNT,
            key1.public_key().into(),
            node.config.receive_minimum - Amount::raw(1),
            node.work_generate_dev(send1.hash().into()),
            true,
            None,
        )
        .unwrap();

    let args = ReceiveArgs::builder(wallet, key1.public_key().into(), send2.hash()).build();

    let block_hash = node
        .runtime
        .block_on(async { rpc_client.receive(args).await.unwrap() })
        .block;

    let tx = node.ledger.read_txn();
    assert_timely_msg(
        Duration::from_secs(5),
        || node.ledger.get_block(&tx, &block_hash).is_some(),
        "Receive block not found in ledger",
    );

    assert_eq!(
        node.ledger
            .any()
            .account_balance(&tx, &key1.public_key().into())
            .unwrap(),
        node.config.receive_minimum - Amount::raw(1)
    );

    let args = ReceiveArgs::builder(wallet, key1.public_key().into(), send2.hash()).build();

    let error_result = node
        .runtime
        .block_on(async { rpc_client.receive(args).await });

    assert_eq!(
        error_result.err().map(|e| e.to_string()),
        Some("node returned error: \"Block is not receivable\"".to_string())
    );

    let args = ReceiveArgs::builder(wallet, key1.public_key().into(), BlockHash::zero()).build();

    let error_result = node
        .runtime
        .block_on(async { rpc_client.receive(args).await });

    assert_eq!(
        error_result.err().map(|e| e.to_string()),
        Some("node returned error: \"Block not found\"".to_string())
    );

    server.abort();
}
