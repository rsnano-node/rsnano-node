use std::{sync::Arc, time::Duration};

use rsnano_ledger::{
    DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH, DEV_GENESIS_PUB_KEY,
    test_helpers::UnsavedBlockLatticeBuilder,
};
use rsnano_node::{
    config::NodeFlags,
    consensus::{ReceivedVote, election::VoteType},
};
use rsnano_types::{
    Amount, DEV_GENESIS_KEY, Epoch, PrivateKey, Signature, Vote, VoteError, VoteSource, WalletId,
};
use rsnano_utils::stats::{DetailType, Direction, StatType};
use test_helpers::{
    System, assert_timely, assert_timely_eq2, assert_timely2, make_fake_channel, upgrade_epoch,
};

#[test]
fn check_signature() {
    let mut system = System::new();
    let mut config = System::default_config();
    config.online_weight_minimum = Amount::MAX;
    let node = system.build_node().config(config).finish();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().send(&key1, 100);
    node.process(send1.clone());
    assert_timely2(|| node.is_active_hash(&send1.hash()));
    let channel = make_fake_channel(&node);
    let mut vote1 = Vote::new(&DEV_GENESIS_KEY, Vote::TIMESTAMP_MIN, 0, vec![send1.hash()]);
    let good_signature = vote1.signature;
    vote1.signature = Signature::new();
    let received_vote1 = ReceivedVote::new(
        Arc::new(vote1.clone()),
        VoteSource::Live,
        Some(channel.clone()),
    );
    assert_eq!(
        Err(VoteError::Invalid),
        node.vote_processor.vote_blocking(&received_vote1.into())
    );

    vote1.signature = good_signature;

    let received_vote2 =
        ReceivedVote::new(Arc::new(vote1), VoteSource::Live, Some(channel.clone()));
    assert!(
        node.vote_processor
            .vote_blocking(&received_vote2.clone().into())
            .is_ok()
    );
    assert_eq!(
        Err(VoteError::Replay),
        node.vote_processor.vote_blocking(&received_vote2.into())
    );
}

// The voting cooldown is respected
#[test]
fn add_cooldown() {
    let mut system = System::new();
    let node = system.make_node();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().send_max(&key1);
    node.process(send1.clone());
    assert_timely2(|| node.is_active_root(&send1.qualified_root()));
    let vote1 = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        Vote::TIMESTAMP_MIN * 1,
        0,
        vec![send1.hash()],
    ));
    let channel = make_fake_channel(&node);
    let _ = node
        .vote_processor
        .vote_blocking(&ReceivedVote::new(vote1, VoteSource::Live, Some(channel.clone())).into());

    let key2 = PrivateKey::new();
    let send2 = fork_lattice.genesis().send_max(&key2);
    let vote2 = Arc::new(Vote::new(
        &DEV_GENESIS_KEY,
        Vote::TIMESTAMP_MIN * 2,
        0,
        vec![send2.hash()],
    ));

    let _ = node
        .vote_processor
        .vote_blocking(&ReceivedVote::new(vote2, VoteSource::Live, Some(channel)).into());

    let election1 = node
        .active
        .election_for_root(&send1.qualified_root())
        .unwrap();
    assert_eq!(1, election1.vote_count());
    let votes = election1.votes();
    assert!(votes.contains_key(&DEV_GENESIS_PUB_KEY));
    assert_eq!(send1.hash(), votes.get(&DEV_GENESIS_PUB_KEY).unwrap().hash);
    assert_eq!(send1.hash(), election1.winner().hash());
}

// Assuming necessary imports and module declarations are present
#[test]
fn vote_generator_cache() {
    let mut system = System::new();
    let node = system.make_node();

    let epoch1 = upgrade_epoch(node.clone(), Epoch::Epoch1);
    let wallet_id = WalletId::random();

    node.wallets.create(wallet_id);
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();

    node.vote_generators
        .generate_vote(&epoch1.root(), &epoch1.hash(), VoteType::NonFinal);

    // Wait until the votes are available
    assert_timely(Duration::from_secs(1), || {
        !node
            .history
            .votes(&epoch1.root(), &epoch1.hash(), false)
            .is_empty()
    });

    let votes = node.history.votes(&epoch1.root(), &epoch1.hash(), false);
    assert!(!votes.is_empty());

    let hashes = &votes[0].hashes;
    assert!(hashes.contains(&epoch1.hash()));
}

#[test]
fn vote_generator_multiple_representatives() {
    let mut system = System::new();
    let node = system.make_node();
    let wallet_id = WalletId::random();
    node.wallets.create(wallet_id);
    let key1 = PrivateKey::new();
    let key2 = PrivateKey::new();
    let key3 = PrivateKey::new();

    // Insert keys into the wallet
    node.wallets
        .insert_adhoc2(&wallet_id, &DEV_GENESIS_KEY.raw_key(), true)
        .unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &key1.raw_key(), true)
        .unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &key2.raw_key(), true)
        .unwrap();
    node.wallets
        .insert_adhoc2(&wallet_id, &key3.raw_key(), true)
        .unwrap();

    let amount = Amount::nano(100_000);

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key1.account(),
            amount,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key2.account(),
            amount,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    node.wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            key3.account(),
            amount,
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    // Assert balances
    assert_timely2(|| {
        node.balance(&key1.account()) == amount
            && node.balance(&key2.account()) == amount
            && node.balance(&key3.account()) == amount
    });

    // Change representatives
    node.wallets
        .change(
            &wallet_id,
            key1.account(),
            key1.public_key(),
            0.into(),
            true,
        )
        .wait()
        .unwrap();

    node.wallets
        .change(
            &wallet_id,
            key2.account(),
            key2.public_key(),
            0.into(),
            true,
        )
        .wait()
        .unwrap();

    node.wallets
        .change(
            &wallet_id,
            key3.account(),
            key3.public_key(),
            0.into(),
            true,
        )
        .wait()
        .unwrap();

    assert_eq!(node.ledger.weight(&key1.public_key()), amount);
    assert_eq!(node.ledger.weight(&key2.public_key()), amount);
    assert_eq!(node.ledger.weight(&key3.public_key()), amount);

    node.wallet_reps.lock().unwrap().compute_reps();
    assert_eq!(node.wallet_reps.lock().unwrap().voting_reps(), 4);

    let send = node
        .wallets
        .send(
            wallet_id,
            *DEV_GENESIS_ACCOUNT,
            *DEV_GENESIS_ACCOUNT,
            Amount::raw(1),
            0.into(),
            true,
            None,
        )
        .wait()
        .unwrap();

    // Wait until the votes are available
    assert_timely(Duration::from_secs(5), || {
        node.history.votes(&send.root(), &send.hash(), false).len() == 4
    });

    let votes = node.history.votes(&send.root(), &send.hash(), false);
    for account in &[
        key1.public_key(),
        key2.public_key(),
        key3.public_key(),
        DEV_GENESIS_KEY.public_key(),
    ] {
        let existing = votes.iter().find(|vote| vote.voter == *account);
        assert!(existing.is_some());
    }
}

#[test]
#[ignore = "Rewrite as unit test"]
fn vote_spacing_vote_generator() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    config.hinted_scheduler.hinted_limit_percentage = 0;
    let node = system
        .build_node()
        .config(config)
        .flags(NodeFlags {
            disable_search_pending: true,
            ..Default::default()
        })
        .finish();

    node.insert_into_wallet(&DEV_GENESIS_KEY);

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(1000));

    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let send2 = fork_lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(1001));

    node.ledger.process_one(&send1).unwrap();

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorBroadcasts,
                Direction::In,
            )
        },
        1,
    );

    node.ledger.roll_back(&send1.hash()).unwrap();
    node.ledger.process_one(&send2).unwrap();
    node.vote_generators.generate_vote(
        &(*DEV_GENESIS_HASH).into(),
        &send2.hash().into(),
        VoteType::NonFinal,
    );

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorSpacing,
                Direction::In,
            )
        },
        1,
    );

    assert_eq!(
        1,
        node.stats.count(
            StatType::VoteGenerator,
            DetailType::GeneratorBroadcasts,
            Direction::In
        )
    );
    std::thread::sleep(node.vote_generators.voting_delay());

    node.vote_generators.generate_vote(
        &(*DEV_GENESIS_HASH).into(),
        &send2.hash().into(),
        VoteType::NonFinal,
    );

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorBroadcasts,
                Direction::In,
            )
        },
        2,
    );
}

#[test]
#[ignore = "Rewrite as unit test"]
fn vote_spacing_rapid() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    config.hinted_scheduler.hinted_limit_percentage = 0; // Disable election hinting
    let node = system
        .build_node()
        .config(config)
        .flags(NodeFlags {
            disable_search_pending: true,
            ..Default::default()
        })
        .finish();

    node.insert_into_wallet(&DEV_GENESIS_KEY);

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send1 = lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(1000));

    let mut fork_lattice = UnsavedBlockLatticeBuilder::new();
    let send2 = fork_lattice
        .genesis()
        .send(&*DEV_GENESIS_KEY, Amount::nano(1001));

    node.process(send1.clone());

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorBroadcasts,
                Direction::In,
            )
        },
        1,
    );

    node.ledger.roll_back(&send1.hash()).unwrap();
    node.ledger.process_one(&send2).unwrap();

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorSpacing,
                Direction::In,
            )
        },
        1,
    );

    std::thread::sleep(node.vote_generators.voting_delay());

    node.vote_generators.generate_vote(
        &(*DEV_GENESIS_HASH).into(),
        &send2.hash().into(),
        VoteType::NonFinal,
    );

    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::VoteGenerator,
                DetailType::GeneratorBroadcasts,
                Direction::In,
            )
        },
        2,
    );
}
