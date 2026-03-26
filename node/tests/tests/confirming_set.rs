use std::time::Duration;

use rsnano_ledger::{LedgerSet, test_helpers::UnsavedBlockLatticeBuilder};
use rsnano_types::{Amount, PrivateKey};
use rsnano_utils::stats::{DetailType, Direction, StatType};
use test_helpers::{System, assert_always_eq, assert_timely_eq2, assert_timely2, start_election};

// The callback and confirmation history should only be updated after confirmation height is set (and not just after voting)
#[test]
fn confirmed_history() {
    let mut system = System::new();
    let mut config = System::default_config_without_backlog_scan();
    config.bootstrap.enable = false;
    let node = system.build_node().config(config).finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().send(&key1, Amount::nano(1000));
    let send2 = lattice.genesis().send(&key1, Amount::nano(1000));

    node.process_multi(&[send1.clone(), send2.clone()]);
    assert_timely2(|| node.active.is_active_hash(&send1.hash()));
    node.active.cancel(&send1.qualified_root());
    start_election(&node, &send2.hash());
    {
        // Prevent the confirming set doing any writes
        node.confirming_set.set_cooldown(true);

        // Confirm send1
        node.force_confirm(&send2.hash());
        assert_timely2(|| !node.is_active_hash(&send2.hash()));
        assert_eq!(node.recently_cemented.lock().unwrap().len(), 0);
        assert_eq!(node.ledger.confirmed().block_exists(&send1.hash()), false);

        // Confirm that no inactive callbacks have been called when the
        // confirmation height processor has already iterated over it, waiting to write
        assert_always_eq(
            Duration::from_millis(50),
            || {
                node.stats.count(
                    StatType::ConfirmationObserver,
                    DetailType::InactiveConfHeight,
                    Direction::Out,
                )
            },
            0,
        );
        node.confirming_set.set_cooldown(false);
    }

    assert_timely2(|| node.ledger.confirmed().block_exists(&send1.hash()));

    assert_timely_eq2(|| node.active.len(), 0);
    assert_timely_eq2(
        || node.stats().get("confirmation_observer", "active_quorum"),
        1,
    );

    // Each block that's confirmed is in the recently_cemented history
    assert_timely_eq2(|| node.recently_cemented.lock().unwrap().len(), 2);
    assert_eq!(node.active.len(), 0);

    // Confirm the callback is not called under this circumstance
    assert_timely_eq2(
        || node.stats().get("confirmation_observer", "active_quorum"),
        1,
    );
    assert_timely_eq2(|| node.stats().get("confirmation_observer", "inactive"), 1);
    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::ConfirmationHeight,
                DetailType::BlocksConfirmed,
                Direction::In,
            )
        },
        2,
    );
    assert_eq!(node.ledger.confirmed_count(), 3);
}

#[test]
fn dependent_election() {
    let mut system = System::new();
    let config = System::default_config_without_backlog_scan();
    let node = system.build_node().config(config).finish();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let key1 = PrivateKey::new();
    let send1 = lattice.genesis().send(&key1, Amount::nano(1000));
    let send2 = lattice.genesis().send(&key1, Amount::nano(1000));
    let send3 = lattice.genesis().send(&key1, Amount::nano(1000));
    node.process_multi(&[send1.clone(), send2.clone(), send3.clone()]);

    assert_timely2(|| node.active.is_active_hash(&send1.hash()));
    node.active.cancel(&send1.qualified_root());
    assert_timely2(|| !node.active.is_active_hash(&send1.hash()));

    // This election should be confirmed as active_conf_height
    start_election(&node, &send2.hash());
    // Start an election and confirm it
    start_election(&node, &send3.hash());
    node.force_confirm(&send3.hash());

    // Wait for blocks to be confirmed in ledger, callbacks will happen after
    assert_timely_eq2(
        || {
            node.stats.count(
                StatType::ConfirmationHeight,
                DetailType::BlocksConfirmed,
                Direction::In,
            )
        },
        3,
    );
    // Once the item added to the confirming set no longer exists, callbacks have completed
    assert_timely2(|| !node.confirming_set.contains(&send3.hash()));

    assert_timely_eq2(
        || node.stats().get("confirmation_observer", "active_quorum"),
        1,
    );
    assert_timely_eq2(
        || {
            node.stats()
                .get("confirmation_observer", "active_confirmation_height")
        },
        1,
    );
    assert_timely_eq2(|| node.stats().get("confirmation_observer", "inactive"), 1);
    assert_eq!(node.ledger.confirmed_count(), 4);
}
