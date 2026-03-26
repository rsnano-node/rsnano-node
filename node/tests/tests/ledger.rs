use std::sync::Arc;

use rsnano_ledger::{
    BlockError, BlockSource, DEV_GENESIS_PUB_KEY, LedgerSet,
    test_helpers::UnsavedBlockLatticeBuilder,
};
use rsnano_network::ChannelId;
use rsnano_node::block_processing::BlockContext;
use rsnano_types::{
    Account, Amount, Block, BlockHash, DEV_GENESIS_KEY, Epoch, PrivateKey, QualifiedRoot,
    Signature, StateBlockArgs, Vote, VoteError, VoteSource,
};
use test_helpers::{System, assert_timely_eq2, assert_timely2, start_elections};

mod votes {
    use super::*;
    use rsnano_ledger::test_helpers::UnsavedBlockLatticeBuilder;
    use rsnano_node::consensus::ReceivedVote;

    #[test]
    fn add_one() {
        let mut system = System::new();
        let node1 = system.make_node();

        let mut lattice = UnsavedBlockLatticeBuilder::new();
        let key1 = PrivateKey::new();
        let send1 = lattice.genesis().legacy_send(&key1, 100);
        let send1 = node1.process(send1);
        assert_timely2(|| node1.is_active_hash(&send1.hash()));

        let vote1 = Arc::new(Vote::new(
            &DEV_GENESIS_KEY,
            Vote::TIMESTAMP_MIN,
            0,
            vec![send1.hash()],
        ));

        node1
            .vote_processor
            .vote_blocking(&ReceivedVote::new(vote1.into(), VoteSource::Live, None).into())
            .unwrap();

        let vote2 = ReceivedVote::new(
            Arc::new(Vote::new(
                &DEV_GENESIS_KEY,
                Vote::TIMESTAMP_MIN * 2,
                0,
                vec![send1.hash()],
            )),
            VoteSource::Live,
            None,
        );

        // Ignored due to vote cooldown
        assert_eq!(
            node1.vote_processor.vote_blocking(&vote2.into()),
            Err(VoteError::Ignored)
        );

        assert_eq!(
            node1
                .active
                .election_for_block(&send1.hash())
                .unwrap()
                .vote_count(),
            1
        );
        assert_eq!(
            node1
                .active
                .election_for_block(&send1.hash())
                .unwrap()
                .votes()
                .get(&DEV_GENESIS_PUB_KEY)
                .unwrap()
                .hash,
            send1.hash()
        );
    }
}

#[test]
fn epoch_open_pending() {
    let mut system = System::new();
    let node1 = system.make_node();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();

    let send1 = lattice.genesis().send(&key1, 100);
    let epoch_open = lattice.epoch_open(&key1);

    let status = node1.try_process(epoch_open.clone()).unwrap_err();
    assert_eq!(status, BlockError::GapEpochOpenPending);

    // New block to process epoch open
    node1.process(send1);

    node1.block_processor_queue.push(BlockContext::new(
        epoch_open.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));

    assert_timely2(|| node1.block_exists(&epoch_open.hash()));
}

#[test]
fn block_hash_account_conflict() {
    let mut system = System::new();
    let node1 = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();

    /*
     * Generate a send block whose destination is a block hash already
     * in the ledger and not an account
     */
    let send1 = lattice.genesis().send(&key1, 100);
    let receive1 = lattice.account(&key1).receive(&send1);

    /*
     * Note that the below link is a block hash when this is intended
     * to represent a send state block. This can generally never be
     * received , except by epoch blocks, which can sign an open block
     * for arbitrary accounts.
     */
    let unreceivable_account = Account::from_bytes(*receive1.hash().as_bytes());
    let send2 = lattice.account(&key1).send(unreceivable_account, 10);

    /*
     * Generate an epoch open for the account with the same value as the block hash
     */
    let open_epoch1 = lattice.epoch_open(unreceivable_account);

    node1.process_multi(&[
        send1.clone(),
        receive1.clone(),
        send2.clone(),
        open_epoch1.clone(),
    ]);

    start_elections(
        &node1,
        &[
            send1.hash(),
            receive1.hash(),
            send2.hash(),
            open_epoch1.hash(),
        ],
        false,
    );

    let winner_for = |root: &QualifiedRoot| {
        node1
            .active
            .election_for_root(root)
            .unwrap()
            .winner()
            .hash()
    };

    assert_eq!(winner_for(&send1.qualified_root()), send1.hash());
    assert_eq!(winner_for(&receive1.qualified_root()), receive1.hash());
    assert_eq!(winner_for(&send2.qualified_root()), send2.hash());
    assert_eq!(
        winner_for(&open_epoch1.qualified_root()),
        open_epoch1.hash()
    );
}

#[test]
fn unchecked_epoch() {
    let mut system = System::new();
    let node1 = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let destination = PrivateKey::new();

    let send1 = lattice.genesis().send(&destination, Amount::nano(1000));
    let open1 = lattice.account(&destination).receive(&send1);
    let epoch1 = lattice.account(&destination).epoch1();

    node1.block_processor_queue.push(BlockContext::new(
        epoch1.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));

    // Waits for the epoch1 block to pass through block_processor and unchecked.put queues
    assert_timely_eq2(|| node1.unchecked.lock().unwrap().len(), 1);
    node1.block_processor_queue.push(BlockContext::new(
        send1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    node1.block_processor_queue.push(BlockContext::new(
        open1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    assert_timely2(|| node1.ledger.any().block_exists(&epoch1.hash()));

    // Waits for the last blocks to pass through block_processor and unchecked.put queues
    assert_timely_eq2(|| node1.unchecked.lock().unwrap().len(), 0);
    let info = node1
        .ledger
        .any()
        .get_account(&destination.account())
        .unwrap();
    assert_eq!(info.epoch, Epoch::Epoch1);
}

#[test]
fn unchecked_epoch_invalid() {
    let mut system = System::new();
    let node1 = system
        .build_node()
        .config(System::default_config_without_backlog_scan())
        .finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let destination = PrivateKey::new();

    let send1 = lattice.genesis().send(&destination, Amount::nano(1000));
    let open1 = lattice.account(&destination).receive(&send1);

    // Epoch block with account own signature
    let epoch1: Block = StateBlockArgs {
        key: &destination,
        previous: open1.hash(),
        representative: destination.public_key(),
        balance: Amount::nano(1000),
        link: node1.ledger.epoch_link(Epoch::Epoch1).unwrap(),
        work: node1.work_generate_dev(open1.hash()),
    }
    .into();

    // Pseudo epoch block (send subtype, destination - epoch link)
    let epoch2: Block = StateBlockArgs {
        key: &destination,
        previous: open1.hash(),
        representative: destination.public_key(),
        balance: Amount::nano(999),
        link: node1.ledger.epoch_link(Epoch::Epoch1).unwrap(),
        work: node1.work_generate_dev(open1.hash()),
    }
    .into();

    node1.block_processor_queue.push(BlockContext::new(
        epoch1.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    node1.block_processor_queue.push(BlockContext::new(
        epoch2.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));

    // Waits for the last blocks to pass through block_processor and unchecked.put queues
    assert_timely_eq2(|| node1.unchecked.lock().unwrap().len(), 2);
    node1.block_processor_queue.push(BlockContext::new(
        send1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    node1.block_processor_queue.push(BlockContext::new(
        open1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));

    // Waits for the last blocks to pass through block_processor and unchecked.put queues
    assert_timely2(|| node1.ledger.any().block_exists(&epoch2.hash()));

    let any = node1.ledger.any();
    assert_eq!(any.block_exists(&epoch1.hash()), false);
    assert_eq!(node1.unchecked.lock().unwrap().len(), 0);
    let info = any.get_account(&destination.account()).unwrap();
    assert_eq!(info.epoch, Epoch::Epoch0);
    let epoch2_store = node1.block(&epoch2.hash()).unwrap();
    assert_eq!(epoch2_store.epoch(), Epoch::Epoch0);
    assert!(epoch2_store.is_send());
    assert_eq!(epoch2_store.is_epoch(), false);
    assert_eq!(epoch2_store.is_receive(), false);
}

#[test]
fn unchecked_open() {
    let mut system = System::new();
    let node1 = system.make_node();
    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let destination = PrivateKey::new();
    let send1 = lattice.genesis().send(&destination, Amount::nano(1000));
    let open1 = lattice.account(&destination).receive(&send1);
    // Invalid signature for open block
    let mut open2 = open1.clone();
    open2.set_signature(Signature::from_bytes([1; 64]));

    // Insert open2 in to the queue before open1
    node1.block_processor_queue.push(BlockContext::new(
        open2.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    node1.block_processor_queue.push(BlockContext::new(
        open1.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));

    // Waits for the last blocks to pass through block_processor and unchecked.put queues
    assert_timely_eq2(|| node1.unchecked.lock().unwrap().len(), 1);
    // When open1 existists in unchecked, we know open2 has been processed.
    node1.block_processor_queue.push(BlockContext::new(
        send1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    // Waits for the send1 block to pass through block_processor and unchecked.put queues
    assert_timely2(|| node1.block_exists(&open1.hash()));
    assert_eq!(node1.unchecked.lock().unwrap().len(), 0);
}

#[test]
fn unchecked_receive() {
    let mut system = System::new();
    let node1 = system.make_node();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let destination = PrivateKey::new();
    let send1 = lattice.genesis().send(&destination, Amount::nano(1000));
    let send2 = lattice.genesis().send(&destination, Amount::nano(1000));
    let open1 = lattice.account(&destination).receive(&send1);
    let receive1 = lattice.account(&destination).receive(&send2);
    node1.block_processor_queue.push(BlockContext::new(
        send1.into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    node1.block_processor_queue.push(BlockContext::new(
        receive1.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    let check_block_is_listed =
        |hash: &BlockHash| node1.unchecked.lock().unwrap().contains_dependency(*hash);
    // Previous block for receive1 is unknown, signature cannot be validated

    // Waits for the last blocks to pass through block_processor and unchecked.put queues
    assert_timely2(|| check_block_is_listed(&receive1.previous()));
    assert_eq!(
        node1
            .unchecked
            .lock()
            .unwrap()
            .blocks_dependend_on(receive1.previous())
            .count(),
        1
    );

    // Waits for the open1 block to pass through block_processor and unchecked.put queues
    node1.block_processor_queue.push(BlockContext::new(
        open1.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    assert_timely2(|| check_block_is_listed(&receive1.source_or_link()));
    // Previous block for receive1 is known, signature was validated
    assert_eq!(
        node1
            .unchecked
            .lock()
            .unwrap()
            .blocks_dependend_on(receive1.source_or_link())
            .count(),
        1
    );
    node1.block_processor_queue.push(BlockContext::new(
        send2.clone().into(),
        BlockSource::Live,
        ChannelId::LOOPBACK,
    ));
    assert_timely2(|| node1.block_exists(&receive1.hash()));
    assert_eq!(node1.unchecked.lock().unwrap().len(), 0);
}
