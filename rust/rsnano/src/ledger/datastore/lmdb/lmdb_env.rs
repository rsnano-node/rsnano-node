use lmdb::{Environment, EnvironmentFlags};
use lmdb_sys::MDB_SUCCESS;
#[cfg(test)]
use std::ops::Deref;
use std::path::PathBuf;
use std::{
    ffi::{c_char, CStr},
    fs::{create_dir_all, set_permissions, Permissions},
    os::unix::prelude::PermissionsExt,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::config::{LmdbConfig, SyncStrategy, TxnTrackingConfig};
use crate::utils::Logger;
use crate::{
    ledger::datastore::{NullTxnCallbacks, TxnCallbacks, TxnTracker},
    memory_intensive_instrumentation,
    utils::PropertyTreeWriter,
};

use super::{LmdbReadTransaction, LmdbWriteTransaction};

#[derive(Default)]
pub struct EnvOptions {
    pub config: LmdbConfig,
    pub use_no_mem_init: bool,
}

pub struct LmdbEnv {
    pub environment: Environment,
    next_txn_id: AtomicU64,
    txn_tracker: Option<Arc<TxnTracker>>,
}

impl LmdbEnv {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::with_options(path, &EnvOptions::default())
    }

    pub fn with_options(path: impl AsRef<Path>, options: &EnvOptions) -> anyhow::Result<Self> {
        let env = Self {
            environment: Self::init(path, options)?,
            next_txn_id: AtomicU64::new(0),
            txn_tracker: None,
        };
        Ok(env)
    }

    pub fn with_tracking(
        path: &Path,
        options: &EnvOptions,
        tracking_cfg: TxnTrackingConfig,
        block_processor_batch_max_time: Duration,
        logger: Arc<dyn Logger>,
    ) -> anyhow::Result<Self> {
        let txn_tracker = if tracking_cfg.enable {
            Some(Arc::new(TxnTracker::new(
                logger,
                tracking_cfg,
                block_processor_batch_max_time,
            )))
        } else {
            None
        };

        let result = Self {
            environment: Self::init(path, options)?,
            next_txn_id: AtomicU64::new(0),
            txn_tracker,
        };
        Ok(result)
    }

    pub fn init(path: impl AsRef<Path>, options: &EnvOptions) -> anyhow::Result<Environment> {
        let path = path.as_ref();
        try_create_parent_dir(path)?;
        let mut map_size = options.config.map_size;
        let max_instrumented_map_size = 16 * 1024 * 1024;
        if memory_intensive_instrumentation() && map_size > max_instrumented_map_size {
            // In order to run LMDB under Valgrind, the maximum map size must be smaller than half your available RAM
            map_size = max_instrumented_map_size;
        }

        // It seems if there's ever more threads than mdb_env_set_maxreaders has read slots available, we get failures on transaction creation unless MDB_NOTLS is specified
        // This can happen if something like 256 io_threads are specified in the node config
        // MDB_NORDAHEAD will allow platforms that support it to load the DB in memory as needed.
        // MDB_NOMEMINIT prevents zeroing malloc'ed pages. Can provide improvement for non-sensitive data but may make memory checkers noisy (e.g valgrind).
        let mut environment_flags = EnvironmentFlags::NO_SUB_DIR
            | EnvironmentFlags::NO_TLS
            | EnvironmentFlags::NO_READAHEAD;
        if options.config.sync == SyncStrategy::NosyncSafe {
            environment_flags |= EnvironmentFlags::NO_META_SYNC;
        } else if options.config.sync == SyncStrategy::NosyncUnsafe {
            environment_flags |= EnvironmentFlags::NO_SYNC;
        } else if options.config.sync == SyncStrategy::NosyncUnsafeLargeMemory {
            environment_flags |= EnvironmentFlags::NO_SYNC
                | EnvironmentFlags::WRITE_MAP
                | EnvironmentFlags::MAP_ASYNC;
        }

        if !memory_intensive_instrumentation() && options.use_no_mem_init {
            environment_flags |= EnvironmentFlags::NO_MEM_INIT;
        }

        let env = Environment::new()
            .set_max_dbs(options.config.max_databases)
            .set_map_size(map_size)
            .set_flags(environment_flags)
            .open_with_permissions(path, 0o600)?;
        Ok(env)
    }

    pub fn tx_begin_read(&self) -> lmdb::Result<LmdbReadTransaction> {
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::Relaxed);
        LmdbReadTransaction::new(txn_id, &self.environment, self.create_txn_callbacks())
    }

    pub fn tx_begin_write(&self) -> lmdb::Result<LmdbWriteTransaction> {
        // For IO threads, we do not want them to block on creating write transactions.
        debug_assert!(std::thread::current().name() != Some("I/O"));
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::Relaxed);
        LmdbWriteTransaction::new(txn_id, &self.environment, self.create_txn_callbacks())
    }

    pub fn file_path(&self) -> anyhow::Result<PathBuf> {
        let mut path: *const c_char = std::ptr::null();
        let status = unsafe { lmdb_sys::mdb_env_get_path(self.environment.env(), &mut path) };
        if status != MDB_SUCCESS {
            bail!("could not get env path");
        }
        let source_path: PathBuf = unsafe { CStr::from_ptr(path) }.to_str()?.into();
        Ok(source_path)
    }

    fn create_txn_callbacks(&self) -> Arc<dyn TxnCallbacks> {
        match &self.txn_tracker {
            Some(tracker) => Arc::clone(tracker) as Arc<dyn TxnCallbacks>,
            None => Arc::new(NullTxnCallbacks::new()),
        }
    }

    pub fn serialize_txn_tracker(
        &self,
        json: &mut dyn PropertyTreeWriter,
        min_read_time: Duration,
        min_write_time: Duration,
    ) -> anyhow::Result<()> {
        match &self.txn_tracker {
            Some(tracker) => tracker.serialize_json(json, min_read_time, min_write_time),
            None => Ok(()),
        }
    }
}

fn try_create_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if parent != Path::new("") {
            if !parent.is_dir() {
                create_dir_all(parent)?;
                set_permissions(parent, Permissions::from_mode(0o700))?;
            }
        }
    }
    Ok(())
}

impl Drop for LmdbEnv {
    fn drop(&mut self) {
        let _ = self.environment.sync(true);
    }
}

#[cfg(test)]
pub struct TestDbFile {
    pub path: PathBuf,
}

#[cfg(test)]
impl TestDbFile {
    fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: Path::new("/tmp").join(path),
        }
    }

    pub fn random() -> Self {
        Self::new(Self::temp_file_name())
    }

    fn temp_file_name() -> PathBuf {
        PathBuf::from(format!("{}.ldb", uuid::Uuid::new_v4().to_simple()))
    }

    fn lock_file_path(&self) -> PathBuf {
        let mut lock_file_path = self.path.parent().unwrap().to_owned();
        let mut fname = self.path.file_name().unwrap().to_os_string();
        fname.push("-lock");
        lock_file_path.push(fname);
        lock_file_path
    }
}

#[cfg(test)]
impl Drop for TestDbFile {
    fn drop(&mut self) {
        if self.path.exists() {
            std::fs::remove_file(&self.path).unwrap();
            let lock_file = self.lock_file_path();
            if lock_file.exists() {
                std::fs::remove_file(&lock_file).unwrap();
            }

            if let Some(parent) = self.path.parent() {
                if parent != Path::new("/tmp") {
                    std::fs::remove_dir(parent).unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) struct TestLmdbEnv {
    env: Arc<LmdbEnv>,
    file: TestDbFile,
}

#[cfg(test)]
impl TestLmdbEnv {
    pub(crate) fn new() -> Self {
        let file = TestDbFile::random();
        let env = Arc::new(LmdbEnv::new(&file.path).unwrap());
        Self { file, env }
    }

    pub(crate) fn env(&self) -> Arc<LmdbEnv> {
        self.env.clone()
    }
}

#[cfg(test)]
impl Deref for TestLmdbEnv {
    type Target = LmdbEnv;

    fn deref(&self) -> &Self::Target {
        &self.env
    }
}

#[cfg(test)]
mod tests {
    use crate::ledger::datastore::ReadTransaction;

    use super::*;

    mod test_db_file {
        use super::*;

        #[test]
        fn tmp_test() {
            let path = Path::new("foo.tmp");
            assert_eq!(path.parent(), Some(Path::new("")));
            assert_eq!(Path::new(""), Path::new(""))
        }

        #[test]
        fn dont_panic_when_file_not_found() {
            let file = TestDbFile::new("does-not-exist.ldb");
            drop(file)
        }

        #[test]
        fn delete_file_when_dropped() {
            let file = TestDbFile::new("drop-test.ldb");
            let mut lock_file_path = file.path.parent().unwrap().to_owned();
            lock_file_path.push("drop-test.ldb-lock");
            std::fs::write(&file.path, "foo").unwrap();
            std::fs::write(&lock_file_path, "foo").unwrap();
            let path = file.path.clone();
            drop(file);
            assert_eq!(path.exists(), false, "db file was not deleted");
            assert_eq!(lock_file_path.exists(), false, "lock file was not deleted");
        }

        #[test]
        fn delete_dir_when_dropped() {
            let file = TestDbFile::new("drop-dir/db.ldb");
            std::fs::create_dir(file.path.parent().unwrap()).unwrap();
            std::fs::write(&file.path, "foo").unwrap();
            let path = file.path.clone();
            drop(file);
            assert_eq!(path.exists(), false);
            assert_eq!(path.parent().unwrap().exists(), false);
        }

        #[test]
        fn tmp_file_name() {
            let filename = TestDbFile::temp_file_name();
            assert_eq!(filename.extension().unwrap(), "ldb");
            assert_eq!(filename.file_stem().unwrap().len(), 32);
            assert_ne!(TestDbFile::temp_file_name(), TestDbFile::temp_file_name());
        }
    }

    #[test]
    fn first_test() {
        let db_file = TestDbFile::new("foo.ldb");
        let env = LmdbEnv::new(&db_file.path).unwrap();
        let mut txn = env.tx_begin_read().unwrap();
        txn.refresh();
        assert!(true)
    }
}
