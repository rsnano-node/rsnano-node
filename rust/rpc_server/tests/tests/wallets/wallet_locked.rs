use rsnano_core::WalletId;
use rsnano_node::wallets::WalletsExt;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn wallet_locked_false() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet_id: WalletId = 1.into();

    node.wallets.create(wallet_id);

    assert_eq!(node.wallets.valid_password(&wallet_id).unwrap(), true);

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_locked(wallet_id).await.unwrap() });

    assert_eq!(result.locked, false);

    server.abort();
}

#[test]
fn wallet_locked_true() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let wallet_id: WalletId = 1.into();

    node.wallets.create(wallet_id);

    node.wallets.lock(&wallet_id).unwrap();

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_locked(wallet_id).await.unwrap() });

    assert_eq!(result.locked, true);

    server.abort();
}

#[test]
fn wallet_locked_fails_with_wallet_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_locked(WalletId::zero()).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet not found\"".to_string())
    );

    server.abort();
}
