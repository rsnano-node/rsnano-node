use super::ContextWrapper;
use crate::VoidPointerCallback;
use rsnano_node::utils::{ThreadPool, ThreadPoolImpl};
use std::{
    ffi::{c_char, c_void, CStr},
    ops::Deref,
    sync::Arc,
    time::Duration,
};

pub struct ThreadPoolHandle(pub Arc<dyn ThreadPool>);

impl Deref for ThreadPoolHandle {
    type Target = Arc<dyn ThreadPool>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[no_mangle]
pub unsafe extern "C" fn rsn_thread_pool_create(
    num_threads: usize,
    thread_name: *const c_char,
) -> *mut ThreadPoolHandle {
    let thread_name = CStr::from_ptr(thread_name).to_str().unwrap().to_owned();
    Box::into_raw(Box::new(ThreadPoolHandle(Arc::new(
        ThreadPoolImpl::create(num_threads, thread_name),
    ))))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_thread_pool_destroy(handle: *mut ThreadPoolHandle) {
    drop(Box::from_raw(handle));
}

#[no_mangle]
pub unsafe extern "C" fn rsn_thread_pool_stop(handle: *mut ThreadPoolHandle) {
    (*handle).0.stop()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_thread_pool_push_task(
    handle: *mut ThreadPoolHandle,
    task: VoidPointerCallback,
    context: *mut c_void,
    drop_context: VoidPointerCallback,
) {
    let context_wrapper = ContextWrapper::new(context, drop_context);
    let callback = Box::new(move || unsafe {
        task(context_wrapper.get_context());
    });
    (*handle).0.push_task(callback);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_thread_pool_add_delayed_task(
    handle: *mut ThreadPoolHandle,
    delay_ms: u64,
    task: VoidPointerCallback,
    context: *mut c_void,
    drop_context: VoidPointerCallback,
) {
    let context_wrapper = ContextWrapper::new(context, drop_context);
    let callback = Box::new(move || unsafe {
        task(context_wrapper.get_context());
    });
    (*handle)
        .0
        .add_delayed_task(Duration::from_millis(delay_ms), callback);
}
