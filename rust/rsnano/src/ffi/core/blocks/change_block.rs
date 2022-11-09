use std::ffi::c_void;
use std::sync::{Arc, RwLock};

use crate::core::{
    Account, BlockEnum, BlockHash, ChangeBlock, ChangeHashables, LazyBlockHash, PublicKey, RawKey,
    Signature,
};
use crate::ffi::utils::FfiStream;
use crate::ffi::FfiPropertyTreeReader;

use super::BlockHandle;

#[repr(C)]
pub struct ChangeBlockDto {
    pub work: u64,
    pub signature: [u8; 64],
    pub previous: [u8; 32],
    pub representative: [u8; 32],
}

#[repr(C)]
pub struct ChangeBlockDto2 {
    pub previous: [u8; 32],
    pub representative: [u8; 32],
    pub priv_key: [u8; 32],
    pub pub_key: [u8; 32],
    pub work: u64,
}

#[no_mangle]
pub extern "C" fn rsn_change_block_create(dto: &ChangeBlockDto) -> *mut BlockHandle {
    Box::into_raw(Box::new(BlockHandle {
        block: Arc::new(RwLock::new(BlockEnum::Change(ChangeBlock {
            work: dto.work,
            signature: Signature::from_bytes(dto.signature),
            hashables: ChangeHashables {
                previous: BlockHash::from_bytes(dto.previous),
                representative: Account::from_bytes(dto.representative),
            },
            hash: LazyBlockHash::new(),
            sideband: None,
        }))),
    }))
}

#[no_mangle]
pub extern "C" fn rsn_change_block_create2(dto: &ChangeBlockDto2) -> *mut BlockHandle {
    let block = match ChangeBlock::new(
        BlockHash::from_bytes(dto.previous),
        Account::from_bytes(dto.representative),
        &RawKey::from_bytes(dto.priv_key),
        &PublicKey::from_bytes(dto.pub_key),
        dto.work,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("could not create change block: {}", e);
            return std::ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(BlockHandle {
        block: Arc::new(RwLock::new(BlockEnum::Change(block))),
    }))
}

unsafe fn read_change_block<T>(handle: *const BlockHandle, f: impl FnOnce(&ChangeBlock) -> T) -> T {
    let block = (*handle).block.read().unwrap();
    match &*block {
        BlockEnum::Change(b) => f(b),
        _ => panic!("expected change block"),
    }
}

unsafe fn write_change_block<T>(
    handle: *mut BlockHandle,
    mut f: impl FnMut(&mut ChangeBlock) -> T,
) -> T {
    let mut block = (*handle).block.write().unwrap();
    match &mut *block {
        BlockEnum::Change(b) => f(b),
        _ => panic!("expected change block"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_change_block_previous_set(
    handle: *mut BlockHandle,
    source: &[u8; 32],
) {
    write_change_block(handle, |b| {
        b.hashables.previous = BlockHash::from_bytes(*source)
    });
}

#[no_mangle]
pub unsafe extern "C" fn rsn_change_block_representative(
    handle: *const BlockHandle,
    result: *mut [u8; 32],
) {
    (*result) = read_change_block(handle, |b| *b.hashables.representative.as_bytes());
}

#[no_mangle]
pub unsafe extern "C" fn rsn_change_block_representative_set(
    handle: *mut BlockHandle,
    representative: &[u8; 32],
) {
    write_change_block(handle, |b| {
        b.hashables.representative = Account::from_bytes(*representative)
    });
}

#[no_mangle]
pub extern "C" fn rsn_change_block_size() -> usize {
    ChangeBlock::serialized_size()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_change_block_deserialize(stream: *mut c_void) -> *mut BlockHandle {
    let mut stream = FfiStream::new(stream);
    match ChangeBlock::deserialize(&mut stream) {
        Ok(block) => Box::into_raw(Box::new(BlockHandle {
            block: Arc::new(RwLock::new(BlockEnum::Change(block))),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rsn_change_block_deserialize_json(ptree: *const c_void) -> *mut BlockHandle {
    let reader = FfiPropertyTreeReader::new(ptree);
    match ChangeBlock::deserialize_json(&reader) {
        Ok(block) => Box::into_raw(Box::new(BlockHandle {
            block: Arc::new(RwLock::new(BlockEnum::Change(block))),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}
