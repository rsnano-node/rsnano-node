use std::{
    cmp::max,
    collections::HashMap,
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
    },
    time::{Duration, Instant},
};

use rsnano_ledger::{
    AnySet, BlockError, BlockSource, ConfirmedSet, DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH,
    DEV_GENESIS_PUB_KEY, LedgerSet, test_helpers::UnsavedBlockLatticeBuilder,
};
use rsnano_messages::{ConfirmAck, Message, Publish};
use rsnano_network::{ChannelId, TrafficType};
use rsnano_node::{
    NodeEvent,
    block_processing::{BlockContext, BoundedBacklogConfig},
    config::{NodeConfig, NodeFlags},
    consensus::{FilteredVote, ReceivedVote, election::VoteType},
};
use rsnano_nullable_tcp::get_available_port;
use rsnano_types::{
    Account, Amount, Block, BlockHash, DEV_GENESIS_KEY, DifficultyV1, PrivateKey, PublicKey, Root,
    Signature, StateBlockArgs, UnixMillisTimestamp, Vote, VoteSource, WorkRequest,
};
use rsnano_utils::{
    BackpressureHandler,
    stats::{DetailType, Direction, StatType},
};
use test_helpers::{
    System, activate_hashes, assert_never, assert_timely, assert_timely_eq, assert_timely_eq2,
    assert_timely_msg, assert_timely2, establish_tcp, make_fake_channel, setup_chains,
    start_election,
};

#[test]
fn rollback_gap_source() {
    let mut system = System::new();
    let node = system.build_node().finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();
    let send1 = lattice.genesis().send(&key, 1);
    let send2 = lattice.genesis().send(&key, 1);

    let mut fork_lattice = lattice.clone();
    let fork1a = lattice.account(&key).receive(&send1);
    let fork1b = fork_lattice.account(&key).receive(&send2);

    node.process_local(send1.clone()).unwrap();
    node.process_local(fork1a.clone()).unwrap();

    assert!(!node.block_exists(&send2.hash()));
    node.block_processor_queue.push(BlockContext::new(
        fork1b.clone(),
        BlockSource::Forced,
        ChannelId::LOOPBACK,
    ));

    assert_timely2(|| node.block(&fork1a.hash()).is_none());

    assert_timely_eq2(
        || {
            node.stats
                .count(StatType::Rollback, DetailType::Open, Direction::In)
        },
        1,
    );

    assert!(!node.block_exists(&fork1b.hash()));

    node.process_active(fork1a.clone());
    assert_timely2(|| node.block_exists(&fork1a.hash()));

    node.process_local(send2.clone()).unwrap();
    node.block_processor_queue.push(BlockContext::new(
        fork1b.clone(),
        BlockSource::Forced,
        ChannelId::LOOPBACK,
    ));

    assert_timely_eq2(
        || {
            node.stats
                .count(StatType::Rollback, DetailType::Open, Direction::In)
        },
        2,
    );

    assert_timely2(|| node.block_exists(&fork1b.hash()));
    assert!(!node.block_exists(&fork1a.hash()));
}

#[test]
fn vote_by_hash_bundle() {
    // Initialize the test system with one node
    let mut system = System::new();
    let (tx, rx) = mpsc::sync_channel(128);
    let node = system.build_node().event_sink(tx).finish();
    let wallet_id = node.wallets.wallet_ids()[0];

    // Prepare a vector to hold the blocks
    let mut blocks = Vec::new();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    // Create the first block in the chain
    let block = lattice.genesis().send(&*DEV_GENESIS_KEY, 1);

    blocks.push(block.clone());
    node.process(block);

    // Create a chain of blocks
    for _ in 2..20 {
        let block = lattice.genesis().send(&*DEV_GENESIS_KEY, 1);
        blocks.push(block.clone());
        node.process(block);
    }

    // Confirm the last block to confirm the entire chain
    node.ledger.confirm(blocks.last().unwrap().hash());

    // Insert the genesis key and a new key into the wallet
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), false)
        .unwrap();

    assert_timely_eq2(|| node.wallet_reps.lock().unwrap().voting_reps(), 1);

    // Enqueue vote requests for all the blocks
    for block in &blocks {
        node.vote_generators
            .generate_vote(&block.root(), &block.hash(), VoteType::NonFinal);
    }

    let mut max_hashes = 0;
    let start = Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(5) {
            panic!("timeout!");
        }

        match rx.try_recv() {
            Ok(e) => {
                if let NodeEvent::VoteProcessed(vote, _) = e {
                    max_hashes = max(max_hashes, vote.hashes.len());

                    if max_hashes >= 3 {
                        break;
                    }
                }
            }
            Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(1)),
        }
    }

    assert!(max_hashes >= 3);
}

#[test]
fn confirm_quorum() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    // Put greater than node.delta() in pending so quorum can't be reached
    let new_balance = node1.online_reps.lock().unwrap().quorum_delta() - Amount::raw(1);
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice
        .genesis()
        .send_all_except(&*DEV_GENESIS_KEY, new_balance);

    node1.process_local(send1.clone()).unwrap();

    node1
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            *DEV_GENESIS_ACCOUNT,
            new_balance,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));

    let votes = node1
        .active
        .read()
        .unwrap()
        .election_for_root(&send1.qualified_root())
        .unwrap()
        .vote_count();
    assert_eq!(0, votes);
}

#[test]
fn send_callback() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();

    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();

    let send_result = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait();

    assert!(send_result.is_ok());

    assert_timely2(|| node.balance(&key2.account()).is_zero());

    assert_eq!(
        Amount::MAX - node.config.receive_minimum,
        node.balance(&DEV_GENESIS_ACCOUNT)
    );
}

// Test that nodes can disable representative voting
#[test]
fn no_voting() {
    let mut system = System::new();
    let node0 = system.make_node();
    let mut config = System::default_config();
    config.enable_voting = false;
    let node1 = system.build_node().config(config).finish();

    let wallet_id1 = node1.wallets.wallet_ids()[0];

    // Node1 has a rep
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    let key1 = PrivateKey::new();
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &key1.raw_key(), true)
        .unwrap();
    // Broadcast a confirm so others should know this is a rep node
    node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key1.account(),
            Amount::nano(1),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely_eq2(|| node0.active.read().unwrap().len(), 0);
    assert_eq!(
        node0
            .stats
            .count(StatType::Message, DetailType::ConfirmAck, Direction::In),
        0
    );
}

// Bootstrapping a forked open block should succeed.
#[test]
fn bootstrap_fork_open() {
    let mut system = System::new();
    let mut node_config = System::default_config();
    // Reduce cooldown to speed up fork resolution
    node_config.bootstrap.candidate_accounts.cooldown = Duration::from_millis(100);
    // Make sure we can process the full account number range
    node_config.bootstrap.frontier_scan.parallelism = 3;
    // Disable rate limiting to speed up the scan
    node_config.bootstrap.frontier_rate_limit = 0;
    // Disable automatic election activation
    node_config.backlog_scan.enabled = false;
    node_config.enable_priority_scheduler = false;
    node_config.enable_hinted_scheduler = false;
    node_config.enable_optimistic_scheduler = false;

    let node0 = system.build_node().config(node_config.clone()).finish();
    node_config.network.listening_port = get_available_port();
    let node1 = system.build_node().config(node_config).finish();
    node0.local_block_broadcaster.stop();
    node1.local_block_broadcaster.stop();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key0 = PrivateKey::new();

    let send0 = lattice.genesis().send(&key0, 500);
    let mut fork_lattice = lattice.clone();

    let open0 = lattice
        .account(&key0)
        .receive_and_change(&send0, PublicKey::from_bytes([1; 32]));

    let open1 = fork_lattice
        .account(&key0)
        .receive_and_change(&send0, PublicKey::from_bytes([2; 32]));

    // Both know about send0
    node0.process(send0.clone());
    node1.process(send0.clone());

    // Confirm send0 to allow starting and voting on the following blocks
    node0.confirm(send0.hash());
    node1.confirm(send0.hash());

    // They disagree about open0/open1
    node0.process(open0.clone());
    node1.process(open1.clone());

    node0.confirming_set.add_block(open0.hash());
    assert_timely2(|| node0.block_confirmed(&open0.hash()));

    // Start election for open block which is necessary to resolve the fork
    start_election(&node1, &open1.hash());
    assert_timely2(|| node1.is_active_root(&open1.qualified_root()));

    // Allow node0 to vote on its fork

    node0.insert_into_wallet(&DEV_GENESIS_KEY);

    assert_timely2(|| !node1.block_exists(&open1.hash()) && node1.block_exists(&open0.hash()));
}

#[test]
fn rep_self_vote() {
    let mut system = System::new();
    let node0 = system
        .build_node()
        .flags(NodeFlags {
            // Prevent automatic election cleanup
            disable_request_loop: true,
            ..Default::default()
        })
        .config(NodeConfig {
            online_weight_minimum: Amount::MAX,
            // Disable automatic election activation
            enable_priority_scheduler: false,
            enable_hinted_scheduler: false,
            enable_optimistic_scheduler: false,
            ..System::default_config_without_backlog_scan()
        })
        .finish();

    let rep_big = PrivateKey::new();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let fund_big = lattice.genesis().send_all_except(
        &rep_big,
        Amount::raw(0xb000_0000_0000_0000_0000_0000_0000_0000),
    );

    let open_big = lattice.account(&rep_big).receive(&fund_big);

    node0.process_local(fund_big.clone()).unwrap();
    node0.process_local(open_big.clone()).unwrap();

    // Confirm both blocks, allowing voting on the upcoming block
    start_election(&node0, &open_big.hash());

    assert_timely2(|| node0.is_active_root(&open_big.qualified_root()));
    node0.force_confirm(&open_big.hash());

    // Insert representatives into the node to allow voting
    node0.insert_into_wallet(&rep_big);
    node0.insert_into_wallet(&DEV_GENESIS_KEY);
    assert_timely_eq2(|| node0.wallet_reps.lock().unwrap().voting_reps(), 2);

    let block0 = lattice.genesis().send_all_except(
        &rep_big,
        Amount::raw(0x6000_0000_0000_0000_0000_0000_0000_0000),
    );

    node0.process_local(block0.clone()).unwrap();

    start_election(&node0, &block0.hash());

    // Wait until representatives are activated & make vote
    assert_timely2(|| node0.ledger.confirmed().block_exists(&block0.hash()));
    let info = node0
        .recently_cemented
        .lock()
        .unwrap()
        .iter()
        .find(|i| i.winner.hash() == block0.hash())
        .unwrap()
        .clone();
    assert_eq!(info.voter_count, 2);
}

#[test]
fn fork_bootstrap_flip() {
    let mut system = System::new();
    let config1 = System::default_config_without_backlog_scan();

    let node1 = system.build_node().config(config1).finish();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let mut config2 = System::default_config();
    // Reduce cooldown to speed up fork resolution
    config2.bootstrap.candidate_accounts.cooldown = Duration::from_millis(100);
    let node2 = system.build_node().config(config2).disconnected().finish();
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = lattice.clone();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().legacy_send(&key1, Amount::raw(1_000_000));

    let key2 = PrivateKey::new();
    let send2 = fork_lattice
        .genesis()
        .legacy_send(&key2, Amount::raw(1_000_000));

    // Insert but don't rebroadcast, simulating settled blocks
    node1.process_local(send1.clone()).unwrap();
    node2.process_local(send2.clone()).unwrap();

    node1.confirm(send1.hash());
    assert_timely2(|| node1.block_exists(&send1.hash()));
    assert_timely2(|| node2.block_exists(&send2.hash()));

    // Additionally add new peer to confirm & replace bootstrap block
    //node2.network.merge_peer(node1.network.endpoint());
    establish_tcp(&node2, &node1);

    assert_timely_msg(
        Duration::from_secs(10),
        || node2.block_exists(&send1.hash()),
        "send1 not found on node2 after bootstrap",
    );
}

// Test that more than one block can be rolled back
#[test]
fn fork_multi_flip() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    let mut flags = NodeFlags::default();
    flags.disable_block_processor_republishing = true;
    let node1 = system
        .build_node()
        .config(config.clone())
        .flags(flags.clone())
        .finish();
    node1.local_block_broadcaster.stop();

    config.network.listening_port = get_available_port();
    // Reduce cooldown to speed up fork resolution
    config.bootstrap.candidate_accounts.cooldown = Duration::from_millis(100);
    let node2 = system
        .build_node()
        .config(config)
        .flags(flags)
        .disconnected()
        .finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = lattice.clone();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().legacy_send(&key1, 100);

    let key2 = PrivateKey::new();
    let send2 = fork_lattice.genesis().legacy_send(&key2, 100);
    let send3 = fork_lattice.genesis().legacy_send(&key2, 0);

    node1.process(send1.clone());
    // Node2 has two blocks that will be rolled back by node1's vote
    node2.process(send2.clone());
    node2.process(send3.clone());

    establish_tcp(&node1, &node2);

    // Insert voting key into node1
    node1.insert_into_wallet(&DEV_GENESIS_KEY);

    assert_timely2(|| {
        let aec = node2.active.read().unwrap();
        if let Some(election) = aec.election_for_root(&send2.qualified_root()) {
            election.contains_block(&send1.hash())
        } else {
            false
        }
    });

    node1.confirm(send1.hash());
    assert_timely2(|| node2.ledger.any().block_exists(&send1.hash()));
    assert!(!node2.ledger.any().block_exists(&send2.hash()));
    assert!(!node2.ledger.any().block_exists(&send3.hash()));
}

// This test is racy, there is no guarantee that the election won't be confirmed until all forks are fully processed
#[test]
#[ignore = "The test has race conditions. Rewrite it as unit test"]
fn fork_publish() {
    let mut system = System::new();
    let node1 = system.make_node();
    node1.insert_into_wallet(&DEV_GENESIS_KEY);
    let key1 = PrivateKey::new();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = lattice.clone();
    let send1 = lattice.genesis().send(&key1, 100);
    let key2 = PrivateKey::new();
    let send2 = fork_lattice.genesis().send(&key2, 100);
    node1.process_active(send1.clone());
    node1.process_active(send2.clone());
    assert_timely_eq2(|| node1.active.read().unwrap().len(), 1);
    assert_timely2(|| node1.is_active_root(&send2.qualified_root()));
    // Wait until the genesis rep activated & makes vote
    assert_timely_eq2(
        || {
            let aec = node1.active.read().unwrap();
            if let Some(e) = aec.election_for_root(&send1.qualified_root()) {
                e.vote_count()
            } else {
                0
            }
        },
        1,
    );
    let votes1 = node1
        .active
        .read()
        .unwrap()
        .election_for_root(&send1.qualified_root())
        .unwrap()
        .votes()
        .clone();
    let existing1 = votes1.get(&DEV_GENESIS_PUB_KEY).unwrap();
    assert_eq!(send1.hash(), existing1.hash);
}

// In test case there used to be a race condition, it was worked around in:.
// https://github.com/nanocurrency/nano-node/pull/4091
// The election and the processing of block send2 happen in parallel.
// Usually the election happens first and the send2 block is added to the election.
// However, if the send2 block is processed before the election is started then
// there is a race somewhere and the election might not notice the send2 block.
// The test case can be made to pass by ensuring the election is started before the send2 is processed.
// However, is this a problem with the test case or this is a problem with the node handling of forks?
#[test]
#[ignore = "The test has race conditions. Rewrite it as unit test"]
fn fork_publish_inactive() {
    let mut system = System::new();
    let node = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = lattice.clone();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();

    let send1 = lattice.genesis().legacy_send(&key1, 100);
    let send2 = fork_lattice.genesis().legacy_send(&key2, 100);

    node.process_active(send1.clone());
    assert_timely2(|| node.block_exists(&send1.hash()));
    assert_timely2(|| node.is_active_root(&send1.qualified_root()));

    assert_eq!(node.process_local(send2.clone()), Err(BlockError::Fork));

    assert_timely_eq2(
        || {
            node.active
                .read()
                .unwrap()
                .election_for_root(&send1.qualified_root())
                .unwrap()
                .block_count()
        },
        2,
    );

    assert_eq!(
        node.active
            .read()
            .unwrap()
            .election_for_root(&send1.qualified_root())
            .unwrap()
            .winner()
            .hash(),
        send1.hash()
    );
}

#[test]
fn unlock_search() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    let balance = node.balance(&DEV_GENESIS_ACCOUNT);

    node.wallets.rekey(&wallet_id, "").unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely2(|| node.balance(&DEV_GENESIS_ACCOUNT) != balance);

    assert_timely_eq(
        Duration::from_secs(10),
        || node.active.read().unwrap().len(),
        0,
    );

    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();
    //node.wallets
    //    .set_password(&wallet_id, &KeyPair::new().private_key())
    //    .unwrap();
    node.wallets.enter_password(wallet_id, "").unwrap();

    assert_timely_msg(
        Duration::from_secs(10),
        || !node.balance(&key2.account()).is_zero(),
        "balance is still zero",
    );
}

#[test]
fn search_receivable_confirmed() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let send1 = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();
    assert_timely2(|| node.block_hashes_confirmed(&[send1.hash()]));

    let send2 = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();
    assert_timely2(|| node.block_hashes_confirmed(&[send2.hash()]));

    node.wallets
        .remove_key(&wallet_id, &*DEV_GENESIS_PUB_KEY)
        .unwrap();

    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();

    node.wallets.search_receivable(&wallet_id);

    assert_timely2(|| !node.is_active_root(&send1.qualified_root()));
    assert_timely2(|| !node.is_active_root(&send2.qualified_root()));

    assert_timely_eq2(
        || node.balance(&key2.account()),
        node.config.receive_minimum * 2,
    );
}

#[test]
fn search_receivable() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();

    node.wallets.search_receivable(&wallet_id).wait().unwrap();

    assert_timely_msg(
        Duration::from_secs(10),
        || !node.balance(&key2.account()).is_zero(),
        "balance is still zero",
    );
}

#[test]
fn search_receivable_same() {
    let mut system = System::new();
    let node = system.make_node();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();

    let send_result1 = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait_timeout(Duration::from_secs(5));
    assert!(send_result1.is_ok());

    let send_result2 = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait_timeout(Duration::from_secs(5));
    assert!(send_result2.is_ok());

    node.insert_into_wallet(&key2);
    node.wallets
        .search_receivable(&wallet_id)
        .wait_timeout(Duration::from_secs(5))
        .unwrap();

    assert_timely2(|| node.balance(&key2.account()) == node.config.receive_minimum * 2);
}

#[test]
fn search_receivable_multiple() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    node.insert_into_wallet(&key3);

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key3.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait_timeout(Duration::from_secs(5))
        .unwrap();

    assert_timely2(|| !node.balance(&key3.account()).is_zero());

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait_timeout(Duration::from_secs(5))
        .unwrap();

    node.wallets
        .send(
            wallet_id,
            key3.account(),
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait_timeout(Duration::from_secs(5))
        .unwrap();

    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();

    node.wallets
        .search_receivable(&wallet_id)
        .wait_timeout(Duration::from_secs(5))
        .unwrap();

    assert_timely2(|| node.balance(&key2.account()) == node.config.receive_minimum * 2);
}

#[test]
fn auto_bootstrap() {
    let mut system = System::new();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let destination = PrivateKey::new();
    let amount = Amount::nano(1000);
    let send = lattice.genesis().send(&destination, amount);
    let receive = lattice.account(&destination).receive(&send);

    let node0 = system.make_node();
    node0.process_and_confirm_multi(&[send, receive]);

    let node1 = system.make_node();
    assert_timely_eq2(|| node1.balance(&destination.account()), amount);
}

#[test]
fn quick_confirm() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];
    let key = PrivateKey::new();

    node1
        .wallets
        .insert_adhoc2(&wallet_id, &key.raw_key(), true)
        .unwrap();
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send_all_except(
        &key,
        node1.online_reps.lock().unwrap().quorum_delta() + Amount::raw(1),
    );

    node1.process_active(send.clone());

    assert_timely_msg(
        Duration::from_secs(10),
        || !node1.balance(&key.account()).is_zero(),
        "balance is still zero",
    );

    assert_eq!(
        node1.balance(&DEV_GENESIS_ACCOUNT),
        node1.online_reps.lock().unwrap().quorum_delta() + Amount::raw(1)
    );

    assert_eq!(
        node1.balance(&key.account()),
        Amount::MAX - (node1.online_reps.lock().unwrap().quorum_delta() + Amount::raw(1))
    );
}

#[test]
fn send_out_of_order() {
    let mut system = System::new();
    let node1 = system.make_node();
    let key2 = PrivateKey::new();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice.genesis().send(&key2, node1.config.receive_minimum);
    let send2 = lattice.genesis().send(&key2, node1.config.receive_minimum);
    let send3 = lattice.genesis().send(&key2, node1.config.receive_minimum);

    node1.process_active(send3.clone());
    node1.process_active(send2.clone());
    node1.process_active(send1.clone());

    assert_timely_msg(
        Duration::from_secs(10),
        || {
            system.nodes.iter().all(|node| {
                node.balance(&DEV_GENESIS_ACCOUNT) == Amount::MAX - node1.config.receive_minimum * 3
            })
        },
        "balance is incorrect on at least one node",
    );
}

#[test]
fn send_single_observing_peer() {
    let mut system = System::new();
    let key2 = PrivateKey::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];

    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node2
        .wallets
        .insert_adhoc2(&wallet_id2, &key2.raw_key(), true)
        .unwrap();

    node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node1.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_eq!(
        Amount::MAX - node1.config.receive_minimum,
        node1.balance(&DEV_GENESIS_ACCOUNT)
    );

    assert!(node1.balance(&key2.account()).is_zero());

    assert_timely_msg(
        Duration::from_secs(10),
        || {
            system
                .nodes
                .iter()
                .all(|node| !node.balance(&key2.account()).is_zero())
        },
        "balance is still zero on at least one node",
    );
}

#[test]
fn send_single() {
    let mut system = System::new();
    let key2 = PrivateKey::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];

    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node2
        .wallets
        .insert_adhoc2(&wallet_id2, &key2.raw_key(), true)
        .unwrap();

    node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node1.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_eq!(
        Amount::MAX - node1.config.receive_minimum,
        node1.balance(&DEV_GENESIS_ACCOUNT)
    );

    assert!(node1.balance(&key2.account()).is_zero());

    assert_timely_msg(
        Duration::from_secs(10),
        || !node1.balance(&key2.account()).is_zero(),
        "balance is still zero",
    );
}

#[test]
fn send_self() {
    let mut system = System::new();
    let key2 = PrivateKey::new();
    let node = system.make_node();
    let wallet_id = node.wallets.wallet_ids()[0];
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            node.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely_msg(
        Duration::from_secs(10),
        || !node.balance(&key2.account()).is_zero(),
        "balance is still zero",
    );

    assert_eq!(
        Amount::MAX - node.config.receive_minimum,
        node.balance(&DEV_GENESIS_ACCOUNT)
    );
}

#[test]
fn balance() {
    let mut system = System::new();
    let node = system.make_node();

    let wallet_id = node.wallets.wallet_ids()[0];
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let balance = node.balance(&DEV_GENESIS_KEY.account());

    assert_eq!(Amount::MAX, balance);
}

#[test]
fn work_generate() {
    let mut system = System::new();
    let node = system.make_node();
    let root = Root::from(1);

    // Test with higher difficulty
    {
        let difficulty =
            DifficultyV1::from_multiplier(1.5, node.network_params.work.threshold_base());

        let work = node
            .work_factory
            .generate_work(WorkRequest::new(root, difficulty));

        assert!(work.is_some());
        let work = work.unwrap();
        assert!(node.network_params.work.difficulty(&root, work) >= difficulty);
    }

    // Test with lower difficulty
    {
        let difficulty =
            DifficultyV1::from_multiplier(0.5, node.network_params.work.threshold_base());
        let mut work;
        loop {
            work = node
                .work_factory
                .generate_work(WorkRequest::new(root, difficulty));
            if let Some(work_value) = work {
                if node.network_params.work.difficulty(&root, work_value)
                    < node.network_params.work.threshold_base()
                {
                    break;
                }
            }
        }
        let work = work.unwrap();
        assert!(node.network_params.work.difficulty(&root, work) >= difficulty);
        assert!(
            node.network_params.work.difficulty(&root, work)
                < node.network_params.work.threshold_base()
        );
    }
}

#[test]
fn local_block_broadcast() {
    let mut system = System::new();

    let mut node_config = System::default_config();
    node_config.enable_priority_scheduler = false;
    node_config.enable_hinted_scheduler = false;
    node_config.enable_optimistic_scheduler = false;
    node_config.local_block_broadcaster.rebroadcast_interval = Duration::from_secs(1);

    let node1 = system.build_node().config(node_config).finish();
    let node2 = system.make_disconnected_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();

    let send1 = lattice.genesis().send(&key1, 1000);
    let qualified_root = send1.qualified_root();
    let send_hash = send1.hash();
    node1.process_local(send1).unwrap();

    assert_never(Duration::from_millis(500), || {
        node1.is_active_root(&qualified_root)
    });

    // Wait until a broadcast is attempted
    assert_timely_eq2(|| node1.local_block_broadcaster.len(), 1);
    assert_timely2(|| {
        node1.stats.count(
            StatType::LocalBlockBroadcaster,
            DetailType::Broadcast,
            Direction::Out,
        ) >= 1
    });

    // The other node should not have received a block
    assert_never(Duration::from_millis(500), || {
        node2.block_exists(&send_hash)
    });

    // Connect the nodes and check that the block is propagated
    let _ = node1
        .peer_connector
        .connect_to(node2.tcp_listener.local_address());

    assert_timely2(|| {
        node1
            .network
            .read()
            .unwrap()
            .find_node_id(&node2.get_node_id())
            .is_some()
    });
    assert_timely2(|| node2.block_exists(&send_hash))
}

#[test]
fn fork_no_vote_quorum() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let node3 = system.make_node();
    node1.local_block_broadcaster.stop();
    node2.local_block_broadcaster.stop();
    node3.local_block_broadcaster.stop();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    let wallet_id3 = node3.wallets.wallet_ids()[0];

    node1.insert_into_wallet(&DEV_GENESIS_KEY);

    let key4 = node1
        .wallets
        .deterministic_insert2(&wallet_id1, true)
        .unwrap();

    node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key4.into(),
            Amount::MAX / 4,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    let key1 = node2
        .wallets
        .deterministic_insert2(&wallet_id2, true)
        .unwrap();

    node2
        .wallets
        .set_representative(wallet_id2, key1, false)
        .wait()
        .unwrap();

    let block = node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key1.into(),
            node1.config.receive_minimum,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely_msg(
        Duration::from_secs(30),
        || {
            node3.balance(&key1.into()) == node1.config.receive_minimum
                && node2.balance(&key1.into()) == node1.config.receive_minimum
                && node1.balance(&key1.into()) == node1.config.receive_minimum
        },
        "balances are wrong",
    );
    assert_eq!(node1.config.receive_minimum, node1.ledger.weight(&key1));
    assert_eq!(node1.config.receive_minimum, node2.ledger.weight(&key1));
    assert_eq!(node1.config.receive_minimum, node3.ledger.weight(&key1));

    let send1: Block = StateBlockArgs {
        key: &DEV_GENESIS_KEY,
        previous: block.hash(),
        representative: *DEV_GENESIS_PUB_KEY,
        balance: (Amount::MAX / 4) - (node1.config.receive_minimum * 2),
        link: Account::from(key1).into(),
        work: node1.work_generate_dev(block.hash()),
    }
    .into();

    node1.process(send1.clone());
    node2.process(send1.clone());
    node3.process(send1.clone());

    let key2 = node3
        .wallets
        .deterministic_insert2(&wallet_id3, true)
        .unwrap();

    let send2: Block = StateBlockArgs {
        key: &DEV_GENESIS_KEY,
        previous: block.hash(),
        representative: *DEV_GENESIS_PUB_KEY,
        balance: (Amount::MAX / 4) - (node1.config.receive_minimum * 2),
        link: Account::from(key2).into(),
        work: node1.work_generate_dev(block.hash()),
    }
    .into();

    let vote = Vote::new(
        &PrivateKey::new(),
        UnixMillisTimestamp::ZERO,
        0,
        vec![send2.hash()],
    );
    let confirm = Message::ConfirmAck(ConfirmAck::new_with_own_vote(vote));
    let channel = node2
        .network
        .read()
        .unwrap()
        .find_node_id(&node3.node_id())
        .unwrap()
        .clone();
    node2
        .message_sender
        .lock()
        .unwrap()
        .try_send(&channel, &confirm, TrafficType::Generic);

    assert_timely_msg(
        Duration::from_secs(10),
        || {
            node3
                .stats
                .count(StatType::Message, DetailType::ConfirmAck, Direction::In)
                >= 3
        },
        "no confirm ack",
    );
    assert_eq!(node1.latest(&DEV_GENESIS_ACCOUNT), send1.hash());
    assert_eq!(node2.latest(&DEV_GENESIS_ACCOUNT), send1.hash());
    assert_eq!(node3.latest(&DEV_GENESIS_ACCOUNT), send1.hash());
}

#[test]
fn fork_open() {
    let mut system = System::new();
    let node = system.make_node();

    // create block send1, to send all the balance from genesis to key1
    // this is done to ensure that the open block(s) cannot be voted on and confirmed
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().send(&key1, Amount::MAX);
    let mut fork_lattice = lattice.clone();

    node.process(send1.clone());
    node.confirm(send1.hash());

    // create the 1st open block to receive send1, which should be regarded as the winner just because it is first
    let open1 = lattice.account(&key1).receive_and_change(&send1, 1);
    let channel = make_fake_channel(&node);
    node.inbound_message_queue.put(
        Message::Publish(Publish::new_forward(open1.clone())),
        channel.clone(),
    );
    assert_timely_eq2(|| node.active.read().unwrap().len(), 1);

    // create 2nd open block, which is a fork of open1 block
    // create the 1st open block to receive send1, which should be regarded as the winner just because it is first
    let open2 = fork_lattice.account(&key1).receive_and_change(&send1, 2);
    node.inbound_message_queue.put(
        Message::Publish(Publish::new_forward(open2.clone())),
        channel.clone(),
    );
    assert_timely2(|| node.is_active_root(&open2.qualified_root()));

    // we expect to find 2 blocks in the election and we expect the first block to be the winner just because it was first
    assert_timely_eq2(
        || {
            node.active
                .read()
                .unwrap()
                .election_for_root(&open2.qualified_root())
                .unwrap()
                .block_count()
        },
        2,
    );
    assert_eq!(
        open1.hash(),
        node.active
            .read()
            .unwrap()
            .election_for_root(&open2.qualified_root())
            .unwrap()
            .winner()
            .hash()
    );

    // check that only the first block is saved to the ledger
    assert_timely2(|| node.block_exists(&open1.hash()));
    assert_eq!(node.block_exists(&open2.hash()), false);
}

#[test]
fn online_reps_rep_crawler() {
    let mut system = System::new();
    let mut flags = NodeFlags::default();
    flags.disable_rep_crawler = true;
    let node = system.build_node().flags(flags).finish();

    // Without rep crawler
    let channel = make_fake_channel(&node);

    let vote: FilteredVote = ReceivedVote::new(
        Arc::new(Vote::new(
            &DEV_GENESIS_KEY,
            UnixMillisTimestamp::now(),
            0,
            vec![*DEV_GENESIS_HASH],
        )),
        VoteSource::Live,
        Some(channel.clone()),
    )
    .into();

    assert_eq!(
        Amount::ZERO,
        node.online_reps.lock().unwrap().online_weight()
    );

    let _ = node.vote_processor.vote_blocking(&vote);
    assert_eq!(
        Amount::ZERO,
        node.online_reps.lock().unwrap().online_weight()
    );

    // After inserting to rep crawler
    node.rep_crawler
        .force_query(*DEV_GENESIS_HASH, channel.channel_id());
    let _ = node.vote_processor.vote_blocking(&vote);

    assert_timely_eq2(
        || node.online_reps.lock().unwrap().online_weight(),
        Amount::MAX,
    );
}

#[test]
fn online_reps_election() {
    let mut system = System::new();
    let mut flags = NodeFlags::default();
    flags.disable_rep_crawler = true;
    let node = system.build_node().flags(flags).finish();

    // Start election
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();
    let send1 = lattice.genesis().send(&key, Amount::nano(1000));

    node.process_active(send1.clone());
    assert_timely_eq(
        Duration::from_secs(5),
        || node.active.read().unwrap().len(),
        1,
    );

    // Process vote for ongoing election
    let vote = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        UnixMillisTimestamp::now(),
        0,
        vec![send1.hash()],
    ));
    assert_eq!(
        Amount::ZERO,
        node.online_reps.lock().unwrap().online_weight()
    );

    let channel = make_fake_channel(&node);
    let _ = node
        .vote_processor
        .vote_blocking(&ReceivedVote::new(vote.into(), VoteSource::Live, Some(channel)).into());

    assert_eq!(
        Amount::MAX - Amount::nano(1000),
        node.online_reps.lock().unwrap().online_weight()
    );
}

#[test]
fn vote_republish() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let key2 = PrivateKey::new();
    // by not setting a private key on node1's wallet for genesis account, it is stopped from voting
    node2.insert_into_wallet(&key2);

    // send1 and send2 are forks of each other
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice.genesis().send(&key2, Amount::nano(1000));
    let send2 = fork_lattice.genesis().send(&key2, Amount::nano(2000));

    // process send1 first, this will make sure send1 goes into the ledger and an election is started
    node1.process_active(send1.clone());
    assert_timely2(|| node2.block_exists(&send1.hash()));
    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));
    assert_timely2(|| node2.is_active_root(&send1.qualified_root()));

    // now process send2, send2 will not go in the ledger because only the first block of a fork goes in the ledger
    node1.process_active(send2.clone());
    assert_timely2(|| node1.is_active_root(&send2.qualified_root()));

    // send2 cannot be synced because it is not in the ledger of node1, it is only in the election object in RAM on node1
    assert_eq!(node1.block_exists(&send2.hash()), false);

    // the vote causes the election to reach quorum and for the vote (and block?) to be published from node1 to node2
    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send2.hash()]));
    node1
        .vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);

    // FIXME: there is a race condition here, if the vote arrives before the block then the vote is wasted and the test fails
    // we could resend the vote but then there is a race condition between the vote resending and the election reaching quorum on node1
    // the proper fix would be to observe on node2 that both the block and the vote arrived in whatever order
    // the real node will do a confirm request if it needs to find a lost vote

    // check that send2 won on both nodes
    assert_timely2(|| node1.block_confirmed(&send2.hash()));
    assert_timely2(|| node2.block_confirmed(&send2.hash()));

    // check that send1 is deleted from the ledger on nodes
    assert_eq!(node1.block_exists(&send1.hash()), false);
    assert_eq!(node2.block_exists(&send1.hash()), false);
    assert_timely_eq2(|| node1.balance(&key2.account()), Amount::nano(2000));
    assert_timely_eq2(|| node2.balance(&key2.account()), Amount::nano(2000));
}

// This test places block send1 onto every node. Then it creates block send2 (which is a fork of send1) and sends it to node1.
// Then it sends a vote for send2 to node1 and expects node2 to also get the block plus vote and confirm send2.
// TODO: This test enforces the order block followed by vote on node1, should vote followed by block also work? It doesn't currently.
#[test]
fn vote_by_hash_republish() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let key = PrivateKey::new();

    // send1 and send2 are forks of each other
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice.genesis().send(&key, Amount::nano(1000));
    let send2 = fork_lattice.genesis().send(&key, Amount::nano(2000));

    // give block send1 to node1 and check that an election for send1 starts on both nodes
    node1.process_active(send1.clone());
    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));
    assert_timely2(|| node2.is_active_root(&send1.qualified_root()));

    // give block send2 to node1 and wait until the block is received and processed by node1
    node1.network_filter.clear_all();
    node1.process_active(send2.clone());
    assert_timely2(|| node1.is_active_root(&send2.qualified_root()));

    // construct a vote for send2 in order to overturn send1
    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send2.hash()]));
    node1
        .vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);

    // send2 should win on both nodes
    assert_timely2(|| node1.blocks_confirmed(&[send2.clone()]));
    assert_timely2(|| node2.blocks_confirmed(&[send2.clone()]));
    assert_eq!(node1.block_exists(&send1.hash()), false);
    assert_eq!(node2.block_exists(&send1.hash()), false);
}

#[test]
fn fork_election_invalid_block_signature() {
    let mut system = System::new();
    let node1 = system.make_node();

    // send1 and send2 are forks of each other
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(1000));
    let send2 = fork_lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(2000));
    let mut send3 = send2.clone();
    send3.set_signature(Signature::new()); // Invalid signature

    let channel = make_fake_channel(&node1);
    node1.inbound_message_queue.put(
        Message::Publish(Publish::new_forward(send1.clone())),
        channel.clone(),
    );
    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));

    node1.inbound_message_queue.put(
        Message::Publish(Publish::new_forward(send3)),
        channel.clone(),
    );
    node1.inbound_message_queue.put(
        Message::Publish(Publish::new_forward(send2.clone())),
        channel.clone(),
    );
    assert_timely2(|| {
        node1
            .active
            .read()
            .unwrap()
            .election_for_root(&send1.qualified_root())
            .unwrap()
            .block_count()
            > 1
    });
    assert_eq!(
        node1
            .active
            .read()
            .unwrap()
            .election_for_root(&send1.qualified_root())
            .unwrap()
            .candidate_blocks()
            .get(&send2.hash())
            .unwrap()
            .signature(),
        send2.signature()
    );
}

#[test]
fn confirm_back() {
    let mut system = System::new();
    let node = system.make_node();
    let key = PrivateKey::new();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice.genesis().send(&key, 1);
    let open = lattice.account(&key).receive(&send1);
    let send2 = lattice.account(&key).send(&*DEV_GENESIS_KEY, 1);

    node.process(send1.clone());
    node.process(open.clone());
    node.process(send2.clone());

    start_election(&node, &send1.hash());
    start_election(&node, &open.hash());
    start_election(&node, &send2.hash());
    assert_eq!(node.active.read().unwrap().len(), 3);
    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send2.hash()]));

    node.vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);

    assert_timely_eq2(|| node.active.read().unwrap().len(), 0);
}

// Test that rep_crawler removes unreachable reps from its search results.
// This test creates three principal representatives (rep1, rep2, genesis_rep) and
// one node for searching them (searching_node).
#[test]
fn rep_crawler_rep_remove() {
    let mut system = System::new();
    let searching_node = system.make_node(); // will be used to find principal representatives
    let key_rep1 = PrivateKey::new(); // Principal representative 1
    let key_rep2 = PrivateKey::new(); // Principal representative 2
    //
    let rep_weight = (Amount::MAX / 1000) * 2;

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    // Send enough nanos to Rep1 to make it a principal representative
    let send_to_rep1 = lattice.genesis().send(&key_rep1, rep_weight);

    // Receive by Rep1
    let receive_rep1 = lattice.account(&key_rep1).receive(&send_to_rep1);

    // Send enough nanos to Rep2 to make it a principal representative
    let send_to_rep2 = lattice.genesis().send(&key_rep2, rep_weight);

    // Receive by Rep2
    let receive_rep2 = lattice.account(&key_rep2).receive(&send_to_rep2);

    searching_node.process(send_to_rep1);
    searching_node.process(receive_rep1);
    searching_node.process(send_to_rep2);
    searching_node.process(receive_rep2);

    // Create channel for Rep1
    let channel_rep1 = make_fake_channel(&searching_node);

    // Ensure Rep1 is found by the rep_crawler after receiving a vote from it
    let vote_rep1 = ReceivedVote::new(
        Arc::new(Vote::new(
            &key_rep1,
            UnixMillisTimestamp::ZERO,
            0,
            vec![*DEV_GENESIS_HASH],
        )),
        VoteSource::Live,
        Some(channel_rep1.clone()),
    );

    searching_node.rep_crawler.force_process2(vote_rep1);

    assert_timely_eq2(
        || {
            searching_node
                .online_reps
                .lock()
                .unwrap()
                .peered_reps_count()
        },
        1,
    );

    let reps = searching_node.online_reps.lock().unwrap().peered_reps();
    assert_eq!(1, reps.len());
    assert_eq!(rep_weight, searching_node.ledger.weight(&reps[0].rep_key));
    assert_eq!(key_rep1.public_key(), reps[0].rep_key);
    assert_eq!(channel_rep1.channel_id(), reps[0].channel_id());

    // When rep1 disconnects then rep1 should not be found anymore
    channel_rep1.close();
    assert_timely_eq2(
        || {
            searching_node
                .online_reps
                .lock()
                .unwrap()
                .peered_reps_count()
        },
        0,
    );

    // Add working node for genesis representative
    let node_genesis_rep = system.make_node();
    let wallet_id = node_genesis_rep.wallets.wallet_ids()[0];
    node_genesis_rep
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    let channel_genesis_rep = searching_node
        .network
        .read()
        .unwrap()
        .find_node_id(&node_genesis_rep.get_node_id())
        .unwrap()
        .clone();

    // genesis_rep should be found as principal representative after receiving a vote from it
    let vote_genesis_rep = ReceivedVote::new(
        Arc::new(Vote::new(
            &DEV_GENESIS_KEY,
            UnixMillisTimestamp::ZERO,
            0,
            vec![*DEV_GENESIS_HASH],
        )),
        VoteSource::Live,
        Some(channel_genesis_rep),
    );

    searching_node.rep_crawler.force_process2(vote_genesis_rep);

    assert_timely_eq2(
        || {
            searching_node
                .online_reps
                .lock()
                .unwrap()
                .peered_reps_count()
        },
        1,
    );

    // Start a node for Rep2 and wait until it is connected
    let node_rep2 = system.make_node();
    let _ = searching_node
        .peer_connector
        .connect_to(node_rep2.tcp_listener.local_address());

    assert_timely2(|| {
        searching_node
            .network
            .read()
            .unwrap()
            .find_node_id(&node_rep2.get_node_id())
            .is_some()
    });
    let channel_rep2 = searching_node
        .network
        .read()
        .unwrap()
        .find_node_id(&node_rep2.get_node_id())
        .unwrap()
        .clone();

    // Rep2 should be found as a principal representative after receiving a vote from it
    let vote_rep2 = ReceivedVote::new(
        Arc::new(Vote::new(
            &key_rep2,
            UnixMillisTimestamp::ZERO,
            0,
            vec![*DEV_GENESIS_HASH],
        )),
        VoteSource::Live,
        Some(channel_rep2),
    );

    searching_node.rep_crawler.force_process2(vote_rep2);

    assert_timely_eq2(
        || {
            searching_node
                .online_reps
                .lock()
                .unwrap()
                .peered_reps_count()
        },
        2,
    );

    // TODO rewrite this test and the missing part below this commit
    // ... part missing:
}

#[test]
fn epoch_conflict_confirm() {
    let mut system = System::new();
    let config0 = System::default_config_without_backlog_scan();
    let node0 = system.build_node().config(config0).finish();

    let config1 = System::default_config_without_backlog_scan();
    let node1 = system.build_node().config(config1).finish();

    // Node 1 is the voting node
    // Send sends to an account we control: send -> open -> change
    // Send2 sends to an account with public key of the open block
    // Epoch open qualified root: (open, 0) on account with the same public key as the hash of the open block
    // Epoch open and change have the same root!

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();

    let send = lattice.genesis().send(&key, 1);
    let open = lattice.account(&key).receive(&send);

    let change = lattice.account(&key).change(&key);
    let conflict_account = Account::from_bytes(*open.hash().as_bytes());
    let send2 = lattice.genesis().send(conflict_account, 1);
    let epoch_open = lattice.epoch_open(conflict_account);

    // Process initial blocks
    node0.process_multi(&[send.clone(), send2.clone(), open.clone()]);
    node1.process_multi(&[send.clone(), send2.clone(), open.clone()]);

    // Process conflicting blocks on nodes as blocks coming from live network
    node0.process_active(change.clone());
    node0.process_active(epoch_open.clone());
    node1.process_active(change.clone());
    node1.process_active(epoch_open.clone());

    // Ensure blocks were propagated to both nodes
    assert_timely2(|| node0.blocks_exist(&[change.clone(), epoch_open.clone()]));
    assert_timely2(|| node1.blocks_exist(&[change.clone(), epoch_open.clone()]));

    // Confirm initial blocks in node1 to allow generating votes later
    node1.confirm_multi(&[change.clone(), epoch_open.clone(), send2.clone()]);

    // Start elections on node0 for conflicting change and epoch_open blocks (those two blocks have the same root)
    activate_hashes(&node0, &[change.hash(), epoch_open.hash()]);
    assert_timely2(|| {
        node0.is_active_hash(&change.hash()) && node0.is_active_hash(&epoch_open.hash())
    });

    // Make node1 a representative so it can vote for both blocks
    node1.insert_into_wallet(&DEV_GENESIS_KEY);

    // Ensure both conflicting blocks were successfully processed and confirmed
    assert_timely2(|| node0.blocks_confirmed(&[change.clone(), epoch_open.clone()]));
}

#[test]
fn node_receive_quorum() {
    let mut system = System::new();
    let node1 = system.make_node();

    let wallet_id = node1.wallets.wallet_ids()[0];
    let key = PrivateKey::new();

    node1
        .wallets
        .insert_adhoc2(&wallet_id, &key.raw_key(), true)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(&key, Amount::nano(1000));
    node1.process_active(send.clone());

    assert_timely_msg(
        Duration::from_secs(10),
        || node1.block_exists(&send.hash()),
        "send block not found",
    );

    assert_timely_msg(
        Duration::from_secs(10),
        || node1.is_active_root(&send.qualified_root()),
        "election not found",
    );

    let node2 = system.make_disconnected_node();
    let wallet_id2 = node2.wallets.wallet_ids()[0];

    node2
        .wallets
        .insert_adhoc2(&wallet_id2, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    assert!(node1.balance(&key.account()).is_zero());

    let _ = node2
        .peer_connector
        .connect_to(node1.tcp_listener.local_address());

    assert_timely_msg(
        Duration::from_secs(10),
        || !node1.balance(&key.account()).is_zero(),
        "balance is still zero",
    );
}

#[test]
fn fork_open_flip() {
    let mut system = System::new();
    let node1 = system.make_node();
    let wallet_id = node1.wallets.wallet_ids()[0];

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let rep1 = PrivateKey::new();
    let rep2 = PrivateKey::new();

    // send 1 raw from genesis to key1 on both node1 and node2
    let send1 = lattice.genesis().legacy_send(&key1, 1);
    node1.process(send1.clone());

    let mut fork_lattice = lattice.clone();
    // We should be keeping this block
    let open1 = lattice.account(&key1).legacy_open_with_rep(&send1, &rep1);

    // create a fork of block open1, this block will lose the election
    let open2 = fork_lattice
        .account(&key1)
        .legacy_open_with_rep(&send1, &rep2);
    assert_ne!(open1.hash(), open2.hash());

    // give block open1 to node1, manually trigger an election for open1 and ensure it is in the ledger
    let open1 = node1.process(open1);
    node1.election_schedulers.manual.push(open1.clone());
    assert_timely2(|| node1.is_active_root(&open1.qualified_root()));
    node1
        .active
        .write()
        .unwrap()
        .transition_active(&open1.hash());

    // create node2, with blocks send1 and open2 pre-initialised in the ledger,
    // so that block open1 cannot possibly get in the ledger before open2 via background sync
    system.initialization_blocks.push(send1.clone());
    system.initialization_blocks.push(open2.clone());
    let node2 = system.make_node();
    system.initialization_blocks.clear();
    let open2 = node2.block(&open2.hash()).unwrap();

    // ensure open2 is in node2 ledger (and therefore has sideband) and manually trigger an election for open2
    assert_timely2(|| node2.block_exists(&open2.hash()));
    node2.election_schedulers.manual.push(open2.clone());
    assert_timely2(|| node2.is_active_root(&open2.qualified_root()));
    node2
        .active
        .write()
        .unwrap()
        .transition_active(&open2.hash());

    assert_timely_eq2(|| node1.active.read().unwrap().len(), 2);
    assert_timely_eq2(|| node2.active.read().unwrap().len(), 2);

    // allow node1 to vote and wait for open1 to be confirmed on node1
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    assert_timely2(|| node1.block_confirmed(&open1.hash()));

    // Notify both nodes of both blocks, both nodes will become aware that a fork exists
    node1.process_active(open2.clone().into());
    node2.process_active(open1.clone().into());

    // Node2 should eventually settle on open1
    assert_timely2(|| node2.block_exists(&open1.hash()));
    assert_timely2(|| node1.block_confirmed(&open1.hash()));

    // check the correct blocks are in the ledgers
    assert!(node1.block_exists(&open1.hash()));
    assert!(node2.block_exists(&&open1.hash()));
    assert!(!node2.block_exists(&open2.hash()));
}

#[test]
fn unconfirmed_send() {
    let mut system = System::new();

    let node1 = system.make_node();
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id1, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let key2 = PrivateKey::new();
    let node2 = system.make_node();
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    node2
        .wallets
        .insert_adhoc2(&wallet_id2, &key2.raw_key(), true)
        .unwrap();

    // firstly, send two units from node1 to node2 and expect that both nodes see the block as confirmed
    // (node1 will start an election for it, vote on it and node2 gets synced up)
    let send1 = node1
        .wallets
        .send(
            wallet_id1,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            Amount::nano(2),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    assert_timely2(|| node1.block_confirmed(&send1.hash()));
    assert_timely2(|| node2.block_confirmed(&send1.hash()));

    // wait until receive1 (auto-receive created by wallet) is cemented
    assert_timely_eq2(
        || {
            node2
                .ledger
                .confirmed()
                .get_conf_info(&key2.account())
                .unwrap_or_default()
                .height
        },
        1,
    );

    assert_eq!(node2.balance(&key2.account()), Amount::nano(2));

    let recv1 = node2
        .ledger
        .any()
        .find_receive_block_by_send_hash(&key2.account(), &send1.hash())
        .unwrap();

    // create send2 to send from node2 to node1 and save it to node2's ledger without triggering an election (node1 does not hear about it)
    let send2: Block = StateBlockArgs {
        key: &key2,
        previous: recv1.hash(),
        representative: *DEV_GENESIS_PUB_KEY,
        balance: Amount::nano(1),
        link: (*DEV_GENESIS_ACCOUNT).into(),
        work: node2.work_generate_dev(recv1.hash()),
    }
    .into();

    node2.process_local(send2.clone()).unwrap();

    let send3 = node2
        .wallets
        .send(
            wallet_id2,
            key2.account(),
            *DEV_GENESIS_ACCOUNT,
            Amount::nano(1),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();
    assert_timely2(|| node2.block_confirmed(&send2.hash()));
    assert_timely2(|| node1.block_confirmed(&send2.hash()));
    assert_timely2(|| node2.block_confirmed(&send3.hash()));
    assert_timely2(|| node1.block_confirmed(&send3.hash()));
    assert_timely_eq2(|| node2.ledger.confirmed_count(), 7);
    assert_timely_eq2(|| node1.balance(&DEV_GENESIS_ACCOUNT), Amount::MAX);
}

#[test]
fn block_processor_signatures() {
    let mut system = System::new();
    let node = system.make_node();

    // Insert the genesis key into the wallet for signing operations
    let wallet_id = node.wallets.wallet_ids()[0];
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();

    // Create a valid send block
    let send1 = lattice.genesis().send(&key1, Amount::nano(1000));

    // Create additional send blocks with proper signatures
    let send2 = lattice.genesis().send(&key2, Amount::nano(1000));
    let send3 = lattice.genesis().send(&key3, Amount::nano(1000));

    let mut fork_lattice = lattice.clone();
    // Create a block with an invalid signature (tampered signature bits)
    let mut send4 = lattice.genesis().send(&key3, Amount::nano(1000));
    send4.set_signature(Signature::new());

    // Invalid signature bit (force)
    let mut send5 = fork_lattice.genesis().send(&key3, Amount::nano(2000));
    send5.set_signature(Signature::new());

    // Invalid signature to unchecked
    node.unchecked
        .lock()
        .unwrap()
        .put(send5.previous(), send5.clone(), node.steady_clock.now());

    // Create a valid receive block
    let receive1 = lattice.account(&key1).receive(&send1);
    let receive2 = lattice.account(&key2).receive(&send2);

    // Invalid private key
    let mut receive3 = lattice.account(&key3).receive(&send3);
    receive3.set_signature(Signature::new());

    node.process_active(send1.clone());
    node.process_active(send2.clone());
    node.process_active(send3.clone());
    node.process_active(send4.clone());
    node.process_active(receive1.clone());
    node.process_active(receive2.clone());
    node.process_active(receive3.clone());

    assert_timely(Duration::from_secs(5), || {
        node.block_exists(&receive2.hash())
    });

    assert_timely_eq2(|| node.unchecked.lock().unwrap().len(), 0);

    assert!(node.block(&receive3.hash()).is_none()); // Invalid signer
    assert!(node.block(&send4.hash()).is_none()); // Invalid signature via process_active
    assert!(node.block(&send5.hash()).is_none()); // Invalid signature via unchecked
}

#[test]
fn block_confirm() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();

    let send1 = lattice.genesis().send(&key, Amount::nano(1000));
    let hash1 = send1.hash();

    assert_eq!(
        node1.block_processor_queue.push(BlockContext::new(
            send1.clone().into(),
            BlockSource::Live,
            ChannelId::LOOPBACK
        )),
        true
    );
    assert_eq!(
        node2.block_processor_queue.push(BlockContext::new(
            send1.clone().into(),
            BlockSource::Live,
            ChannelId::LOOPBACK,
        )),
        true
    );

    assert_timely2(|| {
        node1.ledger.any().block_exists(&hash1) && node2.ledger.any().block_exists(&hash1)
    });

    assert!(node1.ledger.any().block_exists(&hash1));
    assert!(node2.ledger.any().block_exists(&hash1));

    // Confirm send1 on node2 so it can vote for send2
    start_election(&node2, &hash1);

    assert_timely2(|| node2.is_active_root(&send1.qualified_root()));

    // Make node2 genesis representative so it can vote
    node2
        .wallets
        .insert_adhoc2(&wallet_id2, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    assert_timely_eq(
        Duration::from_secs(10),
        || node1.recently_cemented.lock().unwrap().len(),
        1,
    );
}

/// Confirm a complex dependency graph. Uses frontiers confirmation which will fail to
/// confirm a frontier optimistically then fallback to pessimistic confirmation.
#[test]
fn dependency_graph_frontier() {
    let mut system = System::new();
    let node1 = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();
    let node2 = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();

    // Send to key1
    let gen_send1 = lattice.genesis().send(&key1, 1);

    // Receive from genesis
    let key1_open = lattice.account(&key1).receive(&gen_send1);
    // Send to genesis
    let key1_send1 = lattice.account(&key1).send(&*DEV_GENESIS_KEY, 1);
    // Receive from key1
    let gen_receive = lattice.genesis().receive(&key1_send1);
    // Send to key2
    let gen_send2 = lattice.genesis().send(&key2, 2);
    // Receive from genesis
    let key2_open = lattice.account(&key2).receive(&gen_send2);
    // Send to key3
    let key2_send1 = lattice.account(&key2).send(&key3, 1);
    // Receive from key2
    let key3_open = lattice.account(&key3).receive(&key2_send1);
    // Send to key1
    let key2_send2 = lattice.account(&key2).send(&key1, 1);
    // Receive from key2
    let key1_receive = lattice.account(&key1).receive(&key2_send2);
    // Send to key3
    let key1_send2 = lattice.account(&key1).send_max(&key3);
    // Receive from key1
    let key3_receive = lattice.account(&key3).receive(&key1_send2);
    // Upgrade key3
    let key3_epoch = lattice.account(&key3).epoch1();

    for node in &system.nodes {
        node.process_multi(&[
            gen_send1.clone(),
            key1_open.clone(),
            key1_send1.clone(),
            gen_receive.clone(),
            gen_send2.clone(),
            key2_open.clone(),
            key2_send1.clone(),
            key3_open.clone(),
            key2_send2.clone(),
            key1_receive.clone(),
            key1_send2.clone(),
            key3_receive.clone(),
            key3_epoch.clone(),
        ]);
    }

    // node1 can vote, but only on the first block
    node1.insert_into_wallet(&DEV_GENESIS_KEY);
    assert_timely(Duration::from_secs(10), || {
        node2.is_active_root(&gen_send1.qualified_root())
    });
    start_election(&node1, &gen_send1.hash());
    assert_timely_eq(
        Duration::from_secs(15),
        || node1.ledger.confirmed_count(),
        node1.ledger.block_count(),
    );
    assert_timely_eq(
        Duration::from_secs(15),
        || node2.ledger.confirmed_count(),
        node2.ledger.block_count(),
    );
}

/// Confirm a complex dependency graph starting from the first block
#[test]
fn dependency_graph() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();

    // Send to key1
    let gen_send1 = lattice.genesis().send(&key1, 1);

    // Receive from genesis
    let key1_open = lattice.account(&key1).receive(&gen_send1);
    // Send to genesis
    let key1_send1 = lattice.account(&key1).send(&*DEV_GENESIS_KEY, 1);
    // Receive from key1
    let gen_receive = lattice.genesis().receive(&key1_send1);
    // Send to key2
    let gen_send2 = lattice.genesis().send(&key2, 2);
    // Receive from genesis
    let key2_open = lattice.account(&key2).receive(&gen_send2);
    // Send to key3
    let key2_send1 = lattice.account(&key2).send(&key3, 1);
    // Receive from key2
    let key3_open = lattice.account(&key3).receive(&key2_send1);
    // Send to key1
    let key2_send2 = lattice.account(&key2).send_max(&key1);
    // Receive from key2
    let key1_receive = lattice.account(&key1).receive(&key2_send2);
    // Send to key3
    let key1_send2 = lattice.account(&key1).send_max(&key3);
    // Receive from key1
    let key3_receive = lattice.account(&key3).receive(&key1_send2);
    // Upgrade key3
    let key3_epoch = lattice.account(&key3).epoch1();

    for node in &system.nodes {
        node.process_multi(&[
            gen_send1.clone(),
            key1_open.clone(),
            key1_send1.clone(),
            gen_receive.clone(),
            gen_send2.clone(),
            key2_open.clone(),
            key2_send1.clone(),
            key3_open.clone(),
            key2_send2.clone(),
            key1_receive.clone(),
            key1_send2.clone(),
            key3_receive.clone(),
            key3_epoch.clone(),
        ]);
    }

    // Hash -> Ancestors
    let dependency_graph: HashMap<BlockHash, Vec<BlockHash>> = [
        (key1_open.hash(), vec![gen_send1.hash()]),
        (key1_send1.hash(), vec![key1_open.hash()]),
        (gen_receive.hash(), vec![gen_send1.hash(), key1_open.hash()]),
        (gen_send2.hash(), vec![gen_receive.hash()]),
        (key2_open.hash(), vec![gen_send2.hash()]),
        (key2_send1.hash(), vec![key2_open.hash()]),
        (key3_open.hash(), vec![key2_send1.hash()]),
        (key2_send2.hash(), vec![key2_send1.hash()]),
        (
            key1_receive.hash(),
            vec![key1_send1.hash(), key2_send2.hash()],
        ),
        (key1_send2.hash(), vec![key1_send1.hash()]),
        (
            key3_receive.hash(),
            vec![key3_open.hash(), key1_send2.hash()],
        ),
        (key3_epoch.hash(), vec![key3_receive.hash()]),
    ]
    .into();
    assert_eq!(node.ledger.block_count() - 2, dependency_graph.len() as u64);

    // Start an election for the first block of the dependency graph, and ensure all blocks are eventually confirmed
    node.insert_into_wallet(&DEV_GENESIS_KEY);
    start_election(&node, &gen_send1.hash());
    assert_timely(Duration::from_secs(30), || {
        // Not many blocks should be active simultaneously
        assert!(node.active.read().unwrap().len() < 6);

        // Ensure that active blocks have their ancestors confirmed
        let error = dependency_graph.iter().any(|entry| {
            if node.is_active_hash(entry.0) {
                for ancestor in entry.1 {
                    if !node.block_confirmed(ancestor) {
                        return true;
                    }
                }
            }
            false
        });
        assert!(!error);
        error || node.ledger.confirmed_count() == node.ledger.block_count()
    });
    assert_eq!(node.ledger.confirmed_count(), node.ledger.block_count());
    assert_timely2(|| node.active.read().unwrap().len() == 0);
}

#[test]
fn fork_keep() {
    let mut system = System::new();
    let node1 = system.make_node();
    let node2 = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    // send1 and send2 fork to different accounts
    let send1 = lattice.genesis().send(&key1, 100);
    let send2 = fork_lattice.genesis().send(&key2, 100);
    node1.process_active(send1.clone());
    node2.process_active(send1.clone());
    assert_timely_eq2(|| node1.active.read().unwrap().len(), 1);
    assert_timely_eq2(|| node2.active.read().unwrap().len(), 1);
    node1.insert_into_wallet(&DEV_GENESIS_KEY);
    // Fill node with forked blocks
    node1.process_active(send2.clone());
    assert_timely2(|| node1.is_active_root(&send2.qualified_root()));
    node2.process_active(send2.clone());
    assert!(node1.block_exists(&send1.hash()));
    assert!(node2.block_exists(&send1.hash()));
    // Wait until the genesis rep makes a vote and confirms send1
    assert_timely2(|| node1.block_confirmed(&send1.hash()));
    assert_timely2(|| node2.block_confirmed(&send1.hash()));
}

#[test]
fn bounded_backlog() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(NodeConfig {
            bounded_backlog: BoundedBacklogConfig {
                max_backlog: 10,
                ..Default::default()
            },
            ..System::default_config()
        })
        .flags(NodeFlags {
            disable_block_processor_republishing: true,
            ..Default::default()
        })
        .finish();
    node.bounded_backlog.as_ref().unwrap().cool_down();

    let howmany_blocks = 8;
    let howmany_chains = 8;
    setup_chains(
        &node,
        howmany_chains,
        howmany_blocks,
        &DEV_GENESIS_KEY,
        false,
    );

    assert_timely_eq2(|| node.ledger.block_count() as usize, 81);

    node.bounded_backlog.as_ref().unwrap().recovered();

    assert_timely(Duration::from_secs(20), || node.ledger.block_count() <= 11);
    // 10 + genesis
}

#[test]
fn backlog_scan_election_activation() {
    let mut system = System::new();
    let node = system.make_node();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(Account::from(1), Amount::nano(1000));

    node.process(send.clone());
    assert_timely2(|| node.is_active_hash(&send.hash()));
    node.active.write().unwrap().cancel(&send.qualified_root());

    std::thread::sleep(Duration::from_secs(1));

    assert_timely2(|| node.is_active_hash(&send.hash()));
}
