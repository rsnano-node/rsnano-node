use rsnano_core::{Amount, WalletId, DEV_GENESIS_KEY};
use rsnano_ledger::DEV_GENESIS_ACCOUNT;
use rsnano_node::wallets::WalletsExt;
use rsnano_rpc_messages::RepresentativesOnlineArgs;
use std::time::Duration;
use test_helpers::{assert_timely_msg, setup_rpc_client_and_server, System};

#[test]
fn representatives_online() {
    let mut system = System::new();
    let node = system.make_node();
    let node2 = system.make_node(); // Create node2
    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::zero();
    node.wallets.create(wallet);
    node.wallets
        .insert_adhoc2(&wallet, &(*DEV_GENESIS_KEY).private_key(), true)
        .unwrap();

    // Set up wallet for node2
    let node2_wallet = WalletId::random();
    node2.wallets.create(node2_wallet);

    let send_amount = Amount::raw(1_000_000_000_000_000_000_000_000u128); // 1 Gxrb

    // Create a new representative on node2
    let new_rep = node2
        .wallets
        .deterministic_insert2(&node2_wallet, true)
        .unwrap();

    // Send funds to new representative
    let send = node
        .wallets
        .send_action2(
            &wallet,
            *DEV_GENESIS_ACCOUNT,
            new_rep.into(),
            send_amount,
            0,
            true,
            None,
        )
        .unwrap();
    node.process_active(send.clone());

    // Ensure both nodes process the send
    assert_timely_msg(
        Duration::from_secs(10),
        || {
            node.ledger
                .any()
                .get_block(&node.ledger.read_txn(), &send.hash())
                .is_some()
                && node2
                    .ledger
                    .any()
                    .get_block(&node2.ledger.read_txn(), &send.hash())
                    .is_some()
        },
        "send block not received by both nodes",
    );

    // Receive the funds on node2
    let receive = node2
        .wallets
        .receive_action2(
            &node2_wallet,
            send.hash(),
            new_rep.into(),
            send_amount,
            send.destination().unwrap(),
            0,
            true,
        )
        .unwrap()
        .unwrap();
    node2.process_active(receive.clone());

    // Ensure both nodes process the receive
    assert_timely_msg(
        Duration::from_secs(10),
        || {
            node.ledger
                .any()
                .get_block(&node.ledger.read_txn(), &receive.hash())
                .is_some()
                && node2
                    .ledger
                    .any()
                    .get_block(&node2.ledger.read_txn(), &receive.hash())
                    .is_some()
        },
        "receive block not processed by both nodes",
    );

    // Change representative for genesis account
    let change = node
        .wallets
        .change_action2(&wallet, *DEV_GENESIS_ACCOUNT, new_rep.into(), 0, true)
        .unwrap();
    node.process_active(change.clone());

    // Ensure both nodes process the change
    assert_timely_msg(
        Duration::from_secs(10),
        || {
            node.ledger
                .any()
                .get_block(&node.ledger.read_txn(), &change.hash())
                .is_some()
                && node2
                    .ledger
                    .any()
                    .get_block(&node2.ledger.read_txn(), &change.hash())
                    .is_some()
        },
        "change block not processed by both nodes",
    );

    // Ensure we have two online representatives
    assert_timely_msg(
        Duration::from_secs(10),
        || {
            node.online_reps.lock().unwrap().online_reps().count() == 2
                && node2.online_reps.lock().unwrap().online_reps().count() == 2
        },
        "two representatives not online on both nodes",
    );

    let args = RepresentativesOnlineArgs::builder()
        .weight()
        .accounts(vec![new_rep.into()])
        .build();

    // Test filtering by accounts using node2
    let filtered_result = node2
        .runtime
        .block_on(async { rpc_client.representatives_online(args).await.unwrap() });

    assert_eq!(filtered_result.representatives.len(), 1);
    assert!(filtered_result
        .representatives
        .contains_key(&new_rep.into()));
    assert!(!filtered_result
        .representatives
        .contains_key(&(*DEV_GENESIS_ACCOUNT)));

    // Ensure node2 has the same view of online representatives
    let node2_online_reps = node2.online_reps.lock().unwrap().online_reps().count();
    assert_eq!(
        node2_online_reps, 2,
        "Node2 doesn't have the correct number of online representatives"
    );

    server.abort();
}
