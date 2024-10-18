use rsnano_core::{Account, WalletId};
use rsnano_node::wallets::WalletsExt;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn account_move() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);
    node.wallets.create(source);

    let account = node
        .wallets
        .deterministic_insert2(&source, false)
        .unwrap()
        .into();

    let wallet_accounts = node.wallets.get_accounts_of_wallet(&wallet).unwrap();
    let source_accounts = node.wallets.get_accounts_of_wallet(&source).unwrap();

    assert!(!wallet_accounts.contains(&account));
    assert!(source_accounts.contains(&account));

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![account])
            .await
            .unwrap()
    });

    assert_eq!(result.moved, true);

    let new_wallet_accounts = node.wallets.get_accounts_of_wallet(&wallet).unwrap();
    let new_source_accounts = node.wallets.get_accounts_of_wallet(&source).unwrap();

    assert!(new_wallet_accounts.contains(&account));
    assert!(!new_source_accounts.contains(&account));

    server.abort();
}

#[test]
fn account_remove_fails_without_enable_control() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);
    node.wallets.create(source);

    let account = node
        .wallets
        .deterministic_insert2(&source, false)
        .unwrap()
        .into();

    let wallet_accounts = node.wallets.get_accounts_of_wallet(&wallet).unwrap();
    let source_accounts = node.wallets.get_accounts_of_wallet(&source).unwrap();

    assert!(!wallet_accounts.contains(&account));
    assert!(source_accounts.contains(&account));

    let result = node
        .runtime
        .block_on(async { rpc_client.account_move(wallet, source, vec![account]).await });

    assert!(result.is_err());

    server.abort();
}

#[test]
fn account_move_fails_source_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![Account::zero()])
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet not found\"".to_string())
    );

    server.abort();
}

#[test]
fn account_move_fails_target_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(source);

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![Account::zero()])
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet not found\"".to_string())
    );

    server.abort();
}

#[test]
fn account_move_fails_source_locked() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);
    node.wallets.create(source);

    node.wallets.lock(&source).unwrap();

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![Account::zero()])
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet is locked\"".to_string())
    );

    server.abort();
}

#[test]
fn account_move_fails_target_locked() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);
    node.wallets.create(source);

    node.wallets.lock(&wallet).unwrap();

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![Account::zero()])
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Wallet is locked\"".to_string())
    );

    server.abort();
}

#[test]
fn account_move_fails_account_not_found() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let wallet = WalletId::random();
    let source = WalletId::random();

    node.wallets.create(wallet);
    node.wallets.create(source);

    let result = node.runtime.block_on(async {
        rpc_client
            .account_move(wallet, source, vec![Account::zero()])
            .await
    });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Account not found\"".to_string())
    );

    server.abort();
}
