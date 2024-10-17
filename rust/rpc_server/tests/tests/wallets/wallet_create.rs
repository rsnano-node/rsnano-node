use rsnano_core::RawKey;
use test_helpers::{setup_rpc_client_and_server, System};

#[test]
fn wallet_create_seed_none() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_create(None).await.unwrap() });

    let wallets = node.wallets.wallet_ids();

    assert!(wallets.contains(&result.wallet));

    server.abort();
}

#[test]
fn wallet_create_seed_some() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let seed = RawKey::from_slice(&[1u8; 32]).unwrap();

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_create(Some(seed)).await.unwrap() });

    let wallets = node.wallets.wallet_ids();

    assert!(wallets.contains(&result.wallet));

    server.abort();
}

#[test]
fn wallet_create_fails_without_enable_control() {
    let mut system = System::new();
    let node = system.make_node();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node
        .runtime
        .block_on(async { rpc_client.wallet_create(None).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"RPC control is disabled\"".to_string())
    );

    server.abort();
}
