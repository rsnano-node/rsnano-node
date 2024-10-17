use rsnano_core::Amount;
use rsnano_ledger::DEV_GENESIS_ACCOUNT;
use std::collections::HashMap;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn representatives_rpc_response() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node
        .runtime
        .block_on(async { rpc_client.representatives(None, None).await.unwrap() });

    let mut representatives = HashMap::new();
    representatives.insert(*DEV_GENESIS_ACCOUNT, Amount::MAX);

    assert_eq!(result.representatives, representatives);

    server.abort();
}
