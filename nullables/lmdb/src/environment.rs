use std::{
    ffi::CString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use lmdb::{DatabaseFlags, EnvironmentFlags, Stat};
use lmdb_sys::{MDB_CP_COMPACT, MDB_SUCCESS, MDB_env};

use super::{ConfiguredDatabase, LmdbDatabase};
use crate::{ConfiguredDatabaseBuilder, ReadTransaction, Result, WriteTransaction};
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentOptions {
    pub max_dbs: u32,
    pub map_size: usize,
    pub flags: EnvironmentFlags,
    pub path: PathBuf,
}

pub struct NullLmdbEnvBuilder {
    env_builder: EnvironmentStubBuilder,
}

impl NullLmdbEnvBuilder {
    pub fn database(self, name: impl Into<String>, dbi: LmdbDatabase) -> NullDatabaseBuilder {
        NullDatabaseBuilder {
            db_builder: ConfiguredDatabaseBuilder::new(name, dbi, self.env_builder),
        }
    }

    pub fn configured_database(mut self, db: ConfiguredDatabase) -> Self {
        self.env_builder = self.env_builder.configured_database(db);
        self
    }

    pub fn build(self) -> LmdbEnvironment {
        self.env_builder.build()
    }
}

pub struct NullDatabaseBuilder {
    db_builder: ConfiguredDatabaseBuilder,
}

impl NullDatabaseBuilder {
    pub fn entry(mut self, key: &[u8], value: &[u8]) -> Self {
        self.db_builder = self.db_builder.entry(key, value);
        self
    }

    pub fn error(mut self, key: &[u8], error: lmdb::Error) -> Self {
        self.db_builder = self.db_builder.error(key, error);
        self
    }

    pub fn build(self) -> NullLmdbEnvBuilder {
        NullLmdbEnvBuilder {
            env_builder: self.db_builder.finish(),
        }
    }
}

#[derive(Default)]
pub struct LmdbEnvironmentFactory {
    is_nulled: bool,
    create_listener: OutputListenerMt<EnvironmentOptions>,
}

impl LmdbEnvironmentFactory {
    pub fn new_null() -> Self {
        Self {
            is_nulled: true,
            create_listener: OutputListenerMt::default(),
        }
    }

    pub fn track(&self) -> Arc<OutputTrackerMt<EnvironmentOptions>> {
        self.create_listener.track()
    }

    pub fn create(&self, options: EnvironmentOptions) -> Result<LmdbEnvironment> {
        self.create_listener.emit(options.clone());
        if self.is_nulled {
            Ok(LmdbEnvironment::new_null_with_options(options))
        } else {
            LmdbEnvironment::create(options)
        }
    }
}

pub struct LmdbEnvironment {
    env_strategy: EnvironmentStrategy,
    path: PathBuf,
}

impl LmdbEnvironment {
    pub fn new_null() -> Self {
        Self::new_null_with_data(Vec::new())
    }

    pub fn new_null_with_options(options: EnvironmentOptions) -> Self {
        Self {
            env_strategy: EnvironmentStrategy::Nulled(EnvironmentStub::new()),
            path: options.path.to_path_buf(),
        }
    }

    pub fn new_null_with_data(databases: Vec<ConfiguredDatabase>) -> Self {
        Self {
            env_strategy: EnvironmentStrategy::Nulled(EnvironmentStub::new_with(databases)),
            path: "/nulled/ledger.ldb".into(),
        }
    }

    pub fn null_builder() -> NullLmdbEnvBuilder {
        NullLmdbEnvBuilder {
            env_builder: EnvironmentStubBuilder::default(),
        }
    }

    pub fn create(options: EnvironmentOptions) -> Result<Self> {
        let filepath = options.path.clone();
        let env_wrapper = EnvironmentWrapper::build(options)?;
        Ok(Self {
            env_strategy: EnvironmentStrategy::Real(env_wrapper),
            path: filepath,
        })
    }

    pub fn begin_read(&self) -> ReadTransaction {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.begin_read(),
            EnvironmentStrategy::Nulled(s) => s.begin_read(),
        }
    }

    pub fn begin_write(&self) -> WriteTransaction {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.begin_write(),
            EnvironmentStrategy::Nulled(s) => s.begin_write(),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.path
    }

    pub fn sync(&self) -> Result<()> {
        if let EnvironmentStrategy::Real(s) = &self.env_strategy {
            s.sync(true)?;
        }
        Ok(())
    }

    pub fn copy_db(&self, destination: &Path) -> Result<()> {
        if let EnvironmentStrategy::Real(_) = &self.env_strategy {
            let c_path = CString::new(destination.as_os_str().to_str().unwrap()).unwrap();
            let status =
                unsafe { lmdb_sys::mdb_env_copy2(self.env(), c_path.as_ptr(), MDB_CP_COMPACT) };
            if status == MDB_SUCCESS {
                Ok(())
            } else {
                Err(lmdb::Error::Other(status))
            }
        } else {
            Ok(())
        }
    }

    pub fn create_db(&self, name: Option<&str>, flags: DatabaseFlags) -> Result<LmdbDatabase> {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.create_db(name, flags),
            EnvironmentStrategy::Nulled(s) => s.create_db(name, flags),
        }
    }

    pub fn open_db(&self, name: Option<&str>) -> Result<LmdbDatabase> {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.open_db(name),
            EnvironmentStrategy::Nulled(s) => s.open_db(name),
        }
    }

    pub fn stat(&self) -> Result<Stat> {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.stat(),
            EnvironmentStrategy::Nulled(s) => s.stat(),
        }
    }

    pub fn refresh(&self, txn: WriteTransaction) -> WriteTransaction {
        txn.commit();
        self.begin_write()
    }

    fn env(&self) -> *mut MDB_env {
        match &self.env_strategy {
            EnvironmentStrategy::Real(s) => s.env(),
            EnvironmentStrategy::Nulled(_) => unimplemented!(),
        }
    }
}

enum EnvironmentStrategy {
    Nulled(EnvironmentStub),
    Real(EnvironmentWrapper),
}

struct EnvironmentWrapper(lmdb::Environment);

impl EnvironmentWrapper {
    fn build(options: EnvironmentOptions) -> lmdb::Result<Self> {
        let env = lmdb::Environment::new()
            .set_max_dbs(options.max_dbs)
            .set_map_size(options.map_size)
            .set_flags(options.flags)
            .open_with_permissions(&options.path, 0o600.try_into().unwrap())?;
        Ok(Self(env))
    }

    fn begin_read(&self) -> ReadTransaction {
        self.0
            .begin_ro_txn()
            .map(|txn| {
                // todo: don't use static life time
                let txn = unsafe {
                    std::mem::transmute::<lmdb::RoTransaction<'_>, lmdb::RoTransaction<'static>>(
                        txn,
                    )
                };
                ReadTransaction::new(txn)
            })
            .expect("Could not create LMDB read-only transaction")
    }

    fn begin_write(&self) -> WriteTransaction {
        self.0
            .begin_rw_txn()
            .map(|txn| {
                // todo: don't use static life time
                let txn = unsafe {
                    std::mem::transmute::<lmdb::RwTransaction<'_>, lmdb::RwTransaction<'static>>(
                        txn,
                    )
                };
                WriteTransaction::new(txn)
            })
            .expect("Could not create LMDB read-write transaction")
    }

    fn create_db(&self, name: Option<&str>, flags: DatabaseFlags) -> lmdb::Result<LmdbDatabase> {
        self.0.create_db(name, flags).map(LmdbDatabase::new)
    }

    fn env(&self) -> *mut MDB_env {
        self.0.env()
    }

    fn open_db(&self, name: Option<&str>) -> lmdb::Result<LmdbDatabase> {
        self.0.open_db(name).map(LmdbDatabase::new)
    }

    fn sync(&self, force: bool) -> lmdb::Result<()> {
        self.0.sync(force)
    }

    fn stat(&self) -> lmdb::Result<Stat> {
        self.0.stat()
    }
}

struct EnvironmentStub {
    databases: Arc<Mutex<Vec<ConfiguredDatabase>>>,
    write_active: Arc<AtomicBool>,
}

impl EnvironmentStub {
    fn new() -> Self {
        Self::new_with(Vec::new())
    }

    fn new_with(databases: Vec<ConfiguredDatabase>) -> Self {
        Self {
            databases: Arc::new(Mutex::new(databases)),
            write_active: Arc::new(AtomicBool::new(false)),
        }
    }

    fn begin_read(&self) -> ReadTransaction {
        ReadTransaction::new_null(self.databases.lock().unwrap().clone())
    }

    fn begin_write(&self) -> WriteTransaction {
        WriteTransaction::new_null(self.databases.clone(), self.write_active.clone())
    }

    fn create_db(&self, name: Option<&str>, _flags: DatabaseFlags) -> lmdb::Result<LmdbDatabase> {
        let mut guard = self.databases.lock().unwrap();
        let db_name = name.unwrap_or("");
        if let Some(db) = guard.iter().find(|x| x.db_name == db_name) {
            return Ok(db.dbi);
        }

        let dbi = create_dbi(&guard);
        guard.push(ConfiguredDatabase::new(dbi, db_name.to_owned()));
        Ok(dbi)
    }

    fn open_db(&self, name: Option<&str>) -> lmdb::Result<LmdbDatabase> {
        let db_name = name.unwrap_or("");
        self.databases
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.db_name == db_name)
            .map(|x| x.dbi)
            .ok_or(lmdb::Error::NotFound)
    }

    fn stat(&self) -> lmdb::Result<Stat> {
        todo!()
    }
}

fn create_dbi(guard: &std::sync::MutexGuard<'_, Vec<ConfiguredDatabase>>) -> LmdbDatabase {
    let id = guard.iter().map(|i| i.dbi.as_nulled()).max().unwrap_or(41) + 1;
    LmdbDatabase::new_null(id)
}

#[derive(Default)]
pub struct EnvironmentStubBuilder {
    databases: Vec<ConfiguredDatabase>,
}

impl EnvironmentStubBuilder {
    pub fn database(self, name: impl Into<String>, dbi: LmdbDatabase) -> ConfiguredDatabaseBuilder {
        ConfiguredDatabaseBuilder::new(name, dbi, self)
    }

    pub fn configured_database(mut self, db: ConfiguredDatabase) -> Self {
        if self
            .databases
            .iter()
            .any(|x| x.dbi == db.dbi || x.db_name == db.db_name)
        {
            panic!(
                "trying to duplicated database for {} / {}",
                db.dbi.as_nulled(),
                db.db_name
            );
        }
        self.databases.push(db);
        self
    }

    pub fn build(self) -> LmdbEnvironment {
        LmdbEnvironment::new_null_with_data(self.databases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PutEvent, Transaction};
    use lmdb::WriteFlags;

    #[test]
    fn can_track_env_creations() {
        let env_factory = LmdbEnvironmentFactory::new_null();
        let tracker = env_factory.track();

        let options = EnvironmentOptions {
            max_dbs: 42,
            map_size: 1024 * 1024,
            flags: EnvironmentFlags::NO_SUB_DIR,
            path: "test-lmdb-file.ldb".into(),
        };

        let _ = env_factory.create(options.clone());

        let output = tracker.output();
        assert_eq!(output, vec![options]);
    }

    #[test]
    fn can_track_puts() {
        let env = LmdbEnvironment::new_null();

        let database = env
            .create_db(Some("testdb"), DatabaseFlags::empty())
            .unwrap();

        let mut txn = env.begin_write();
        let tracker = txn.track_puts();
        let key = &[1, 2, 3];
        let value = &[4, 5, 6];
        let flags = WriteFlags::APPEND;
        txn.put(database, key, value, flags).unwrap();

        let puts = tracker.output();
        assert_eq!(
            puts,
            vec![PutEvent {
                database,
                key: key.to_vec(),
                value: value.to_vec(),
                flags
            }]
        )
    }

    mod nullability {
        use super::*;

        #[test]
        fn read_database() {
            let database = LmdbDatabase::new_null(1);
            let env = LmdbEnvironment::null_builder()
                .database("foo", database)
                .entry(&[1, 2], &[3, 4])
                .build()
                .build();

            let txn = env.begin_read();
            let result = txn.get(database, &[1, 2]).unwrap();
            assert_eq!(result, [3, 4]);
        }

        #[test]
        fn open_unknown_database_fails() {
            let env = LmdbEnvironment::new_null();
            let result = env.open_db(Some("UNKNOWN"));
            assert_eq!(result, Err(lmdb::Error::NotFound));
        }

        #[test]
        fn create_db() {
            let env = LmdbEnvironment::new_null();
            env.create_db(Some("mydb"), DatabaseFlags::empty()).unwrap();
            let result = env.open_db(Some("mydb"));
            assert!(result.is_ok());
        }

        #[test]
        fn create_unnamed_db() {
            let env = LmdbEnvironment::new_null();
            env.create_db(None, DatabaseFlags::empty()).unwrap();
            let result = env.open_db(None);
            assert!(result.is_ok());
        }

        #[test]
        fn write_key_value() {
            let env = LmdbEnvironment::new_null();
            let dbi = env.create_db(Some("mydb"), DatabaseFlags::empty()).unwrap();
            {
                let mut txn = env.begin_write();
                txn.put(dbi, &[1, 2], &[3, 4], WriteFlags::empty()).unwrap();
                txn.commit();
            }
            let txn = env.begin_read();
            let result = txn.get(dbi, &[1, 2]).unwrap();
            assert_eq!(result, [3, 4]);
        }
    }
}
