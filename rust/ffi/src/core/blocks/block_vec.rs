use super::BlockHandle;
use rsnano_core::BlockEnum;
use std::sync::Arc;

pub struct BlockVecHandle(pub Vec<Arc<BlockEnum>>);

impl BlockVecHandle {
    pub fn new(blocks: Vec<Arc<BlockEnum>>) -> *mut BlockVecHandle {
        Box::into_raw(Box::new(BlockVecHandle(blocks)))
    }

    pub fn new2(mut blocks: Vec<BlockEnum>) -> *mut BlockVecHandle {
        Self::new(blocks.drain(..).map(|b| Arc::new(b)).collect())
    }
}

#[no_mangle]
pub extern "C" fn rsn_block_vec_create() -> *mut BlockVecHandle {
    Box::into_raw(Box::new(BlockVecHandle(Vec::new())))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_block_vec_destroy(handle: *mut BlockVecHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_block_vec_erase_last(handle: *mut BlockVecHandle, count: usize) {
    (*handle).0.truncate((*handle).0.len() - count);
}

#[no_mangle]
pub extern "C" fn rsn_block_vec_push_back(handle: &mut BlockVecHandle, block: &BlockHandle) {
    handle.0.push(Arc::clone(&block))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_block_vec_size(handle: *mut BlockVecHandle) -> usize {
    (*handle).0.len()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_block_vec_clear(handle: *mut BlockVecHandle) {
    (*handle).0.clear();
}

#[no_mangle]
pub unsafe extern "C" fn rsn_block_vec_get_block(
    handle: *mut BlockVecHandle,
    index: usize,
) -> *mut BlockHandle {
    let block = (*handle).0.get(index).unwrap().clone();
    Box::into_raw(Box::new(BlockHandle(block)))
}
