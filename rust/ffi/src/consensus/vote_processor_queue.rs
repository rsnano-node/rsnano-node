use super::VoteHandle;
use crate::{
    ledger::datastore::LedgerHandle,
    representatives::OnlineRepsHandle,
    transport::ChannelHandle,
    utils::{ContainerInfoComponentHandle, LoggerHandle, LoggerMT},
    StatHandle,
};
use rsnano_core::{Account, Vote};
use rsnano_node::{consensus::VoteProcessorQueue, transport::ChannelEnum};
use std::{
    collections::VecDeque,
    ffi::{c_char, CStr},
    ops::Deref,
    sync::Arc,
};

pub struct VoteProcessorQueueHandle(Arc<VoteProcessorQueue>);

impl Deref for VoteProcessorQueueHandle {
    type Target = Arc<VoteProcessorQueue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_vote_processor_queue_create(
    max_votes: usize,
    stats: &StatHandle,
    online_reps: &OnlineRepsHandle,
    ledger: &LedgerHandle,
    logger: *mut LoggerHandle,
) -> *mut VoteProcessorQueueHandle {
    let logger = Arc::new(LoggerMT::new(Box::from_raw(logger)));
    Box::into_raw(Box::new(VoteProcessorQueueHandle(Arc::new(
        VoteProcessorQueue::new(
            max_votes,
            Arc::clone(stats),
            Arc::clone(online_reps),
            Arc::clone(ledger),
            logger,
        ),
    ))))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_vote_processor_queue_destroy(handle: *mut VoteProcessorQueueHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_len(handle: &VoteProcessorQueueHandle) -> usize {
    handle.0.len()
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_is_empty(handle: &VoteProcessorQueueHandle) -> bool {
    handle.0.is_empty()
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_vote(
    handle: &VoteProcessorQueueHandle,
    vote: &VoteHandle,
    channel: &ChannelHandle,
) -> bool {
    handle.0.vote(vote, channel)
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_calculate_weights(handle: &VoteProcessorQueueHandle) {
    handle.0.calculate_weights();
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_wait_and_take(
    handle: &VoteProcessorQueueHandle,
) -> *mut RawVoteProcessorQueueHandle {
    let new_votes = handle.0.wait_for_votes();
    Box::into_raw(Box::new(RawVoteProcessorQueueHandle(new_votes)))
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_flush(handle: &VoteProcessorQueueHandle) {
    handle.0.flush();
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_clear(handle: &VoteProcessorQueueHandle) {
    handle.0.clear();
}

#[no_mangle]
pub extern "C" fn rsn_vote_processor_queue_stop(handle: &VoteProcessorQueueHandle) {
    handle.0.stop();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_vote_processor_queue_reps_contains(
    handle: &VoteProcessorQueueHandle,
    reps_id: u8,
    account: *const u8,
) -> bool {
    handle.0.reps_contains(reps_id, &Account::from_ptr(account))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_vote_processor_collect_container_info(
    handle: &VoteProcessorQueueHandle,
    name: *const c_char,
) -> *mut ContainerInfoComponentHandle {
    Box::into_raw(Box::new(ContainerInfoComponentHandle(
        handle
            .0
            .collect_container_info(CStr::from_ptr(name).to_string_lossy().to_string()),
    )))
}

pub struct RawVoteProcessorQueueHandle(VecDeque<(Arc<Vote>, Arc<ChannelEnum>)>);

#[no_mangle]
pub unsafe extern "C" fn rsn_raw_vote_processor_queue_destroy(
    handle: *mut RawVoteProcessorQueueHandle,
) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_raw_vote_processor_queue_len(
    handle: &RawVoteProcessorQueueHandle,
) -> usize {
    handle.0.len()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_raw_vote_processor_queue_get(
    handle: &RawVoteProcessorQueueHandle,
    index: usize,
    vote: *mut *mut VoteHandle,
    channel: *mut *mut ChannelHandle,
) {
    let (v, c) = handle.0.get(index).unwrap();
    *vote = Box::into_raw(Box::new(VoteHandle::new(Arc::clone(v))));
    *channel = ChannelHandle::new(Arc::clone(c));
}