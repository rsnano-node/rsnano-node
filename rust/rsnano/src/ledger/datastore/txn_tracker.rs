use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use backtrace::Backtrace;

use crate::{
    config::TxnTrackingConfig,
    utils::{Logger, PropertyTreeWriter},
};

pub trait TxnCallbacks {
    fn txn_start(&self, txn_id: u64, is_write: bool);
    fn txn_end(&self, txn_id: u64, is_write: bool);
}

pub struct NullTxnCallbacks {}

impl NullTxnCallbacks {
    pub fn new() -> Self {
        Self {}
    }
}

impl TxnCallbacks for NullTxnCallbacks {
    fn txn_start(&self, _txn_id: u64, _is_write: bool) {}

    fn txn_end(&self, _txn_id: u64, _is_write: bool) {}
}

pub struct TxnTracker {
    stats: Mutex<HashMap<u64, TxnStats>>,
    logger: Arc<dyn Logger>,
    config: TxnTrackingConfig,
    block_processor_batch_max_time: Duration,
}

impl TxnTracker {
    pub fn new(
        logger: Arc<dyn Logger>,
        config: TxnTrackingConfig,
        block_processor_batch_max_time: Duration,
    ) -> Self {
        Self {
            logger,
            config,
            block_processor_batch_max_time,
            stats: Mutex::new(HashMap::new()),
        }
    }

    pub fn add(&self, txn_id: u64, is_write: bool) {
        let mut stats = self.stats.lock().unwrap();
        stats.insert(
            txn_id,
            TxnStats {
                txn_id,
                is_write,
                start: Instant::now(),
                thread_name: std::thread::current().name().map(|s| s.to_owned()),
                stacktrace: Backtrace::new_unresolved(),
            },
        );
    }

    pub fn erase(&self, txn_id: u64, _is_write: bool) {
        let entry = {
            let mut stats = self.stats.lock().unwrap();
            stats.remove(&txn_id)
        };

        if let Some(mut entry) = entry {
            self.log_if_held_long_enough(&mut entry);
        }
    }

    fn log_if_held_long_enough(&self, txn: &mut TxnStats) {
        // Only log these transactions if they were held for longer than the min_read_txn_time/min_write_txn_time config values
        let time_open = txn.start.elapsed();
        // Reduce noise in log files by removing any entries from the block processor (if enabled) which are less than the max batch time (+ a few second buffer) because these are expected writes during bootstrapping.
        let is_below_max_time =
            time_open <= (self.block_processor_batch_max_time + Duration::from_secs(3));
        let is_blk_processing_thread = txn.thread_name.as_deref() == Some("Blck processing");
        if self.config.ignore_writes_below_block_processor_max_time
            && is_blk_processing_thread
            && txn.is_write
            && is_below_max_time
        {
            return;
        }

        if (txn.is_write
            && time_open >= Duration::from_millis(self.config.min_write_txn_time_ms as u64))
            || (!txn.is_write
                && time_open >= Duration::from_millis(self.config.min_read_txn_time_ms as u64))
        {
            let txn_type = if txn.is_write { "write lock" } else { "read" };
            txn.stacktrace.resolve();
            self.logger.always_log(&format!(
                "{}ms {} held on thread {}\n{:?}",
                time_open.as_millis(),
                txn_type,
                txn.thread_name.as_deref().unwrap_or("unnamed"),
                txn.stacktrace
            ));
        }
    }

    pub fn serialize_json(
        &self,
        json: &mut dyn PropertyTreeWriter,
        min_read_time: Duration,
        min_write_time: Duration,
    ) -> anyhow::Result<()> {
        // Copying is cheap compared to generating the stack trace strings, so reduce time holding the mutex
        let mut copy_stats: Vec<TxnStats> = Vec::new();
        let mut are_writes: Vec<bool> = Vec::new();
        {
            let guard = self.stats.lock().unwrap();
            copy_stats.reserve(guard.len());
            are_writes.reserve(guard.len());

            for i in guard.values() {
                copy_stats.push(i.clone());
                are_writes.push(i.is_write);
            }
        }

        // Get the time difference now as creating stacktraces (Debug/Windows for instance) can take a while so results won't be as accurate
        let times_since_start: Vec<_> = copy_stats.iter().map(|i| i.start.elapsed()).collect();

        for i in 0..times_since_start.len() {
            let stat = &mut copy_stats[i];
            let time_held_open = times_since_start[i];

            if (are_writes[i] && time_held_open >= min_write_time)
                || (!are_writes[i] && time_held_open >= min_read_time)
            {
                stat.stacktrace.resolve();
                let mut mdb_lock_config = json.new_writer();

                mdb_lock_config
                    .put_string("thread", stat.thread_name.as_deref().unwrap_or("unnamed"))?;
                mdb_lock_config.put_u64("time_held_open", time_held_open.as_millis() as u64)?;
                mdb_lock_config.put_string("write", &are_writes[i].to_string())?;

                let mut stacktrace_config = json.new_writer();

                for frame in stat.stacktrace.frames() {
                    let mut frame_json = json.new_writer();
                    for symbol in frame.symbols() {
                        frame_json.put_string(
                            "name",
                            symbol
                                .name()
                                .map(|n| n.as_str().unwrap_or("unknown"))
                                .unwrap_or("unknown"),
                        )?;
                        frame_json.put_string(
                            "address",
                            &format!("{:016x}", symbol.addr().map(|a| a as usize).unwrap_or(0)),
                        )?;
                        frame_json.put_string(
                            "source_file",
                            symbol
                                .filename()
                                .map(|f| f.to_str().unwrap_or("invalid"))
                                .unwrap_or("unknown"),
                        )?;
                        frame_json.put_u64("source_line", symbol.lineno().unwrap_or(0) as u64)?;
                        stacktrace_config.push_back("", frame_json.as_ref());
                    }
                }

                mdb_lock_config.put_child("stacktrace", stacktrace_config.as_ref());
                json.push_back("", mdb_lock_config.as_ref());
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct TxnStats {
    txn_id: u64,
    is_write: bool,
    thread_name: Option<String>,
    start: Instant,
    stacktrace: Backtrace,
}

impl TxnCallbacks for TxnTracker {
    fn txn_start(&self, txn_id: u64, is_write: bool) {
        self.add(txn_id, is_write);
    }

    fn txn_end(&self, txn_id: u64, is_write: bool) {
        self.erase(txn_id, is_write);
    }
}
