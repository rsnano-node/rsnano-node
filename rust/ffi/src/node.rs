use crate::{
    block_processing::{BacklogPopulationHandle, BlockProcessorHandle, UncheckedMapHandle},
    bootstrap::{BootstrapInitiatorHandle, BootstrapServerHandle, TcpListenerHandle},
    cementation::ConfirmingSetHandle,
    consensus::{
        ActiveTransactionsHandle, ElectionEndedCallback, ElectionSchedulerHandle,
        ElectionStatusHandle, FfiAccountBalanceCallback, HintedSchedulerHandle,
        LocalVoteHistoryHandle, ManualSchedulerHandle, OptimisticSchedulerHandle, RepTiersHandle,
        RequestAggregatorHandle, VoteCacheHandle, VoteGeneratorHandle, VoteHandle,
        VoteProcessorHandle, VoteProcessorQueueHandle, VoteProcessorVoteProcessedCallback,
        VoteWithWeightInfoVecHandle,
    },
    fill_node_config_dto,
    ledger::datastore::{lmdb::LmdbStoreHandle, LedgerHandle},
    representatives::{OnlineRepsHandle, RepCrawlerHandle, RepresentativeRegisterHandle},
    telemetry::TelemetryHandle,
    to_rust_string,
    transport::{
        ChannelHandle, LiveMessageProcessorHandle, NetworkFilterHandle, NetworkThreadsHandle,
        OutboundBandwidthLimiterHandle, SocketFfiObserver, SynCookiesHandle, TcpChannelsHandle,
        TcpMessageManagerHandle,
    },
    utils::{AsyncRuntimeHandle, ContainerInfoComponentHandle, ContextWrapper, ThreadPoolHandle},
    wallets::LmdbWalletsHandle,
    websocket::WebsocketListenerHandle,
    work::{DistributedWorkFactoryHandle, WorkPoolHandle},
    NetworkParamsDto, NodeConfigDto, NodeFlagsHandle, StatHandle, VoidPointerCallback,
};
use rsnano_core::{Vote, VoteCode};
use rsnano_node::{
    consensus::{AccountBalanceChangedCallback, ElectionEndCallback},
    node::{Node, NodeExt},
    transport::ChannelEnum,
};
use std::{
    ffi::{c_char, c_void},
    sync::Arc,
};

pub struct NodeHandle(Arc<Node>);

#[no_mangle]
pub unsafe extern "C" fn rsn_node_create(
    path: *const c_char,
    async_rt: &AsyncRuntimeHandle,
    config: &NodeConfigDto,
    params: &NetworkParamsDto,
    flags: &NodeFlagsHandle,
    work: &WorkPoolHandle,
    socket_observer: *mut c_void,
    observers_context: *mut c_void,
    delete_observers_context: VoidPointerCallback,
    election_ended: ElectionEndedCallback,
    balance_changed: FfiAccountBalanceCallback,
    vote_processed: VoteProcessorVoteProcessedCallback,
) -> *mut NodeHandle {
    let path = to_rust_string(path);
    let socket_observer = Arc::new(SocketFfiObserver::new(socket_observer));

    let ctx_wrapper = Arc::new(ContextWrapper::new(
        observers_context,
        delete_observers_context,
    ));

    let ctx = Arc::clone(&ctx_wrapper);
    let election_ended_wrapper: ElectionEndCallback = Box::new(
        move |status, votes, account, amount, is_state_send, is_state_epoch| {
            let status_handle = ElectionStatusHandle::new(status.clone());
            let votes_handle = VoteWithWeightInfoVecHandle::new(votes.clone());
            election_ended(
                ctx.get_context(),
                status_handle,
                votes_handle,
                account.as_bytes().as_ptr(),
                amount.to_be_bytes().as_ptr(),
                is_state_send,
                is_state_epoch,
            );
        },
    );

    let ctx = Arc::clone(&ctx_wrapper);
    let account_balance_changed_wrapper: AccountBalanceChangedCallback =
        Box::new(move |account, is_pending| {
            balance_changed(ctx.get_context(), account.as_bytes().as_ptr(), is_pending);
        });

    let ctx = Arc::clone(&ctx_wrapper);
    let vote_processed = Box::new(
        move |vote: &Arc<Vote>, channel: &Arc<ChannelEnum>, code: VoteCode| {
            let vote_handle = VoteHandle::new(Arc::clone(vote));
            let channel_handle = ChannelHandle::new(Arc::clone(channel));
            vote_processed(ctx.get_context(), vote_handle, channel_handle, code as u8);
        },
    );

    Box::into_raw(Box::new(NodeHandle(Arc::new(Node::new(
        Arc::clone(async_rt),
        path,
        config.try_into().unwrap(),
        params.try_into().unwrap(),
        flags.lock().unwrap().clone(),
        Arc::clone(work),
        socket_observer,
        election_ended_wrapper,
        account_balance_changed_wrapper,
        vote_processed,
    )))))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_node_destroy(handle: *mut NodeHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_node_node_id(handle: &NodeHandle, result: *mut u8) {
    handle.0.node_id.private_key().copy_bytes(result);
}

#[no_mangle]
pub extern "C" fn rsn_node_config(handle: &NodeHandle, result: &mut NodeConfigDto) {
    fill_node_config_dto(result, &handle.0.config);
}

#[no_mangle]
pub extern "C" fn rsn_node_stats(handle: &NodeHandle) -> *mut StatHandle {
    StatHandle::new(&Arc::clone(&handle.0.stats))
}

#[no_mangle]
pub extern "C" fn rsn_node_workers(handle: &NodeHandle) -> *mut ThreadPoolHandle {
    Box::into_raw(Box::new(ThreadPoolHandle(Arc::clone(&handle.0.workers))))
}

#[no_mangle]
pub extern "C" fn rsn_node_bootstrap_workers(handle: &NodeHandle) -> *mut ThreadPoolHandle {
    Box::into_raw(Box::new(ThreadPoolHandle(Arc::clone(
        &handle.0.bootstrap_workers,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_distributed_work(
    handle: &NodeHandle,
) -> *mut DistributedWorkFactoryHandle {
    Box::into_raw(Box::new(DistributedWorkFactoryHandle(Arc::clone(
        &handle.0.distributed_work,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_store(handle: &NodeHandle) -> *mut LmdbStoreHandle {
    Box::into_raw(Box::new(LmdbStoreHandle(Arc::clone(&handle.0.store))))
}

#[no_mangle]
pub extern "C" fn rsn_node_unchecked(handle: &NodeHandle) -> *mut UncheckedMapHandle {
    Box::into_raw(Box::new(UncheckedMapHandle(Arc::clone(
        &handle.0.unchecked,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_ledger(handle: &NodeHandle) -> *mut LedgerHandle {
    Box::into_raw(Box::new(LedgerHandle(Arc::clone(&handle.0.ledger))))
}

#[no_mangle]
pub extern "C" fn rsn_node_outbound_bandwidth_limiter(
    handle: &NodeHandle,
) -> *mut OutboundBandwidthLimiterHandle {
    Box::into_raw(Box::new(OutboundBandwidthLimiterHandle(Arc::clone(
        &handle.0.outbound_limiter,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_syn_cookies(handle: &NodeHandle) -> *mut SynCookiesHandle {
    Box::into_raw(Box::new(SynCookiesHandle(Arc::clone(
        &handle.0.syn_cookies,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_tcp_channels(handle: &NodeHandle) -> *mut TcpChannelsHandle {
    Box::into_raw(Box::new(TcpChannelsHandle(Arc::clone(&handle.0.channels))))
}

#[no_mangle]
pub extern "C" fn rsn_node_tcp_message_manager(
    handle: &NodeHandle,
) -> *mut TcpMessageManagerHandle {
    Box::into_raw(Box::new(TcpMessageManagerHandle(Arc::clone(
        &handle.0.channels.tcp_message_manager,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_network_filter(handle: &NodeHandle) -> *mut NetworkFilterHandle {
    Box::into_raw(Box::new(NetworkFilterHandle(Arc::clone(
        &handle.0.channels.publish_filter,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_telemetry(handle: &NodeHandle) -> *mut TelemetryHandle {
    Box::into_raw(Box::new(TelemetryHandle(Arc::clone(&handle.0.telemetry))))
}

#[no_mangle]
pub extern "C" fn rsn_node_bootstrap_server(handle: &NodeHandle) -> *mut BootstrapServerHandle {
    Box::into_raw(Box::new(BootstrapServerHandle(Arc::clone(
        &handle.0.bootstrap_server,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_online_reps(handle: &NodeHandle) -> *mut OnlineRepsHandle {
    Box::into_raw(Box::new(OnlineRepsHandle {
        online_reps: Arc::clone(&handle.0.online_reps),
        sampler: Arc::clone(&handle.0.online_reps_sampler),
    }))
}

#[no_mangle]
pub extern "C" fn rsn_node_representative_register(
    handle: &NodeHandle,
) -> *mut RepresentativeRegisterHandle {
    Box::into_raw(Box::new(RepresentativeRegisterHandle(Arc::clone(
        &handle.0.representative_register,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_rep_tiers(handle: &NodeHandle) -> *mut RepTiersHandle {
    Box::into_raw(Box::new(RepTiersHandle(Arc::clone(&handle.0.rep_tiers))))
}

#[no_mangle]
pub extern "C" fn rsn_node_vote_processor_queue(
    handle: &NodeHandle,
) -> *mut VoteProcessorQueueHandle {
    Box::into_raw(Box::new(VoteProcessorQueueHandle(Arc::clone(
        &handle.0.vote_processor_queue,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_history(handle: &NodeHandle) -> *mut LocalVoteHistoryHandle {
    Box::into_raw(Box::new(LocalVoteHistoryHandle(Arc::clone(
        &handle.0.history,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_confirming_set(handle: &NodeHandle) -> *mut ConfirmingSetHandle {
    Box::into_raw(Box::new(ConfirmingSetHandle(Arc::clone(
        &handle.0.confirming_set,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_vote_cache(handle: &NodeHandle) -> *mut VoteCacheHandle {
    Box::into_raw(Box::new(VoteCacheHandle(Arc::clone(&handle.0.vote_cache))))
}

#[no_mangle]
pub extern "C" fn rsn_node_block_processor(handle: &NodeHandle) -> *mut BlockProcessorHandle {
    Box::into_raw(Box::new(BlockProcessorHandle(Arc::clone(
        &handle.0.block_processor,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_wallets(handle: &NodeHandle) -> *mut LmdbWalletsHandle {
    Box::into_raw(Box::new(LmdbWalletsHandle(Arc::clone(&handle.0.wallets))))
}

#[no_mangle]
pub extern "C" fn rsn_node_vote_generator(handle: &NodeHandle) -> *mut VoteGeneratorHandle {
    Box::into_raw(Box::new(VoteGeneratorHandle(Arc::clone(
        &handle.0.vote_generator,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_active(handle: &NodeHandle) -> *mut ActiveTransactionsHandle {
    Box::into_raw(Box::new(ActiveTransactionsHandle(Arc::clone(
        &handle.0.active,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_vote_processor(handle: &NodeHandle) -> *mut VoteProcessorHandle {
    Box::into_raw(Box::new(VoteProcessorHandle(Arc::clone(
        &handle.0.vote_processor,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_websocket(handle: &NodeHandle) -> *mut WebsocketListenerHandle {
    match &handle.0.websocket {
        Some(ws) => Box::into_raw(Box::new(WebsocketListenerHandle(Arc::clone(ws)))),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rsn_node_bootstrap_initiator(
    handle: &NodeHandle,
) -> *mut BootstrapInitiatorHandle {
    Box::into_raw(Box::new(BootstrapInitiatorHandle(Arc::clone(
        &handle.0.bootstrap_initiator,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_rep_crawler(handle: &NodeHandle) -> *mut RepCrawlerHandle {
    Box::into_raw(Box::new(RepCrawlerHandle(Arc::clone(
        &handle.0.rep_crawler,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_tcp_listener(handle: &NodeHandle) -> *mut TcpListenerHandle {
    Box::into_raw(Box::new(TcpListenerHandle(Arc::clone(
        &handle.0.tcp_listener,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_hinted(handle: &NodeHandle) -> *mut HintedSchedulerHandle {
    Box::into_raw(Box::new(HintedSchedulerHandle(Arc::clone(
        &handle.0.hinted_scheduler,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_manual(handle: &NodeHandle) -> *mut ManualSchedulerHandle {
    Box::into_raw(Box::new(ManualSchedulerHandle(Arc::clone(
        &handle.0.manual_scheduler,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_optimistic(handle: &NodeHandle) -> *mut OptimisticSchedulerHandle {
    Box::into_raw(Box::new(OptimisticSchedulerHandle(Arc::clone(
        &handle.0.optimistic_scheduler,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_priority(handle: &NodeHandle) -> *mut ElectionSchedulerHandle {
    Box::into_raw(Box::new(ElectionSchedulerHandle(Arc::clone(
        &handle.0.priority_scheduler,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_request_aggregator(handle: &NodeHandle) -> *mut RequestAggregatorHandle {
    Box::into_raw(Box::new(RequestAggregatorHandle(Arc::clone(
        &handle.0.request_aggregator,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_backlog_population(handle: &NodeHandle) -> *mut BacklogPopulationHandle {
    Box::into_raw(Box::new(BacklogPopulationHandle(Arc::clone(
        &handle.0.backlog_population,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_live_message_processor(
    handle: &NodeHandle,
) -> *mut LiveMessageProcessorHandle {
    Box::into_raw(Box::new(LiveMessageProcessorHandle(Arc::clone(
        &handle.0.live_message_processor,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_network_threads(handle: &NodeHandle) -> *mut NetworkThreadsHandle {
    Box::into_raw(Box::new(NetworkThreadsHandle(Arc::clone(
        &handle.0.network_threads,
    ))))
}

#[no_mangle]
pub extern "C" fn rsn_node_start(handle: &NodeHandle) {
    handle.0.start();
}

#[no_mangle]
pub extern "C" fn rsn_node_stop(handle: &NodeHandle) {
    handle.0.stop();
}

#[no_mangle]
pub extern "C" fn rsn_node_is_stopped(handle: &NodeHandle) -> bool {
    handle.0.is_stopped()
}

#[no_mangle]
pub extern "C" fn rsn_node_add_initial_peers(handle: &NodeHandle) {
    handle.0.add_initial_peers();
}

#[no_mangle]
pub extern "C" fn rsn_node_ledger_pruning(
    handle: &NodeHandle,
    batch_size: u64,
    bootstrap_weight_reached: bool,
) {
    handle
        .0
        .ledger_pruning(batch_size, bootstrap_weight_reached);
}

#[no_mangle]
pub extern "C" fn rsn_node_bootstrap_wallet(handle: &NodeHandle) {
    handle.0.bootstrap_wallet();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_node_collect_container_info(
    handle: &NodeHandle,
    name: *const c_char,
) -> *mut ContainerInfoComponentHandle {
    let container_info = handle.0.collect_container_info(to_rust_string(name));
    Box::into_raw(Box::new(ContainerInfoComponentHandle(container_info)))
}