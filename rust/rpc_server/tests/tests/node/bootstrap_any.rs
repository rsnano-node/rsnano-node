use rsnano_node::config::NodeFlags;
use rsnano_rpc_messages::BootstrapAnyArgs;
use test_helpers::{send_block, setup_rpc_client_and_server, System};

#[test]
fn bootstrap_any() {
    let mut system = System::new();
    let node = system.make_node();

    send_block(node.clone());

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    node.runtime.block_on(async {
        rpc_client
            .bootstrap_any(BootstrapAnyArgs::default())
            .await
            .unwrap()
    });

    server.abort();
}

#[test]
fn bootstrap_any_fails_with_legacy_bootstrap_disabled() {
    let mut system = System::new();
    let mut flags = NodeFlags::new();
    flags.disable_legacy_bootstrap = true;
    let node = system.build_node().flags(flags).finish();

    send_block(node.clone());

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node
        .runtime
        .block_on(async { rpc_client.bootstrap_any(BootstrapAnyArgs::default()).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"Legacy bootstrap is disabled\"".to_string())
    );

    server.abort();
}
