use rsnano_core::{Account, WalletId};
use rsnano_node::wallets::WalletsExt;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn wallet_contains_true() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet: WalletId = 1.into();

    node.wallets.create(1.into());

    let account = node
        .wallets
        .deterministic_insert2(&wallet, false)
        .unwrap()
        .into();

    assert!(node.wallets.exists(&account));

    let result = node.runtime.block_on(async {
        rpc_client
            .wallet_contains(wallet, account.into())
            .await
            .unwrap()
    });

    assert_eq!(result.exists, true);

    server.abort();
}

#[test]
fn wallet_contains_false() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet: WalletId = 1.into();

    node.wallets.create(1.into());

    let result = node.runtime.block_on(async {
        rpc_client
            .wallet_contains(wallet, Account::zero())
            .await
            .unwrap()
    });

    assert_eq!(result.exists, false);

    server.abort();
}

#[test]
fn wallet_contains_fails_with_wallet_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node.runtime.block_on(async {
        rpc_client
            .wallet_contains(WalletId::zero(), Account::zero())
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet not found\"".to_string())
    );

    server.abort();
}
