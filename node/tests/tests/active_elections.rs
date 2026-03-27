use std::{collections::HashMap, sync::Arc, thread::sleep, time::Duration, usize};

use rsnano_ledger::{
    BlockError, DEV_GENESIS_ACCOUNT, DEV_GENESIS_PUB_KEY, LedgerSet,
    test_helpers::UnsavedBlockLatticeBuilder,
};
use rsnano_node::{
    bootstrap::BootstrapConfig,
    config::{NodeConfig, NodeFlags},
    consensus::{FilteredVote, ReceivedVote},
};
use rsnano_nullable_tcp::get_available_port;
use rsnano_types::{
    Account, Amount, DEV_GENESIS_KEY, PrivateKey, UnixMillisTimestamp, Vote, VoteError, VoteSource,
};
use rsnano_utils::stats::{DetailType, Direction, StatType};
use test_helpers::{
    System, assert_always_eq, assert_never, assert_timely_eq, assert_timely_eq2, assert_timely2,
    process_open_block, process_send_block, setup_independent_blocks, start_election,
    start_elections,
};

/// What this test is doing:
/// Create 20 representatives with minimum principal weight each
/// Create a send block on the genesis account (the last send block)
/// Create 20 forks of the last send block using genesis as representative (no votes produced)
/// Check that only 10 blocks remain in the election (due to max 10 forks per election object limit)
/// Create 20 more forks of the last send block using the new reps as representatives and produce votes for them
///     (9 votes from this batch should survive and replace existing blocks in the election, why not 10?)
/// Then send winning block and it should replace one of the existing blocks
#[test]
fn fork_replacement_tally() {
    let mut system = System::new();
    let node1 = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();

    const REPS_COUNT: usize = 20;

    let keys: Vec<_> = std::iter::repeat_with(|| PrivateKey::new())
        .take(REPS_COUNT)
        .collect();
    let min_pr_weight = node1.online_reps.lock().unwrap().minimum_principal_weight();
    let mut lattice = UnsavedBlockLatticeBuilder::new();

    // Create 20 representatives & confirm blocks
    for i in 0..REPS_COUNT {
        let send = lattice
            .genesis()
            .send(keys[i].public_key(), min_pr_weight + Amount::raw(i as u128));
        let open = lattice.account(&keys[i]).receive(&send);
        node1.process_and_confirm_multi(&[send, open]);
    }

    let key = PrivateKey::new();
    let fork_lattice = lattice.clone();

    let send_last = lattice.genesis().send(&key, Amount::nano(2000));

    // Forks without votes
    for i in 0..REPS_COUNT {
        let mut fork_l = fork_lattice.clone();
        let fork = fork_l
            .genesis()
            .send(&key, Amount::nano(1000) + Amount::raw(i as u128));
        node1.process_active(fork.clone());
    }

    // Check overflow of blocks
    assert_timely2(|| node1.is_active_root(&send_last.qualified_root()));
    assert_timely2(|| {
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .has_max_blocks()
    });

    // Generate forks with votes to prevent new block insertion to election
    for i in 0..REPS_COUNT {
        let mut fork_l = fork_lattice.clone();
        let fork = fork_l.genesis().send(&key, Amount::raw(1 + i as u128));
        let vote = Arc::new(Vote::new(
            &keys[i],
            UnixMillisTimestamp::ZERO,
            0,
            vec![fork.hash()],
        ));
        node1
            .vote_processor_queue
            .enqueue(vote, None, VoteSource::Live, None);
        assert_timely2(|| node1.vote_cache.lock().unwrap().find(&fork.hash()).len() > 0);
        node1.process_active(fork);
    }

    // function to count the number of rep votes (non genesis) found in election
    // it also checks that there are 10 votes in the election
    let count_rep_votes_in_election = || {
        // Check that only max weight blocks remains (and start winner)
        let election = node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap();
        let mut vote_count = 0;
        for i in 0..REPS_COUNT {
            if election.votes().contains_key(&keys[i].public_key()) {
                vote_count += 1;
            }
        }
        vote_count
    };

    // Check overflow of blocks
    // it is only 9, because the intital block of the election does not get replaced
    assert_timely_eq2(|| count_rep_votes_in_election(), 9);

    assert!(
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .has_max_blocks()
    );

    // Process correct block
    let node2 = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();
    node1.network_filter.clear_all();
    node2
        .local_block_broadcaster
        .flood_block_initial(send_last.clone());
    assert_timely2(|| {
        node1
            .stats
            .count(StatType::Message, DetailType::Publish, Direction::In)
            > 0
    });

    assert_timely2(|| {
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .has_max_blocks()
    });

    let blocks1 = node1
        .active
        .election_for_root(&send_last.qualified_root())
        .unwrap()
        .candidate_blocks()
        .clone();
    assert!(!blocks1.contains_key(&send_last.hash()));

    // Process vote for correct block & replace existing lowest tally block
    let vote = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        UnixMillisTimestamp::ZERO,
        0,
        vec![send_last.hash()],
    ));
    node1
        .vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);
    // ensure vote arrives before the block
    assert_timely_eq2(
        || {
            node1
                .vote_cache
                .lock()
                .unwrap()
                .find(&send_last.hash())
                .len()
        },
        1,
    );
    node1.network_filter.clear_all();
    node2
        .local_block_broadcaster
        .flood_block_initial(send_last.clone());
    assert_timely2(|| {
        node1
            .stats
            .count(StatType::Message, DetailType::Publish, Direction::In)
            > 1
    });

    // the send_last block should replace one of the existing block of the election because it has higher vote weight
    let find_send_last_block = || {
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .contains_block(&send_last.hash())
    };
    assert_timely2(|| find_send_last_block());
    assert!(
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .has_max_blocks()
    );

    assert_timely2(|| {
        node1
            .active
            .election_for_root(&send_last.qualified_root())
            .unwrap()
            .votes()
            .contains_key(&DEV_GENESIS_PUB_KEY)
    });
}

#[test]
fn inactive_votes_cache_basic() {
    let mut system = System::new();
    let node = system.make_node();
    let key = PrivateKey::new();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(&key, Amount::raw(100));
    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send.hash()]));
    node.vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);
    assert_timely_eq2(|| node.vote_cache.lock().unwrap().size(), 1);
    node.process_active(send.clone());
    assert_timely2(|| node.block_confirmed(&send.hash()));
    assert_timely_eq2(|| node.get_stat("election_vote", "cache", Direction::In), 1);
}

// This test case confirms that a non final vote cannot cause an election to become confirmed
#[test]
#[ignore = "TODO: Fix election update_tallies"]
fn non_final() {
    let mut system = System::new();
    let node = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(Account::from(42), 100);

    // Non-final vote
    let vote = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        UnixMillisTimestamp::ZERO,
        0,
        vec![send.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);
    assert_timely_eq(
        Duration::from_secs(5),
        || node.vote_cache.lock().unwrap().size(),
        1,
    );

    node.process_active(send.clone());

    assert_timely2(|| {
        node.active
            .election_for_root(&send.qualified_root())
            .is_some()
    });

    assert_timely_eq2(|| node.get_stat("election_vote", "cache", Direction::In), 1);

    let _quorum_delta = node.online_reps.lock().unwrap().quorum_delta();
    assert_timely_eq2(
        || {
            let election = node
                .active
                .election_for_root(&send.qualified_root())
                .unwrap();
            //election.update_tallies(&node.ledger.rep_weights.read(), quorum_delta);
            election.tallies().winner().unwrap().1
        },
        Amount::MAX - Amount::raw(100),
    );
    assert_eq!(
        node.active
            .election_for_root(&send.qualified_root())
            .unwrap()
            .is_confirmed(),
        false
    );
}

#[test]
fn inactive_votes_cache_fork() {
    let mut system = System::new();
    let node = system.make_node();
    let mut lattice1 = UnsavedBlockLatticeBuilder::new();
    let mut lattice2 = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();

    let send1 = lattice1.genesis().send(&key, 100);
    let send2 = lattice2.genesis().send(&key, 200);

    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send1.hash()]));
    node.vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);

    assert_timely_eq(
        Duration::from_secs(5),
        || node.vote_cache.lock().unwrap().size(),
        1,
    );

    node.process_active(send2.clone());

    assert_timely2(|| node.is_active_root(&send1.qualified_root()));

    node.process_active(send1.clone());

    assert_timely_eq2(|| node.block_confirmed(&send1.hash()), true);
    assert_timely_eq2(|| node.get_stat("election_vote", "cache", Direction::In), 1)
}

#[test]
fn inactive_votes_cache_existing_vote() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();
    let rep_weight = Amount::nano(100_000);

    let send = lattice.genesis().send(&key, rep_weight);
    let open = lattice.account(&key).receive(&send);

    node.process(send.clone());
    node.process(open.clone());

    assert_timely2(|| node.is_active_hash(&send.hash()));
    assert!(
        node.ledger.weight(&key.public_key())
            > node.online_reps.lock().unwrap().minimum_principal_weight()
    );

    // Insert vote
    let vote1 = Arc::new(Vote::new(
        &key,
        UnixMillisTimestamp::ZERO,
        0,
        vec![send.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote1.clone(), None, VoteSource::Live, None);

    assert_timely_eq2(
        || {
            node.active
                .election_for_block(&send.hash())
                .unwrap()
                .vote_count()
        },
        1,
    );

    assert_timely_eq2(|| node.get_stat("election", "vote", Direction::In), 1);

    let last_vote1 = node
        .active
        .election_for_block(&send.hash())
        .unwrap()
        .votes()
        .get(&key.public_key())
        .unwrap()
        .clone();

    assert_eq!(send.hash(), last_vote1.hash);

    // Attempt to change vote with inactive_votes_cache
    node.vote_cache
        .lock()
        .unwrap()
        .insert(&vote1, rep_weight, &HashMap::new());

    let cached = node.vote_cache.lock().unwrap().find(&send.hash());
    assert_eq!(cached.len(), 1);
    let _ = node
        .vote_processor
        .vote_blocking(&ReceivedVote::new(cached[0].clone(), VoteSource::Live, None).into());

    // Check that election data is not changed
    let election = node.active.election_for_block(&send.hash()).unwrap();
    assert_eq!(election.vote_count(), 1);
    let last_vote2 = election.votes().get(&key.public_key()).unwrap().clone();
    assert_eq!(send.hash(), last_vote2.hash);
    assert_eq!(0, node.get_stat("election_vote", "cache", Direction::In));
}

#[test]
fn inactive_votes_cache_multiple_votes() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();

    let send1 = lattice.genesis().send(&key, Amount::nano(100_000));
    let send2 = lattice.genesis().send(&key, Amount::nano(100_000));
    let open = lattice.account(&key).receive(&send1);

    // put the blocks in the ledger witout triggering an election
    node.process(send1.clone());
    node.process(send2.clone());
    node.process(open.clone());

    assert_timely2(|| node.is_active_hash(&send1.hash()));
    node.active.cancel(&send1.qualified_root());
    assert_timely2(|| !node.is_active_hash(&send1.hash()));

    // Process votes
    let vote1 = Arc::new(Vote::new(
        &key,
        UnixMillisTimestamp::ZERO,
        0,
        vec![send1.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote1, None, VoteSource::Live, None);

    let vote2 = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        UnixMillisTimestamp::ZERO,
        0,
        vec![send1.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote2, None, VoteSource::Live, None);

    assert_timely_eq2(
        || node.vote_cache.lock().unwrap().find(&send1.hash()).len(),
        2,
    );
    assert_eq!(1, node.vote_cache.lock().unwrap().size());
    start_election(&node, &send1.hash());
    assert_timely_eq2(
        || {
            node.active
                .election_for_block(&send1.hash())
                .unwrap()
                .vote_count()
        },
        2,
    );
    assert_timely_eq2(|| node.get_stat("election_vote", "cache", Direction::In), 2);
}

#[test]
fn inactive_votes_cache_election_start() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    config.enable_optimistic_scheduler = false;
    config.enable_priority_scheduler = false;
    let node = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();

    // Enough weight to trigger election hinting but not enough to confirm block on its own
    let amount = ((node.online_reps.lock().unwrap().trended_or_minimum_weight() / 100)
        * node.config.hinted_scheduler.hinting_threshold_percent as u128)
        / 2
        + Amount::nano(1_000_000);

    let send1 = lattice.genesis().send(&key1, amount);
    let send2 = lattice.genesis().send(&key2, amount);
    let open1 = lattice.account(&key1).receive(&send1);
    let open2 = lattice.account(&key2).receive(&send2);

    node.process(send1.clone());
    let send2 = node.process(send2.clone());
    node.process(open1.clone());
    node.process(open2.clone());

    // These blocks will be processed later
    let send3 = lattice.genesis().send(Account::from(2), 1);
    let send4 = lattice.genesis().send(Account::from(3), 1);

    // Inactive votes
    let vote1 = Arc::new(Vote::new(
        &key1,
        UnixMillisTimestamp::ZERO,
        0,
        vec![open1.hash(), open2.hash(), send4.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote1, None, VoteSource::Live, None);
    assert_timely_eq2(|| node.vote_cache.lock().unwrap().size(), 3);
    assert_eq!(node.active.len(), 0);
    assert_eq!(1, node.ledger.confirmed_count());

    // 2 votes are required to start election (dev network)
    let vote2 = Arc::new(Vote::new(
        &key2,
        UnixMillisTimestamp::ZERO,
        0,
        vec![open1.hash(), open2.hash(), send4.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote2, None, VoteSource::Live, None);
    // Only election for send1 should start, other blocks are missing dependencies and don't have enough final weight
    assert_timely_eq2(|| node.active.len(), 1);
    assert!(node.is_active_hash(&send1.hash()));

    // Confirm elections with weight quorum
    let vote0 = Arc::new(Vote::new_final(
        &DEV_GENESIS_KEY,
        vec![open1.hash(), open2.hash(), send4.hash()],
    ));
    node.vote_processor_queue
        .enqueue(vote0, None, VoteSource::Live, None);
    assert_timely_eq2(|| node.active.len(), 0);
    assert_timely_eq2(|| node.ledger.confirmed_count(), 5);
    // Confirmation on disk may lag behind cemented_count cache
    assert_timely2(|| {
        node.block_hashes_confirmed(&[send1.hash(), send2.hash(), open1.hash(), open2.hash()])
    });

    // A late block arrival also checks the inactive votes cache
    assert_eq!(node.active.len(), 0);
    let send4_cache = node.vote_cache.lock().unwrap().find(&send4.hash());
    assert_eq!(3, send4_cache.len());
    node.process_active(send3.clone());
    // An election is started for send6 but does not
    assert_eq!(node.ledger.confirmed().block_exists(&send3.hash()), false);
    assert_eq!(node.confirming_set.contains(&send3.hash()), false);
    // send7 cannot be voted on but an election should be started from inactive votes
    node.process_active(send4);
    assert_timely_eq2(|| node.ledger.confirmed_count(), 7);
}

#[test]
fn republish_winner() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    let node1 = system.build_node().config(config.clone()).finish();
    config.network.listening_port = get_available_port();
    let node2 = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();

    let key = PrivateKey::new();
    let send1 = lattice.genesis().send(&key, Amount::nano(1000));

    node1.process_active(send1.clone());
    assert_timely2(|| node1.block_exists(&send1.hash()));

    assert_timely_eq2(
        || {
            node2
                .stats
                .count(StatType::Message, DetailType::Publish, Direction::In)
        },
        1,
    );

    // Several forks
    for i in 0..5 {
        let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
        let fork = fork_lattice.genesis().send(&key, Amount::raw(1 + i));
        node1.process_active(fork.clone());
        assert_timely2(|| node1.is_active_root(&fork.qualified_root()));
    }

    assert_timely2(|| node1.active.len() > 0);
    assert_eq!(
        1,
        node2
            .stats
            .count(StatType::Message, DetailType::Publish, Direction::In)
    );

    // Process new fork with vote to change winner
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let fork = fork_lattice.genesis().send(&key, Amount::nano(2000));
    node1.process_active(fork.clone());
    assert_timely2(|| node1.is_active_hash(&fork.hash()));

    let vote = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![fork.hash()]));

    node1
        .vote_processor_queue
        .enqueue(vote, None, VoteSource::Live, None);

    assert_timely2(|| node2.block_confirmed(&fork.hash()));
}

/*
 * Tests that an election can be confirmed as the result of a confirmation request
 *
 * Set-up:
 * - node1 with:
 * 		- enabled frontiers_confirmation (default) -> allows it to confirm blocks and subsequently generates votes
 * - node2 with:
 * 		- disabled rep crawler -> this inhibits node2 from learning that node1 is a rep
 */
#[test]
fn confirm_election_by_request() {
    let mut system = System::new();
    let node1 = system
        .build_node()
        .config(NodeConfig {
            // Disable vote rebroadcasting to prevent node1 from actively sending votes to node2
            enable_vote_rebroadcast: false,
            ..System::default_config()
        })
        .finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();

    let send1 = lattice.genesis().send(Account::from(1), 100);

    // Process send1 locally on node1
    node1.process(send1.clone());

    // Add rep key to node1
    let wallet_id = node1.wallets.wallet_ids()[0];
    node1
        .wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    // Ensure election on node1 is already confirmed before connecting with node2
    assert_timely2(|| node1.block_confirmed(&send1.hash()));

    // Wait for the election to be removed and give time for any in-flight vote broadcasts to settle
    assert_timely2(|| node1.active.len() == 0);
    sleep(Duration::from_secs(1));

    // At this point node1 should not generate votes for send1 block unless it receives a request

    // Create a second node
    let flags = NodeFlags {
        disable_rep_crawler: true,
        ..Default::default()
    };
    let node2 = system.build_node().flags(flags).finish();

    // Process send1 block as live block on node2, this should start an election
    node2.process_active(send1.clone());

    // Ensure election is started on node2
    assert_timely2(|| node2.is_active_root(&send1.qualified_root()));

    // Ensure election on node2 did not get confirmed without us requesting votes
    sleep(Duration::from_secs(1));

    assert_eq!(
        node2
            .active
            .election_for_root(&send1.qualified_root())
            .unwrap()
            .is_confirmed(),
        false
    );

    // Get random peer list from node2 -- so basically just node2
    let peers = node2.network.read().unwrap().sorted_channels();
    assert_eq!(peers.is_empty(), false);

    // Add representative (node1) to disabled rep crawler of node2
    node2.online_reps.lock().unwrap().vote_observed_directly(
        *DEV_GENESIS_PUB_KEY,
        peers[0].clone(),
        node2.steady_clock.now(),
    );

    // Expect a vote to come back
    // There needs to be at least one request to get the election confirmed,
    // Rep has this block already confirmed so should reply with final vote only

    // Expect election was confirmed
    assert_timely2(|| node1.block_confirmed(&send1.hash()));
    assert_timely2(|| node2.block_confirmed(&send1.hash()));
}

#[test]
fn confirm_frontier() {
    let mut system = System::new();
    let mut lattice = UnsavedBlockLatticeBuilder::new();

    // send 100 raw from genesis to a random account
    let send = lattice.genesis().send(Account::from(1), 100);

    // Voting node
    let node1 = system
        .build_node()
        .flags(NodeFlags {
            disable_request_loop: true,
            disable_ongoing_bootstrap: true,
            ..Default::default()
        })
        .config(NodeConfig {
            bootstrap: BootstrapConfig {
                enable: false,
                ..Default::default()
            },
            ..System::default_config()
        })
        .finish();

    node1.process(send.clone());
    node1.confirm(send.hash());

    // The rep crawler would otherwise request confirmations in order to find representatives
    // start node2 later so that we do not get the gossip traffic
    let node2 = system
        .build_node()
        .flags(NodeFlags {
            disable_ongoing_bootstrap: true,
            disable_rep_crawler: true,
            ..Default::default()
        })
        .config(NodeConfig {
            bootstrap: BootstrapConfig {
                enable: false,
                ..Default::default()
            },
            ..System::default_config()
        })
        .finish();

    // Add representative to disabled rep crawler
    let peers = node2.network.read().unwrap().sorted_channels();
    assert!(!peers.is_empty());
    node2.online_reps.lock().unwrap().vote_observed_directly(
        *DEV_GENESIS_PUB_KEY,
        peers[0].clone(),
        node2.steady_clock.now(),
    );

    node2.process(send.clone());
    assert_timely2(|| node2.active.len() > 0);

    node1.insert_into_wallet(&DEV_GENESIS_KEY);

    // Save election to check request count afterwards
    assert_timely2(|| node2.is_active_root(&send.qualified_root()));

    assert_timely2(|| node2.block_confirmed(&send.hash()));
    assert_timely_eq2(|| node2.ledger.confirmed_count(), 2);
    assert_timely_eq2(|| node2.active.len(), 0);
}

/// Ensures that election winners set won't grow without bounds when cementing
/// is slower that the rate of confirming new elections
#[test]
fn bound_election_winners() {
    let mut system = System::new();
    let mut config = System::default_config();
    // Set election winner limit to a low value
    config.confirming_set.max_blocks = 5;
    let node = system.build_node().config(config).finish();

    // Start elections for a couple of blocks, number of elections is larger than the election winner set limit
    let blocks = setup_independent_blocks(&node, 10, &DEV_GENESIS_KEY);
    assert_timely2(|| {
        blocks
            .iter()
            .all(|block| node.is_active_root(&block.qualified_root()))
    });

    {
        // Prevent cementing of confirmed blocks
        let txn = node.ledger.store.begin_write();

        // Ensure that when the number of election winners reaches the limit, AEC vacancy reflects that
        // Confirming more elections should make the vacancy negative
        assert!(node.active.vacancy() > 0);

        for block in blocks {
            node.force_confirm(&block.hash());
        }

        assert_timely2(|| node.active.vacancy() <= 0);
        // Release the guard to allow cementing, there should be some vacancy now
        txn.commit();
    }

    assert_timely2(|| node.active.vacancy() > 0);
}

/// Blocks should only be broadcasted when they are active in the AEC
#[test]
fn broadcast_block_on_activation() {
    let mut system = System::new();
    let mut config1 = System::default_config();
    // Deactivates elections on both nodes.
    config1.active_elections.max_elections = 0;
    config1.bootstrap.enable = false;

    let mut config2 = System::default_config();
    config2.active_elections.max_elections = 0;
    config2.bootstrap.enable = false;

    let node1 = system.build_node().config(config1).finish();
    let node2 = system.build_node().config(config2).finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice.genesis().send(*DEV_GENESIS_ACCOUNT, 1000);
    // Adds a block to the first node
    node1.local_block_broadcaster.stop();
    let send1 = node1.process(send1.clone());

    // The second node should not have the block
    assert_never(Duration::from_millis(500), || {
        node2.block_exists(&send1.hash())
    });

    // Activating the election should broadcast the block
    node1.election_schedulers.add_manual(send1.clone());
    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));
    assert_timely2(|| node2.block_exists(&send1.hash()));
}

// Tests that blocks are correctly cleared from the duplicate filter for unconfirmed elections
#[test]
fn dropped_cleanup() {
    let mut system = System::new();
    let flags = NodeFlags {
        disable_request_loop: true,
        ..Default::default()
    };
    let node = system
        .build_node()
        .config(NodeConfig {
            enable_priority_scheduler: false,
            enable_hinted_scheduler: false,
            enable_optimistic_scheduler: false,
            ..System::default_config_without_backlog_scan()
        })
        .flags(flags)
        .finish();
    let chain = setup_independent_blocks(&node, 1, &DEV_GENESIS_KEY);
    let hash = chain[0].hash();
    let qual_root = chain[0].qualified_root();

    // Add to network filter to ensure proper cleanup after the election is dropped
    let mut block_bytes = Vec::new();
    chain[0].serialize(&mut block_bytes).unwrap();
    assert!(!node.network_filter.apply(&block_bytes).1);
    assert!(node.network_filter.apply(&block_bytes).1);

    start_election(&node, &hash);

    // Not yet removed
    assert!(node.network_filter.apply(&block_bytes).1);
    assert!(node.is_active_root(&qual_root));

    // Now simulate dropping the election
    node.active.erase(&qual_root);
    // An election was recently dropped
    assert_timely_eq2(
        || node.get_stat("active_elections_dropped", "manual", Direction::In),
        1,
    );

    // The filter must have been cleared
    assert!(node.network_filter.apply(&block_bytes).1);

    // Repeat test for a confirmed election
    assert!(node.network_filter.apply(&block_bytes).1);

    start_election(&node, &hash);
    node.force_confirm(&hash);
    assert_timely2(|| node.ledger.confirmed().block_exists(&hash));
    node.active.erase(&qual_root);

    // The filter should not have been cleared
    assert!(node.network_filter.apply(&block_bytes).1);

    // Not dropped
    assert_timely_eq2(
        || node.get_stat("active_elections_dropped", "manual", Direction::In),
        1,
    );

    // Block cleared from active
    assert_eq!(node.is_active_root(&qual_root), false);
}

#[test]
fn confirmation_consistency() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();
    let wallet_id = node.wallets.wallet_ids()[0];
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    for _ in 0..10 {
        let block = node
            .wallets
            .send(
                wallet_id,
                *DEV_GENESIS_ACCOUNT,
                Account::from(0),
                node.config.receive_minimum,
                0.into(),
                true,
                None,
            )
            .wait()
            .unwrap();

        assert_timely2(|| node.block_confirmed(&block.hash()));
        assert_timely2(|| node.active.was_recently_confirmed(&block.hash()));
    }
}

#[test]
fn fork_filter_cleanup() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    let node1 = system.build_node().config(config.clone()).finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();
    let send1 = lattice.genesis().send(&key, 1);
    let mut send_block_bytes = Vec::new();
    send1.serialize(&mut send_block_bytes).unwrap();

    // Generate 10 forks to prevent new block insertion to election
    for i in 0..10 {
        let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
        let fork = fork_lattice.genesis().send(&key, Amount::raw(1 + i));
        node1.process_active(fork.clone());
        assert_timely2(|| node1.is_active_root(&fork.qualified_root()));
    }

    // All forks were merged into the same election
    assert_timely2(|| node1.is_active_root(&send1.qualified_root()));
    assert_timely_eq2(
        || {
            node1
                .active
                .election_for_root(&send1.qualified_root())
                .unwrap()
                .block_count()
        },
        10,
    );
    assert_eq!(1, node1.active.len());

    // Instantiate a new node
    config.network.listening_port = get_available_port();
    let node2 = system.build_node().config(config).finish();

    // Process the first initial block on node2
    node2.process_active(send1.clone());
    assert_timely2(|| node2.is_active_root(&send1.qualified_root()));

    // TODO: questions: why doesn't node2 pick up "fork" from node1? because it connected to node1 after node1
    //                  already process_active()d the fork? shouldn't it broadcast it anyway, even later?
    //
    //                  how about node1 picking up "send1" from node2? we know it does because we assert at
    //                  the end that it is within node1's AEC, but why node1.block_count doesn't increase?
    //
    assert_timely_eq2(|| node2.ledger.block_count(), 2);
    assert_timely_eq2(|| node1.ledger.block_count(), 2);

    // Block is erased from the duplicate filter
    assert_timely2(|| !node1.network_filter.apply(&send_block_bytes).1);
}

// Ensures votes are tallied on election::publish even if no vote is inserted through inactive_votes_cache
#[test]
fn conflicting_block_vote_existing_election() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let flags = NodeFlags {
        disable_request_loop: true,
        ..Default::default()
    };
    let node = system.build_node().config(config).flags(flags).finish();

    let key = PrivateKey::new();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(&key, 100);

    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let fork = fork_lattice.genesis().send(&key, 200);

    let vote_fork = Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![fork.hash()]));

    node.process_local(send.clone()).unwrap();
    assert_timely_eq2(|| node.active.len(), 1);

    // Vote for conflicting block, but the block does not yet exist in the ledger
    node.vote_processor_queue
        .enqueue(vote_fork, None, VoteSource::Live, None);

    // Block now gets processed
    assert_eq!(node.process_local(fork.clone()), Err(BlockError::Fork));

    // Election must be confirmed
    assert_timely2(|| node.ledger.confirmed().block_exists(&fork.hash()));
}

#[test]
fn activate_account_chain() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();

    let key = PrivateKey::new();
    let send = lattice.genesis().send(*DEV_GENESIS_ACCOUNT, 1);
    let send2 = lattice.genesis().send(&key, 1);
    let send3 = lattice.genesis().send(&key, 1);
    let open = lattice.account(&key).receive(&send2);
    let receive = lattice.account(&key).receive(&send3);

    node.process_local(send.clone()).unwrap();
    node.process_local(send2.clone()).unwrap();
    node.process_local(send3.clone()).unwrap();
    node.process_local(open.clone()).unwrap();
    node.process_local(receive.clone()).unwrap();

    start_election(&node, &send.hash());
    assert_eq!(1, node.active.len());
    node.force_confirm(&send.hash());
    assert_timely2(|| node.block_confirmed(&send.hash()));

    // On cementing, the next election is started
    assert_timely2(|| node.is_active_root(&send2.qualified_root()));
    node.force_confirm(&send2.hash());
    assert_timely2(|| node.block_confirmed(&send2.hash()));

    // On cementing, the next election is started
    assert_timely2(|| node.is_active_root(&open.qualified_root())); // Destination account activated
    assert_timely2(|| node.is_active_root(&send3.qualified_root())); // Block successor activated
    node.force_confirm(&open.hash());
    assert_timely2(|| node.block_confirmed(&open.hash()));

    // Until send3 is also confirmed, the receive block should not activate
    sleep(Duration::from_millis(200));
    assert!(!node.is_active_root(&receive.qualified_root()));
    node.force_confirm(&send3.hash());
    assert_timely2(|| node.block_confirmed(&send3.hash()));
    assert_timely2(|| node.is_active_root(&receive.qualified_root())); // Destination account activated
}

#[test]
fn list_active() {
    let mut system = System::new();
    let node = system.make_node();

    let key = PrivateKey::new();

    let send = process_send_block(node.clone(), *DEV_GENESIS_ACCOUNT, Amount::raw(1));

    let send2 = process_send_block(node.clone(), key.account(), Amount::raw(1));

    let open = process_open_block(node.clone(), key);

    start_elections(&node, &[send.hash(), send2.hash(), open.hash()], false);
    assert_timely_eq2(|| node.active.len(), 3);

    assert_eq!(node.active.len(), 3);
}

#[test]
fn vote_replays() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(NodeConfig {
            enable_voting: false,
            ..System::default_config_without_backlog_scan()
        })
        .finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();

    // send 1000 nano from genesis to key
    let send1 = lattice.genesis().send(&key, Amount::nano(1000));

    // create open block for key receing 1000 nano
    let open1 = lattice.account(&key).receive(&send1);

    // wait for elections objects to appear in the AEC
    node.process(send1.clone());
    node.process(open1.clone());
    start_elections(&node, &[send1.hash(), open1.hash()], false);
    assert_eq!(node.active.len(), 2);

    // First vote is not a replay and confirms the election, second vote should be a replay since the election has confirmed but not yet removed
    let vote_send1: FilteredVote = ReceivedVote::new(
        Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send1.hash()])),
        VoteSource::Live,
        None,
    )
    .into();

    node.vote_processor.vote_blocking(&vote_send1).unwrap();
    let res = node.vote_processor.vote_blocking(&vote_send1);
    assert!(matches!(res, Err(VoteError::Replay) | Err(VoteError::Late)));

    // Wait until the election is removed, at which point the vote is considered late since it's been recently confirmed
    assert_timely_eq2(|| node.active.len(), 1);
    let res = node.vote_processor.vote_blocking(&vote_send1);
    assert_eq!(res, Err(VoteError::Late));

    // Open new account
    let vote_open1: FilteredVote = ReceivedVote::new(
        Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![open1.hash()])),
        VoteSource::Live,
        None,
    )
    .into();
    node.vote_processor.vote_blocking(&vote_open1).unwrap();
    let res = node.vote_processor.vote_blocking(&vote_open1);
    assert!(matches!(res, Err(VoteError::Replay) | Err(VoteError::Late)));

    assert_timely_eq2(|| node.active.len(), 0);

    assert_eq!(
        node.vote_processor.vote_blocking(&vote_open1),
        Err(VoteError::Late)
    );
    assert_eq!(node.ledger.weight(&key.public_key()), Amount::nano(1000));

    // send 1 raw from key to key
    let send2 = lattice.account(&key).send(&key, 1);
    node.process(send2.clone());
    start_elections(&node, &[send2.hash()], false);
    assert_eq!(node.active.len(), 1);

    // vote2_send2 is a non final vote with little weight, vote1_send2 is the vote that confirms the election
    let vote1_send2: FilteredVote = ReceivedVote::new(
        Arc::new(Vote::new_final(&DEV_GENESIS_KEY, vec![send2.hash()])),
        VoteSource::Live,
        None,
    )
    .into();

    let vote2_send2: FilteredVote = ReceivedVote::new(
        Arc::new(Vote::new(
            &DEV_GENESIS_KEY,
            UnixMillisTimestamp::ZERO,
            0,
            vec![send2.hash()],
        )),
        VoteSource::Live,
        None,
    )
    .into();

    // this vote cannot confirm the election
    node.vote_processor.vote_blocking(&vote2_send2).unwrap();
    assert_eq!(node.active.len(), 1);

    // this vote confirms the election
    node.vote_processor.vote_blocking(&vote1_send2).unwrap();

    // This should still return replay or late, either because the election is still in the AEC or because it is recently confirmed
    let res = node.vote_processor.vote_blocking(&vote1_send2);
    assert!(matches!(res, Err(VoteError::Replay) | Err(VoteError::Late)));
    assert_timely_eq2(|| node.active.len(), 0);

    assert_eq!(
        node.vote_processor.vote_blocking(&vote1_send2),
        Err(VoteError::Late)
    );
    assert_eq!(
        node.vote_processor.vote_blocking(&vote2_send2),
        Err(VoteError::Late)
    );

    assert_timely_eq2(|| node.active.len(), 0);

    // Removing blocks as recently confirmed makes every vote indeterminate
    node.active.clear_recently_confirmed();

    assert_eq!(
        node.vote_processor.vote_blocking(&vote_send1),
        Err(VoteError::Indeterminate)
    );
    assert_eq!(
        node.vote_processor.vote_blocking(&vote_open1),
        Err(VoteError::Indeterminate)
    );
    assert_eq!(
        node.vote_processor.vote_blocking(&vote1_send2),
        Err(VoteError::Indeterminate)
    );
    assert_eq!(
        node.vote_processor.vote_blocking(&vote2_send2),
        Err(VoteError::Indeterminate)
    );
}

#[test]
fn confirm_new() {
    let mut system = System::new();
    let node1 = system.make_node();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(Account::from(1), 100);
    node1.process_active(send.clone());
    assert_timely_eq2(|| node1.active.len(), 1);
    let node2 = system.make_node();
    // Add key to node2
    node2.insert_into_wallet(&DEV_GENESIS_KEY);
    // Let node2 know about the block
    assert_timely2(|| node2.block_exists(&send.hash()));
    // Wait confirmation
    assert_timely_eq2(|| node1.ledger.confirmed_count(), 2);
    assert_timely_eq2(|| node2.ledger.confirmed_count(), 2);
}

#[test]
#[ignore = "TODO"]
/*
 * Ensures we limit the number of vote hinted elections in AEC
 */
fn limit_vote_hinted_elections() {
    // disabled because it doesn't run after tokio switch
    // TODO reimplement in Rust
}

#[test]
fn active_inactive() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::from(123);

    let send = lattice.genesis().send(&key, 1);
    let send2 = lattice.genesis().send(Account::from(1), 1);
    let open = lattice.account(&key).receive(&send);
    node.process_multi(&[send.clone(), send2.clone(), open]);
    assert_timely2(|| node.active.is_active_hash(&send.hash()));
    node.active.cancel(&send.qualified_root());
    start_election(&node, &send2.hash());
    node.force_confirm(&send2.hash());

    assert_timely2(|| !node.confirming_set.contains(&send2.hash()));
    assert_timely2(|| node.block_confirmed(&send2.hash()));
    assert_timely2(|| node.block_confirmed(&send.hash()));

    assert_always_eq(
        Duration::from_millis(50),
        || {
            node.stats()
                .get("confirmation_observer", "active_conf_height")
        },
        0,
    );
}

#[test]
fn activate_inactive() {
    let mut system = System::new();
    let node = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key = PrivateKey::new();
    let send = lattice.genesis().send(&key, 1);
    let send2 = lattice.genesis().send(Account::from(1), 1);
    let open = lattice.account(&key).receive(&send);

    node.process_multi(&[send.clone(), send2.clone(), open.clone()]);

    assert_timely2(|| node.is_active_hash(&send.hash()));
    node.active.cancel(&send.qualified_root());
    assert_timely2(|| !node.is_active_hash(&send.hash()));

    start_elections(&node, &[send2.hash()], true);

    assert_timely2(|| !node.confirming_set.contains(&send2.hash()));
    assert_timely2(|| node.block_confirmed(&send2.hash()));
    assert_timely2(|| node.block_confirmed(&send.hash()));

    assert_timely_eq2(|| node.stats().get("confirmation_observer", "inactive"), 1);
    assert_timely_eq2(
        || node.stats().get("confirmation_observer", "active_quorum"),
        1,
    );
    assert_always_eq(
        Duration::from_millis(50),
        || {
            node.stats()
                .get("confirmation_observer", "active_conf_height")
        },
        0,
    );

    // Cementing of send should activate open
    assert_timely2(|| node.is_active_root(&open.qualified_root()))
}
