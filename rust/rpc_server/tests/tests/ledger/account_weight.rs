use rsnano_core::Amount;
use rsnano_ledger::DEV_GENESIS_ACCOUNT;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn account_weight() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node.runtime.block_on(async {
        rpc_client
            .account_weight(DEV_GENESIS_ACCOUNT.to_owned())
            .await
            .unwrap()
    });

    assert_eq!(result.weight, Amount::MAX);

    server.abort();
}
