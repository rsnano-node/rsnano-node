use rsnano_core::{PublicKey, WalletId};
use rsnano_node::wallets::WalletsExt;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn account_get() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet_id = WalletId::random();

    node.wallets.create(wallet_id);

    let result = node
        .runtime
        .block_on(async { rpc_client.account_get(PublicKey::zero()).await.unwrap() });

    assert_eq!(result.account, PublicKey::zero().into());

    server.abort();
}
