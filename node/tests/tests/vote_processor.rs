use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rsnano_ledger::{DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH, DEV_GENESIS_PUB_KEY};
use rsnano_node::consensus::{FilteredVote, ReceivedVote, RepTier};
use rsnano_types::{
    Amount, DEV_GENESIS_KEY, PrivateKey, Signature, Vote, VoteError, VoteSource, VoteTimestamp,
};
use rsnano_utils::stats::{DetailType, Direction, StatType};
use test_helpers::{
    System, assert_always_eq, assert_timely_eq2, assert_timely2, setup_chain, start_election,
};

#[test]
fn codes() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    config.enable_hinted_scheduler = false;
    config.enable_optimistic_scheduler = false;
    config.enable_priority_scheduler = false;
    let node = system.build_node().config(config).finish();
    let blocks = setup_chain(&node, 1, &DEV_GENESIS_KEY, false);

    let vote = Vote::new(
        &DEV_GENESIS_KEY,
        Vote::TIMESTAMP_MIN,
        0,
        vec![blocks[0].hash()],
    );
    let mut vote_invalid = vote.clone();
    vote_invalid.signature = Signature::new();

    let vote: FilteredVote = ReceivedVote::new(Arc::new(vote), VoteSource::Live, None).into();
    let vote_invalid: FilteredVote =
        ReceivedVote::new(Arc::new(vote_invalid), VoteSource::Live, None).into();

    // Invalid signature
    assert_eq!(
        Err(VoteError::Invalid),
        node.vote_processor.vote_blocking(&vote_invalid)
    );

    // No ongoing election (vote goes to vote cache)
    assert_eq!(
        Err(VoteError::Indeterminate),
        node.vote_processor.vote_blocking(&vote)
    );

    assert_timely_eq2(|| node.vote_cache.lock().unwrap().size(), 1);
    // Clear vote cache before starting election
    node.vote_cache.lock().unwrap().clear();

    // First vote from an account for an ongoing election
    start_election(&node, &blocks[0].hash());
    assert_timely2(|| node.is_active_root(&blocks[0].qualified_root()));
    assert_eq!(node.vote_processor.vote_blocking(&vote), Ok(()));

    // Processing the same vote is a replay
    assert_eq!(
        Err(VoteError::Replay),
        node.vote_processor.vote_blocking(&vote)
    );

    // Invalid takes precedence
    assert_eq!(
        Err(VoteError::Invalid),
        node.vote_processor.vote_blocking(&vote_invalid)
    );

    // Once the election is removed (confirmed / dropped) the vote is again indeterminate
    assert!(node.active.erase(&blocks[0].qualified_root()));

    assert_eq!(
        Err(VoteError::Indeterminate),
        node.vote_processor.vote_blocking(&vote)
    );
}

#[test]
fn invalid_signature() {
    let mut system = System::new();
    let node = system.make_node();
    let chain = setup_chain(&node, 1, &DEV_GENESIS_KEY, false);
    let key = PrivateKey::new();
    let vote = Vote::new(&key, Vote::TIMESTAMP_MIN, 0, vec![chain[0].hash()]);
    let mut vote_invalid = vote.clone();
    vote_invalid.signature = Signature::new();

    let vote_invalid = Arc::new(vote_invalid);
    start_election(&node, &chain[0].hash());

    node.vote_processor_queue
        .enqueue(vote_invalid, None, VoteSource::Live, None);

    assert_always_eq(
        Duration::from_millis(500),
        || {
            node.active
                .election_for_block(&chain[0].hash())
                .unwrap()
                .vote_count()
        },
        0,
    );
}

#[test]
fn overflow() {
    let mut system = System::new();
    let node = system.make_node();
    let key = PrivateKey::new();
    let vote = Arc::new(Vote::new(
        &key,
        Vote::TIMESTAMP_MIN,
        0,
        vec![*DEV_GENESIS_HASH],
    ));
    let start_time = Instant::now();
    // No way to lock the processor, but queueing votes in quick succession must result in overflow
    let mut not_processed = 0;
    const TOTAL: usize = 1000;
    for _ in 0..TOTAL {
        if !node
            .vote_processor_queue
            .enqueue(vote.clone(), None, VoteSource::Live, None)
        {
            not_processed += 1;
        }
    }

    assert!(not_processed > 0);
    assert!(not_processed < TOTAL);
    assert_eq!(
        not_processed as u64,
        node.stats
            .count(StatType::VoteProcessor, DetailType::Overfill, Direction::In)
    );

    // check that it did not timeout
    assert!(start_time.elapsed() < Duration::from_secs(10));
}

/**
 * Test that a vote can encode an empty hash set
 */
#[test]
fn empty_hashes() {
    let key = PrivateKey::new();
    let vote = Arc::new(Vote::new(&key, Vote::TIMESTAMP_MIN, 0, vec![]));

    assert_eq!(vote.voter, key.public_key());
    assert_eq!(vote.timestamp(), VoteTimestamp::TIMESTAMP_MIN);
    assert_eq!(vote.hashes.len(), 0);
}

#[test]
fn weights() {
    let mut system = System::new();
    let node0 = system.make_node();
    let node1 = system.make_node();
    let node2 = system.make_node();
    let node3 = system.make_node();

    // Create representatives of different weight levels
    let stake = Amount::MAX;
    let level0 = stake / 5000; // 0.02%
    let level1 = stake / 500; // 0.2%
    let level2 = stake / 50; // 2%

    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();

    let wallet_id0 = node0.wallets.wallet_ids()[0];
    let wallet_id1 = node1.wallets.wallet_ids()[0];
    let wallet_id2 = node2.wallets.wallet_ids()[0];
    let wallet_id3 = node3.wallets.wallet_ids()[0];

    node0.insert_into_wallet(&DEV_GENESIS_KEY);
    node1.insert_into_wallet(&key1);
    node2.insert_into_wallet(&key2);
    node3.insert_into_wallet(&key3);

    node1
        .wallets
        .set_representative(wallet_id1, key1.public_key(), false)
        .wait()
        .unwrap();
    node2
        .wallets
        .set_representative(wallet_id2, key2.public_key(), false)
        .wait()
        .unwrap();
    node3
        .wallets
        .set_representative(wallet_id3, key3.public_key(), false)
        .wait()
        .unwrap();

    node0
        .wallets
        .send(
            wallet_id0,
            *DEV_GENESIS_ACCOUNT,
            key1.account(),
            level0,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    node0
        .wallets
        .send(
            wallet_id0,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            level1,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    node0
        .wallets
        .send(
            wallet_id0,
            *DEV_GENESIS_ACCOUNT,
            key3.account(),
            level2,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    // Wait for representatives
    assert_timely2(|| node0.ledger.rep_weights.len() == 4);
    node0.online_reps.lock().unwrap().set_trended(Amount::MAX);

    // Wait for rep tiers to be updated
    node0.stats.clear();
    assert_timely2(|| {
        node0
            .stats
            .count(StatType::RepTiers, DetailType::Updated, Direction::In)
            >= 2
    });

    assert_timely_eq2(
        || node0.rep_tiers.lock().unwrap().tier(&key1.public_key()),
        RepTier::None,
    );
    assert_timely_eq2(
        || node0.rep_tiers.lock().unwrap().tier(&key2.public_key()),
        RepTier::Tier1,
    );
    assert_timely_eq2(
        || node0.rep_tiers.lock().unwrap().tier(&key3.public_key()),
        RepTier::Tier2,
    );
    assert_timely_eq2(
        || node0.rep_tiers.lock().unwrap().tier(&DEV_GENESIS_PUB_KEY),
        RepTier::Tier3,
    );
}
