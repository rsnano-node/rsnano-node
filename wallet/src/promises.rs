use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use rsnano_types::SavedBlock;

use super::WalletsError;

#[derive(Clone)]
pub struct BlockPromise {
    done_notification: Arc<Condvar>,
    state: Arc<Mutex<BlockPromiseState>>,
}

impl BlockPromise {
    pub fn new() -> Self {
        Self::with_state(BlockPromiseState::new())
    }

    pub fn new_failed(err: WalletsError) -> Self {
        Self::with_state(BlockPromiseState::new_failed(err))
    }

    fn with_state(state: BlockPromiseState) -> Self {
        Self {
            done_notification: Arc::new(Condvar::new()),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn set_result(&self, result: Result<SavedBlock, WalletsError>) {
        {
            let mut state = self.state.lock().unwrap();
            state.result = result;
            state.done = true;
        }
        self.done_notification.notify_all();
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Result<SavedBlock, WalletsError> {
        let result_guard = self.state.lock().unwrap();
        let (guard, timeout_result) = self
            .done_notification
            .wait_timeout_while(result_guard, timeout, |i| !i.done)
            .unwrap();

        if timeout_result.timed_out() {
            return Err(WalletsError::Generic);
        }

        guard.result.clone()
    }

    pub fn wait(&self) -> Result<SavedBlock, WalletsError> {
        let result_guard = self.state.lock().unwrap();
        self.done_notification
            .wait_while(result_guard, |i| !i.done)
            .unwrap()
            .result
            .clone()
    }
}

struct BlockPromiseState {
    done: bool,
    result: Result<SavedBlock, WalletsError>,
}

impl BlockPromiseState {
    fn new() -> Self {
        Self {
            done: false,
            result: Err(WalletsError::Generic),
        }
    }

    fn new_failed(err: WalletsError) -> Self {
        Self {
            done: true,
            result: Err(err),
        }
    }
}

#[derive(Clone)]
pub struct MultiBlockPromise {
    children: Vec<BlockPromise>,
}

impl MultiBlockPromise {
    pub fn new(children: Vec<BlockPromise>) -> Self {
        Self { children }
    }

    pub fn new_failed(error: WalletsError) -> Self {
        Self {
            children: vec![BlockPromise::new_failed(error)],
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn append(&mut self, other: MultiBlockPromise) {
        self.children.extend(other.children)
    }

    pub fn wait(&self) -> Result<Vec<SavedBlock>, WalletsError> {
        self.children.iter().map(|c| c.wait()).collect()
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Result<Vec<SavedBlock>, WalletsError> {
        let start = std::time::Instant::now();
        let mut results = Vec::new();

        for child in &self.children {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return Err(WalletsError::Generic);
            }
            let remaining = timeout - elapsed;
            results.push(child.wait_timeout(remaining)?);
        }

        Ok(results)
    }
}
