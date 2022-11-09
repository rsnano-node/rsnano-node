use num::FromPrimitive;
use std::ffi::c_void;
use std::sync::{Arc, RwLock};

use crate::core::{
    Account, Amount, BlockEnum, BlockHash, LazyBlockHash, PublicKey, RawKey, SendBlock,
    SendHashables, Signature,
};
use crate::ffi::utils::FfiStream;
use crate::ffi::FfiPropertyTreeReader;

use super::BlockHandle;

#[repr(C)]
pub struct SendBlockDto {
    pub previous: [u8; 32],
    pub destination: [u8; 32],
    pub balance: [u8; 16],
    pub signature: [u8; 64],
    pub work: u64,
}

#[repr(C)]
pub struct SendBlockDto2 {
    pub previous: [u8; 32],
    pub destination: [u8; 32],
    pub balance: [u8; 16],
    pub priv_key: [u8; 32],
    pub pub_key: [u8; 32],
    pub work: u64,
}

unsafe fn read_send_block<T>(handle: *const BlockHandle, f: impl FnOnce(&SendBlock) -> T) -> T {
    let block = (*handle).block.read().unwrap();
    match &*block {
        BlockEnum::Send(b) => f(b),
        _ => panic!("expected send block"),
    }
}

unsafe fn write_send_block<T>(
    handle: *mut BlockHandle,
    mut f: impl FnMut(&mut SendBlock) -> T,
) -> T {
    let mut block = (*handle).block.write().unwrap();
    match &mut *block {
        BlockEnum::Send(b) => f(b),
        _ => panic!("expected send block"),
    }
}

#[no_mangle]
pub extern "C" fn rsn_send_block_create(dto: &SendBlockDto) -> *mut BlockHandle {
    Box::into_raw(Box::new(BlockHandle {
        block: Arc::new(RwLock::new(BlockEnum::Send(SendBlock::from(dto)))),
    }))
}

#[no_mangle]
pub extern "C" fn rsn_send_block_create2(dto: &SendBlockDto2) -> *mut BlockHandle {
    let previous = BlockHash::from_bytes(dto.previous);
    let destination = Account::from_bytes(dto.destination);
    let balance = Amount::from_be_bytes(dto.balance);
    let private_key = RawKey::from_bytes(dto.priv_key);
    let public_key = PublicKey::from_bytes(dto.pub_key);
    let block = match SendBlock::new(
        &previous,
        &destination,
        &balance,
        &private_key,
        &public_key,
        dto.work,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("could not create send block: {}", e);
            return std::ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(BlockHandle {
        block: Arc::new(RwLock::new(BlockEnum::Send(block))),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_deserialize(stream: *mut c_void) -> *mut BlockHandle {
    let mut stream = FfiStream::new(stream);
    match SendBlock::deserialize(&mut stream) {
        Ok(block) => Box::into_raw(Box::new(BlockHandle {
            block: Arc::new(RwLock::new(BlockEnum::Send(block))),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_zero(handle: *mut BlockHandle) {
    write_send_block(handle, |b| b.zero());
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_destination(
    handle: *const BlockHandle,
    result: *mut [u8; 32],
) {
    (*result) = read_send_block(handle, |b| *b.hashables.destination.as_bytes());
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_destination_set(
    handle: *mut BlockHandle,
    destination: &[u8; 32],
) {
    let destination = Account::from_bytes(*destination);
    write_send_block(handle, |b| b.set_destination(destination));
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_previous_set(
    handle: *mut BlockHandle,
    previous: &[u8; 32],
) {
    let previous = BlockHash::from_bytes(*previous);
    write_send_block(handle, |b| b.set_previous(previous));
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_balance(handle: *const BlockHandle, result: *mut [u8; 16]) {
    (*result) = read_send_block(handle, |b| b.hashables.balance.to_be_bytes());
}

#[no_mangle]
pub unsafe extern "C" fn rsn_send_block_balance_set(handle: *mut BlockHandle, balance: &[u8; 16]) {
    let balance = Amount::from_be_bytes(*balance);
    write_send_block(handle, |b| b.set_balance(balance));
}

#[no_mangle]
pub extern "C" fn rsn_send_block_valid_predecessor(block_type: u8) -> bool {
    if let Some(block_type) = FromPrimitive::from_u8(block_type) {
        SendBlock::valid_predecessor(block_type)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn rsn_send_block_deserialize_json(ptree: *const c_void) -> *mut BlockHandle {
    let reader = FfiPropertyTreeReader::new(ptree);
    match SendBlock::deserialize_json(&reader) {
        Ok(block) => Box::into_raw(Box::new(BlockHandle {
            block: Arc::new(RwLock::new(BlockEnum::Send(block))),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

impl From<&SendBlockDto> for SendBlock {
    fn from(value: &SendBlockDto) -> Self {
        SendBlock {
            hashables: SendHashables::from(value),
            signature: Signature::from_bytes(value.signature),
            work: value.work,
            hash: LazyBlockHash::new(),
            sideband: None,
        }
    }
}

impl From<&SendBlockDto> for SendHashables {
    fn from(value: &SendBlockDto) -> Self {
        SendHashables {
            previous: BlockHash::from_bytes(value.previous),
            destination: Account::from_bytes(value.destination),
            balance: Amount::new(u128::from_be_bytes(value.balance)),
        }
    }
}
