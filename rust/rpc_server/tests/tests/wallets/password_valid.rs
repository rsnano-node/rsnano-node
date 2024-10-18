use rsnano_core::WalletId;
use rsnano_node::wallets::WalletsExt;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn password_valid() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let wallet_id: WalletId = 1.into();

    node.wallets.create(wallet_id);

    let _ = node.wallets.enter_password(wallet_id, "password");

    let result = node
        .runtime
        .block_on(async { rpc_client.password_valid(wallet_id).await.unwrap() });

    assert_eq!(result.valid, false);

    let _ = node.wallets.enter_password(wallet_id, "");

    let result = node
        .runtime
        .block_on(async { rpc_client.password_valid(wallet_id).await.unwrap() });

    assert_eq!(result.valid, true);

    server.abort();
}

#[test]
fn password_valid_fails_with_wallet_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node
        .runtime
        .block_on(async { rpc_client.password_valid(WalletId::zero()).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet not found\"".to_string())
    );

    server.abort();
}
