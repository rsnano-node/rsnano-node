mod account_store;
mod block_store;
mod confirmation_height_store;
mod final_vote_store;
mod frontier_store;
mod iterator;
mod lmdb_env;
mod online_weight_store;
mod peer_store;
mod pending_store;
mod pruned_store;
mod store;
mod unchecked_store;
mod version_store;
mod wallet_store;
mod wallets;

use std::{ffi::c_void, ops::Deref};
pub use store::LmdbStoreHandle;

use crate::{
    ffi::VoidPointerCallback,
    ledger::datastore::{
        lmdb::{LmdbReadTransaction, LmdbWriteTransaction},
        ReadTransaction, Transaction, TxnCallbacks, WriteTransaction,
    },
};

pub struct TransactionHandle(TransactionType);

impl TransactionHandle {
    pub fn new(txn_type: TransactionType) -> *mut TransactionHandle {
        Box::into_raw(Box::new(TransactionHandle(txn_type)))
    }

    pub fn as_read_txn_mut(&mut self) -> &mut dyn ReadTransaction {
        match &mut self.0 {
            TransactionType::Read(tx) => tx,
            _ => panic!("invalid tx type"),
        }
    }

    pub fn as_read_txn(&mut self) -> &dyn ReadTransaction {
        match &mut self.0 {
            TransactionType::Read(tx) => tx,
            TransactionType::ReadRef(tx) => *tx,
            _ => panic!("invalid tx type"),
        }
    }

    pub fn as_write_txn(&mut self) -> &mut dyn WriteTransaction {
        match &mut self.0 {
            TransactionType::Write(tx) => tx,
            _ => panic!("invalid tx type"),
        }
    }

    pub fn as_txn(&self) -> &dyn Transaction {
        match &self.0 {
            TransactionType::Read(t) => t,
            TransactionType::Write(t) => t,
            TransactionType::ReadRef(t) => t.txn(),
        }
    }
}

impl Deref for TransactionHandle {
    type Target = TransactionType;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub enum TransactionType {
    Read(LmdbReadTransaction),
    ReadRef(&'static dyn ReadTransaction),
    Write(LmdbWriteTransaction),
}

struct FfiCallbacksWrapper {
    handle: *mut c_void,
}

impl FfiCallbacksWrapper {
    fn new(handle: *mut c_void) -> Self {
        Self { handle }
    }
}

impl TxnCallbacks for FfiCallbacksWrapper {
    fn txn_start(&self, txn_id: u64, is_write: bool) {
        unsafe { TXN_START.expect("TXN_START")(self.handle, txn_id, is_write) }
    }

    fn txn_end(&self, txn_id: u64, _is_write: bool) {
        unsafe { TXN_END.expect("TXN_END")(self.handle, txn_id) }
    }
}

impl Drop for FfiCallbacksWrapper {
    fn drop(&mut self) {
        unsafe { TXN_CALLBACKS_DESTROY.expect("TXN_CALLBACKS_DESTROY missing")(self.handle) }
    }
}

static mut TXN_CALLBACKS_DESTROY: Option<VoidPointerCallback> = None;
pub type TxnStartCallback = unsafe extern "C" fn(*mut c_void, u64, bool);
pub type TxnEndCallback = unsafe extern "C" fn(*mut c_void, u64);
static mut TXN_START: Option<TxnStartCallback> = None;
static mut TXN_END: Option<TxnEndCallback> = None;

#[no_mangle]
pub unsafe extern "C" fn rsn_callback_txn_callbacks_destroy(f: VoidPointerCallback) {
    TXN_CALLBACKS_DESTROY = Some(f);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_callback_txn_callbacks_start(f: TxnStartCallback) {
    TXN_START = Some(f);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_callback_txn_callbacks_end(f: TxnEndCallback) {
    TXN_END = Some(f);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_read_txn_destroy(handle: *mut TransactionHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_read_txn_reset(handle: *mut TransactionHandle) {
    (*handle).as_read_txn_mut().reset();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_read_txn_renew(handle: *mut TransactionHandle) {
    (*handle).as_read_txn_mut().renew();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_read_txn_refresh(handle: *mut TransactionHandle) {
    (*handle).as_read_txn_mut().refresh();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_write_txn_destroy(handle: *mut TransactionHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_write_txn_commit(handle: *mut TransactionHandle) {
    (*handle).as_write_txn().commit();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_write_txn_renew(handle: *mut TransactionHandle) {
    (*handle).as_write_txn().renew();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_write_txn_refresh(handle: *mut TransactionHandle) {
    (*handle).as_write_txn().refresh();
}
