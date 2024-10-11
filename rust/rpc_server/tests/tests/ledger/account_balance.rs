use rsnano_core::{Amount, DEV_GENESIS_KEY};
use rsnano_rpc_messages::AccountBalanceArgs;
use test_helpers::{send_block, setup_rpc_client_and_server, System};

#[test]
fn account_balance_only_confirmed_none() {
    let mut system = System::new();
    let node = system.make_node();

    send_block(node.clone());

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node.runtime.block_on(async {
        rpc_client
            .account_balance(DEV_GENESIS_KEY.public_key().as_account())
            .await
            .unwrap()
    });

    assert_eq!(
        result.balance,
        Amount::raw(340282366920938463463374607431768211455)
    );

    assert_eq!(result.pending, Amount::zero());

    assert_eq!(result.receivable, Amount::zero());

    server.abort();
}

#[test]
fn account_balance_only_confirmed_true() {
    let mut system = System::new();
    let node = system.make_node();

    send_block(node.clone());

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let args = AccountBalanceArgs::builder(DEV_GENESIS_KEY.public_key().as_account())
        .include_only_confirmed()
        .finish();

    let result = node
        .runtime
        .block_on(async { rpc_client.account_balance(args).await.unwrap() });

    assert_eq!(
        result.balance,
        Amount::raw(340282366920938463463374607431768211455)
    );

    assert_eq!(result.pending, Amount::zero());

    assert_eq!(result.receivable, Amount::zero());

    server.abort();
}