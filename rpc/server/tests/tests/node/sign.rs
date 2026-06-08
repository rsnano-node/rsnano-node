use rsnano_ledger::test_helpers::UnsavedBlockLatticeBuilder;
use rsnano_rpc_messages::SignArgs;
use rsnano_types::{Account, Amount, Block, DEV_GENESIS_KEY, WalletId};
use test_helpers::{System, setup_rpc_client_and_server};

#[test]
fn sign() {
    let mut system = System::new();
    let node = system.make_node();

    let server = setup_rpc_client_and_server(node.clone(), true);

    let key = rsnano_types::PrivateKey::new();

    let wallet_id = WalletId::random();
    node.wallets.create(wallet_id);
    node.wallets
        .insert_adhoc2(&wallet_id, &key.raw_key(), false)
        .unwrap();

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice.genesis().send(&key, Amount::raw(1_000_000));

    let args: SignArgs = SignArgs {
        block: Some(send.json_representation()),
        wallet: None,
        account: None,
        hash: None,
        key: Some(DEV_GENESIS_KEY.raw_key()),
    };

    let result = node
        .runtime
        .block_on(async { server.client.sign(args).await.unwrap() });

    let signed_block: Block = result.block.unwrap().into();

    if let Block::State(ref state_block) = signed_block {
        assert!(state_block.verify_signature().is_ok());
    } else {
        panic!("Expected a state block");
    }

    assert_eq!(signed_block.signature(), send.signature());
    assert_eq!(signed_block.hash(), send.hash());
}

#[test]
fn sign_without_key() {
    let mut system = System::new();
    let node = system.make_node();

    let server = setup_rpc_client_and_server(node.clone(), true);

    let mut lattice = UnsavedBlockLatticeBuilder::new();
    let send = lattice
        .genesis()
        .send(Account::ZERO, Amount::raw(1_000_000));

    let result = node
        .runtime
        .block_on(async { server.client.sign(send.json_representation()).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some(
            "node returned error: \"Private key or local wallet and account required\"".to_string()
        )
    );
}
