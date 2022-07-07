use std::sync::{Arc, Mutex};

pub struct BufferHandle(Arc<Mutex<Vec<u8>>>);

#[no_mangle]
pub extern "C" fn rsn_buffer_create(len: usize) -> *mut BufferHandle {
    Box::into_raw(Box::new(BufferHandle(Arc::new(Mutex::new(vec![0; len])))))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_buffer_destroy(handle: *mut BufferHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_buffer_data(handle: *mut BufferHandle) -> *mut u8 {
    let ptr = (*handle).0.lock().unwrap().as_ptr();
    std::mem::transmute::<*const u8, *mut u8>(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn rsn_buffer_len(handle: *mut BufferHandle) -> usize {
    (*handle).0.lock().unwrap().len()
}
