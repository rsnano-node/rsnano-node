use std::u64;

use rsnano_core::BlockHash;
use rsnano_rpc_messages::SuccessDto;
use test_helpers::{assert_timely, setup_rpc_client_and_server, System};

#[test]
fn work_cancel() {
    let mut system = System::new();
    let node = system.make_node();
    let node_clone = node.clone();
    let node_clone2 = node.clone();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), true);

    let hash = BlockHash::random();

    let work_handle = node.clone().runtime.spawn(async move {
        node_clone2
            .distributed_work
            .make(hash.into(), node_clone2.network_params.work.base, None)
            .await
    });

    assert_timely(std::time::Duration::from_millis(100), || {
        node_clone
            .runtime
            .block_on(async { !work_handle.is_finished() })
    });

    // Ensure work generation was actually cancelled
    assert_timely(std::time::Duration::from_secs(10), || {
        node_clone
            .runtime
            .block_on(async { work_handle.is_finished() })
    });

    let work_result = node_clone
        .runtime
        .block_on(async { work_handle.await.unwrap() });
    assert!(work_result.is_some());

    let work_handle = node.clone().runtime.spawn(async move {
        node.distributed_work
            .make(hash.into(), u64::MAX, None)
            .await
    });

    let result = node_clone
        .runtime
        .block_on(async { rpc_client.work_cancel(hash).await.unwrap() });

    // Check the result
    assert_eq!(result, SuccessDto::new());

    // Ensure work generation was actually cancelled
    assert_timely(std::time::Duration::from_secs(10), || {
        node_clone
            .runtime
            .block_on(async { work_handle.is_finished() })
    });

    let work_result = node_clone
        .runtime
        .block_on(async { work_handle.await.unwrap() });
    assert!(work_result.is_none());

    server.abort();
}

#[test]
fn work_cancel_fails_without_enable_control() {
    let mut system = System::new();
    let node = system.make_node();
    let node_clone = node.clone();

    let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

    let result = node_clone
        .runtime
        .block_on(async { rpc_client.work_cancel(BlockHash::zero()).await });

    assert_eq!(
        result.err().map(|e| e.to_string()),
        Some("node returned error: \"RPC control is disabled\"".to_string())
    );

    server.abort();
}
