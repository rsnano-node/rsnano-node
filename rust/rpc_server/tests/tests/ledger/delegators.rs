use rsnano_core::Amount;
use rsnano_ledger::DEV_GENESIS_ACCOUNT;
use std::collections::HashMap;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn delegators_rpc_response() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node
        .runtime
        .block_on(async { rpc_client.delegators(*DEV_GENESIS_ACCOUNT).await.unwrap() });

    let mut delegators = HashMap::new();
    delegators.insert(*DEV_GENESIS_ACCOUNT, Amount::MAX);

    assert_eq!(result.delegators, delegators);

    server.abort();
}
