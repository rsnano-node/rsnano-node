use rsnano_core::Amount;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn nano_to_raw() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node
        .runtime
        .block_on(async { rpc_client.nano_to_raw(Amount::nano(1)).await.unwrap() });

    assert_eq!(result.amount, Amount::raw(1000000000000000000000000000000));

    server.abort();
}
