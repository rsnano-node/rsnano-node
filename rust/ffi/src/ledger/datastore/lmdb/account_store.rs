use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use crate::core::AccountInfoHandle;
use rsnano_core::Account;
use rsnano_store_lmdb::LmdbAccountStore;

use super::{iterator::LmdbIteratorHandle, TransactionHandle};

pub struct LmdbAccountStoreHandle(Arc<LmdbAccountStore>);

impl LmdbAccountStoreHandle {
    pub fn new(store: Arc<LmdbAccountStore>) -> *mut Self {
        Box::into_raw(Box::new(LmdbAccountStoreHandle(store)))
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_account_store_destroy(handle: *mut LmdbAccountStoreHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_account_store_put(
    handle: *mut LmdbAccountStoreHandle,
    txn: *mut TransactionHandle,
    account: *const u8,
    info: *const AccountInfoHandle,
) {
    let account = Account::from_ptr(account);
    let info = (*info).deref();
    (*handle).0.put((*txn).as_write_txn(), &account, info);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_account_store_get(
    handle: *mut LmdbAccountStoreHandle,
    txn: *mut TransactionHandle,
    account: *const u8,
    info: *mut AccountInfoHandle,
) -> bool {
    let account = Account::from_ptr(account);
    let info = (*info).deref_mut();
    match (*handle).0.get((*txn).as_txn(), &account) {
        Some(i) => {
            *info = i;
            true
        }
        None => false,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_account_store_begin_account(
    handle: *mut LmdbAccountStoreHandle,
    txn: *mut TransactionHandle,
    account: *const u8,
) -> *mut LmdbIteratorHandle {
    let account = Account::from_ptr(account);
    let iterator = (*handle).0.begin_account((*txn).as_txn(), &account);
    LmdbIteratorHandle::new2(iterator)
}

#[no_mangle]
pub unsafe extern "C" fn rsn_lmdb_account_store_begin(
    handle: *mut LmdbAccountStoreHandle,
    txn: *mut TransactionHandle,
) -> *mut LmdbIteratorHandle {
    let iterator = (*handle).0.begin((*txn).as_txn());
    LmdbIteratorHandle::new2(iterator)
}
