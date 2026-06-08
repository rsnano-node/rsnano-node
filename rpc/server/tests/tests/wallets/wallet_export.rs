use rsnano_types::WalletId;
use test_helpers::{System, setup_rpc_client_and_server};

#[test]
fn wallet_export() {
    let mut system = System::new();
    let node = system.make_node();

    let server = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    node.wallets.create(wallet);

    let result = node
        .runtime
        .block_on(async { server.client.wallet_export(wallet).await.unwrap() });

    assert_ne!(result.json, "");
}
