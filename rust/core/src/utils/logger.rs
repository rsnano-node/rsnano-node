use std::ffi::c_void;

pub trait Logger: Send + Sync {
    fn try_log(&self, message: &str) -> bool;
    fn always_log(&self, message: &str);
    fn handle(&self) -> *mut c_void;
}
pub struct NullLogger {}

impl NullLogger {
    pub fn new() -> Self {
        Self {}
    }
}

impl Logger for NullLogger {
    fn try_log(&self, _message: &str) -> bool {
        false
    }

    fn always_log(&self, _message: &str) {}

    fn handle(&self) -> *mut c_void {
        std::ptr::null_mut()
    }
}

pub struct ConsoleLogger {}

impl ConsoleLogger {
    pub fn new() -> Self {
        Self {}
    }
}

impl Logger for ConsoleLogger {
    fn try_log(&self, message: &str) -> bool {
        println!("{}", message);
        true
    }

    fn always_log(&self, message: &str) {
        println!("{}", message);
    }

    fn handle(&self) -> *mut c_void {
        std::ptr::null_mut()
    }
}
