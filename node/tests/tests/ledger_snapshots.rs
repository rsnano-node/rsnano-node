use rsnano_messages::MessageType;
use rsnano_node::{Node, config::NodeConfig};
use rsnano_types::{Amount, DEV_GENESIS_KEY};
use rsnano_utils::stats::{Direction, StatType};
use test_helpers::{System, assert_timely_eq2, assert_timely2, setup_rep};

#[test]
fn publish_preproposal_integration_test() {
    let mut system = System::new();

    let node1 = system
        .build_node()
        .config(NodeConfig {
            enable_voting: true,
            ..System::default_config()
        })
        .finish();
    node1.insert_into_wallet(&DEV_GENESIS_KEY);

    let node2 = system
        .build_node()
        .config(NodeConfig {
            enable_voting: true,
            ..System::default_config()
        })
        .finish();
    let amount_pr = Amount::nano(2_000_000);
    let rep2_key = setup_rep(&node2, amount_pr, &DEV_GENESIS_KEY);
    node2.insert_into_wallet(&rep2_key);

    assert_peered_principal_reps(&node1, 2);
    assert_peered_principal_reps(&node2, 2);

    node1.ledger_snapshots.start_ledger_snapshot();

    assert_message_received(&node1, MessageType::Preproposal, 1);
    assert_message_received(&node2, MessageType::Preproposal, 1);

    assert_message_received(&node1, MessageType::Proposal, 2);
    assert_message_received(&node2, MessageType::Proposal, 2);

    assert_message_received(&node1, MessageType::ProposalVote, 2);
    assert_message_received(&node2, MessageType::ProposalVote, 2);
}

// Helper functions:
// -----------------------------------------------------------------------------

fn assert_peered_principal_reps(node: &Node, expected_rep_count: usize) {
    assert_timely2(|| {
        node.online_reps
            .lock()
            .unwrap()
            .peered_principal_reps()
            .len()
            == expected_rep_count
    });
}

fn assert_message_received(node: &Node, message_type: MessageType, count: usize) {
    assert_timely_eq2(
        || {
            node.stats
                .count(StatType::Message, message_type.into(), Direction::In) as usize
        },
        count,
    );
}
