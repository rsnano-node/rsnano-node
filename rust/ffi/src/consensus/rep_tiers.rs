use crate::{
    ledger::datastore::LedgerHandle, representatives::OnlineRepsHandle, NetworkParamsDto,
    StatHandle,
};
use rsnano_core::Account;
use rsnano_node::consensus::RepTiers;
use std::{ops::Deref, sync::Arc};

pub struct RepTiersHandle(pub Arc<RepTiers>);

impl Deref for RepTiersHandle {
    type Target = Arc<RepTiers>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[no_mangle]
pub extern "C" fn rsn_rep_tiers_create(
    ledger: &LedgerHandle,
    network_params: &NetworkParamsDto,
    online_reps: &OnlineRepsHandle,
    stats: &StatHandle,
) -> *mut RepTiersHandle {
    Box::into_raw(Box::new(RepTiersHandle(Arc::new(RepTiers::new(
        Arc::clone(ledger),
        network_params.try_into().unwrap(),
        Arc::clone(online_reps),
        Arc::clone(stats),
    )))))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_rep_tiers_destroy(handle: *mut RepTiersHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub extern "C" fn rsn_rep_tiers_start(handle: &RepTiersHandle) {
    handle.start();
}

#[no_mangle]
pub extern "C" fn rsn_rep_tiers_stop(handle: &RepTiersHandle) {
    handle.stop();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_rep_tiers_tier(
    handle: &RepTiersHandle,
    representative: *const u8,
) -> u8 {
    handle.tier(&Account::from_ptr(representative)) as u8
}