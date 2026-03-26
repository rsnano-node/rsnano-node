use std::{
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use lmdb::DatabaseFlags;
use rsnano_output_tracker::{OutputListener, OutputTracker};

use super::{ConfiguredDatabase, LmdbDatabase, RoCursor};
use crate::{RwCursor, Transaction};

#[derive(Clone, Debug, PartialEq)]
pub struct PutEvent {
    pub database: LmdbDatabase,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub flags: lmdb::WriteFlags,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeleteEvent {
    pub database: LmdbDatabase,
    pub key: Vec<u8>,
}

pub struct WriteTransaction {
    txn: RwTransaction,
    put_listener: OutputListener<PutEvent>,
    delete_listener: OutputListener<DeleteEvent>,
    clear_listener: OutputListener<LmdbDatabase>,
    start: Instant,
}

impl WriteTransaction {
    pub fn new(txn: lmdb::RwTransaction<'static>) -> Self {
        Self::with_txn(RwTransaction::new(txn))
    }

    pub fn new_null(databases: Arc<Mutex<Vec<ConfiguredDatabase>>>) -> Self {
        Self::with_txn(RwTransaction::new_null(databases))
    }

    fn with_txn(txn: RwTransaction) -> Self {
        Self {
            txn,
            put_listener: OutputListener::new(),
            delete_listener: OutputListener::new(),
            clear_listener: OutputListener::new(),
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn commit(self) {
        self.txn.commit().unwrap();
    }

    pub fn track_puts(&self) -> Rc<OutputTracker<PutEvent>> {
        self.put_listener.track()
    }

    pub fn track_deletions(&self) -> Rc<OutputTracker<DeleteEvent>> {
        self.delete_listener.track()
    }

    pub fn track_clears(&self) -> Rc<OutputTracker<LmdbDatabase>> {
        self.clear_listener.track()
    }

    /// ## Safety
    ///
    /// This function **must not** be called from multiple concurrent transactions in the same environment.
    pub unsafe fn create_db(
        &mut self,
        name: Option<&str>,
        flags: lmdb::DatabaseFlags,
    ) -> lmdb::Result<LmdbDatabase> {
        unsafe { self.txn.create_db(name, flags) }
    }

    pub fn put(
        &mut self,
        database: LmdbDatabase,
        key: &[u8],
        value: &[u8],
        flags: lmdb::WriteFlags,
    ) -> lmdb::Result<()> {
        if self.put_listener.is_tracked() {
            self.put_listener.emit(PutEvent {
                database,
                key: key.to_vec(),
                value: value.to_vec(),
                flags,
            });
        }
        self.txn.put(database, key, value, flags)
    }

    pub fn delete(
        &mut self,
        database: LmdbDatabase,
        key: &[u8],
        flags: Option<&[u8]>,
    ) -> lmdb::Result<()> {
        if self.delete_listener.is_tracked() {
            self.delete_listener.emit(DeleteEvent {
                database,
                key: key.to_vec(),
            });
        }
        self.txn.del(database, key, flags)
    }

    pub fn clear_db(&mut self, database: LmdbDatabase) -> lmdb::Result<()> {
        self.clear_listener.emit(database);
        self.txn.clear_db(database)
    }

    pub fn open_rw_cursor(&mut self, database: LmdbDatabase) -> lmdb::Result<RwCursor<'_>> {
        self.txn.open_rw_cursor(database)
    }

    /// ## Safety
    ///
    /// This method is unsafe in the same ways as `Environment::close_db`, and
    /// should be used accordingly.
    pub unsafe fn drop_db(&mut self, database: LmdbDatabase) -> lmdb::Result<()> {
        unsafe { self.txn.drop_db(database) }
    }
}

impl Transaction for WriteTransaction {
    fn get(&self, database: LmdbDatabase, key: &[u8]) -> lmdb::Result<&[u8]> {
        self.txn.get(database, key)
    }

    fn open_ro_cursor(&self, database: LmdbDatabase) -> lmdb::Result<RoCursor<'_>> {
        self.txn.open_ro_cursor(database)
    }

    fn count(&self, database: LmdbDatabase) -> u64 {
        self.txn.count(database)
    }

    fn is_refresh_needed(&self) -> bool {
        self.is_refresh_needed_with(Duration::from_millis(500))
    }

    fn is_refresh_needed_with(&self, max_duration: Duration) -> bool {
        self.start.elapsed() > max_duration
    }
}

struct RwTransaction {
    strategy: RwTransactionStrategy,
}

impl RwTransaction {
    pub fn new(txn: lmdb::RwTransaction<'static>) -> Self {
        Self {
            strategy: RwTransactionStrategy::Real(RwTransactionWrapper(txn)),
        }
    }

    pub fn new_null(databases: Arc<Mutex<Vec<ConfiguredDatabase>>>) -> Self {
        let db_copies = databases.lock().unwrap().clone();
        Self {
            strategy: RwTransactionStrategy::Nulled(RwTransactionStub {
                db_copies,
                databases,
            }),
        }
    }

    pub fn get(&self, database: LmdbDatabase, key: &[u8]) -> lmdb::Result<&[u8]> {
        match &self.strategy {
            RwTransactionStrategy::Real(s) => s.get(database, key),
            RwTransactionStrategy::Nulled(s) => s.get(database, key),
        }
    }

    pub fn put(
        &mut self,
        database: LmdbDatabase,
        key: &[u8],
        data: &[u8],
        flags: lmdb::WriteFlags,
    ) -> lmdb::Result<()> {
        match &mut self.strategy {
            RwTransactionStrategy::Real(s) => {
                s.put(database.as_real(), key, data, flags)?;
            }
            RwTransactionStrategy::Nulled(s) => s.put(database, key, data)?,
        }
        Ok(())
    }

    pub fn del(
        &mut self,
        database: LmdbDatabase,
        key: &[u8],
        flags: Option<&[u8]>,
    ) -> lmdb::Result<()> {
        match &mut self.strategy {
            RwTransactionStrategy::Real(s) => s.del(database.as_real(), key, flags)?,
            RwTransactionStrategy::Nulled(s) => s.del(database, key)?,
        }
        Ok(())
    }

    /// ## Safety
    ///
    /// This function (as well as `Environment::open_db`,
    /// `Environment::create_db`, and `Database::open`) **must not** be called
    /// from multiple concurrent transactions in the same environment. A
    /// transaction which uses this function must finish (either commit or
    /// abort) before any other transaction may use this function.
    pub unsafe fn create_db(
        &self,
        name: Option<&str>,
        flags: DatabaseFlags,
    ) -> lmdb::Result<LmdbDatabase> {
        unsafe {
            match &self.strategy {
                RwTransactionStrategy::Real(s) => s.create_db(name, flags),
                RwTransactionStrategy::Nulled(s) => s.create_db(name, flags),
            }
        }
    }

    /// ## Safety
    ///
    /// This method is unsafe in the same ways as `Environment::close_db`, and
    /// should be used accordingly.
    pub unsafe fn drop_db(&mut self, database: LmdbDatabase) -> lmdb::Result<()> {
        unsafe {
            if let RwTransactionStrategy::Real(s) = &mut self.strategy {
                s.drop_db(database.as_real())?;
            }
            Ok(())
        }
    }

    pub fn clear_db(&mut self, database: LmdbDatabase) -> lmdb::Result<()> {
        if let RwTransactionStrategy::Real(s) = &mut self.strategy {
            s.clear_db(database.as_real())?;
        }
        Ok(())
    }

    pub fn open_ro_cursor(&self, database: LmdbDatabase) -> lmdb::Result<RoCursor<'_>> {
        match &self.strategy {
            RwTransactionStrategy::Real(s) => s.open_ro_cursor(database),
            RwTransactionStrategy::Nulled(s) => s.open_ro_cursor(database),
        }
    }

    pub fn open_rw_cursor(&mut self, database: LmdbDatabase) -> lmdb::Result<RwCursor<'_>> {
        match &mut self.strategy {
            RwTransactionStrategy::Real(s) => s.open_rw_cursor(database),
            RwTransactionStrategy::Nulled(_) => todo!(),
        }
    }

    pub fn count(&self, database: LmdbDatabase) -> u64 {
        match &self.strategy {
            RwTransactionStrategy::Real(s) => s.count(database.as_real()),
            RwTransactionStrategy::Nulled(_) => 0,
        }
    }

    pub fn commit(self) -> lmdb::Result<()> {
        match self.strategy {
            RwTransactionStrategy::Real(s) => s.commit()?,
            RwTransactionStrategy::Nulled(s) => s.commit(),
        }
        Ok(())
    }
}

enum RwTransactionStrategy {
    Real(RwTransactionWrapper),
    Nulled(RwTransactionStub),
}

pub struct RwTransactionWrapper(lmdb::RwTransaction<'static>);

impl RwTransactionWrapper {
    fn get(&self, database: LmdbDatabase, key: &[u8]) -> lmdb::Result<&[u8]> {
        lmdb::Transaction::get(&self.0, database.as_real(), &key)
    }

    fn put(
        &mut self,
        database: lmdb::Database,
        key: &[u8],
        data: &[u8],
        flags: lmdb::WriteFlags,
    ) -> lmdb::Result<()> {
        lmdb::RwTransaction::put(&mut self.0, database, &key, &data, flags)
    }

    fn del(
        &mut self,
        database: lmdb::Database,
        key: &[u8],
        flags: Option<&[u8]>,
    ) -> lmdb::Result<()> {
        lmdb::RwTransaction::del(&mut self.0, database, &key, flags)
    }

    fn clear_db(&mut self, database: lmdb::Database) -> lmdb::Result<()> {
        lmdb::RwTransaction::clear_db(&mut self.0, database)
    }

    fn commit(self) -> lmdb::Result<()> {
        lmdb::Transaction::commit(self.0)
    }

    fn open_ro_cursor(&self, database: LmdbDatabase) -> lmdb::Result<RoCursor<'_>> {
        let cursor = lmdb::Transaction::open_ro_cursor(&self.0, database.as_real());
        cursor.map(RoCursor::new)
    }

    fn open_rw_cursor(&mut self, database: LmdbDatabase) -> lmdb::Result<RwCursor<'_>> {
        let cursor = lmdb::RwTransaction::open_rw_cursor(&mut self.0, database.as_real());
        cursor.map(RwCursor::new)
    }

    fn count(&self, database: lmdb::Database) -> u64 {
        let stat = lmdb::Transaction::stat(&self.0, database);
        stat.unwrap().entries() as u64
    }

    /// ## Safety
    ///
    /// This method is unsafe in the same ways as `Environment::close_db`, and
    /// should be used accordingly.
    unsafe fn drop_db(&mut self, database: lmdb::Database) -> lmdb::Result<()> {
        unsafe { lmdb::RwTransaction::drop_db(&mut self.0, database) }
    }

    /// ## Safety
    ///
    /// This function (as well as `Environment::open_db`,
    /// `Environment::create_db`, and `Database::open`) **must not** be called
    /// from multiple concurrent transactions in the same environment. A
    /// transaction which uses this function must finish (either commit or
    /// abort) before any other transaction may use this function.
    unsafe fn create_db(
        &self,
        name: Option<&str>,
        flags: DatabaseFlags,
    ) -> lmdb::Result<LmdbDatabase> {
        unsafe { lmdb::RwTransaction::create_db(&self.0, name, flags).map(LmdbDatabase::new) }
    }
}

pub struct RwTransactionStub {
    db_copies: Vec<ConfiguredDatabase>,
    databases: Arc<Mutex<Vec<ConfiguredDatabase>>>,
}

impl RwTransactionStub {
    fn get(&self, database: LmdbDatabase, key: &[u8]) -> lmdb::Result<&[u8]> {
        let db = self.get_database(database)?;

        match db.entries.get(key) {
            Some(Ok(bytes)) => Ok(bytes.as_slice()),
            Some(Err(e)) => Err(*e),
            None => Err(lmdb::Error::NotFound),
        }
    }

    fn put(&mut self, database: LmdbDatabase, key: &[u8], data: &[u8]) -> lmdb::Result<()> {
        let db = self.get_database_mut(database)?;
        db.insert(key, data);
        Ok(())
    }

    fn open_ro_cursor(&self, database: LmdbDatabase) -> lmdb::Result<RoCursor<'_>> {
        Ok(RoCursor::new_null_with(
            self.db_copies.iter().find(|db| db.dbi == database).unwrap(),
        ))
    }

    fn create_db(&self, _name: Option<&str>, _flags: DatabaseFlags) -> lmdb::Result<LmdbDatabase> {
        Ok(LmdbDatabase::new_null(42))
    }

    fn get_database(&self, database: LmdbDatabase) -> lmdb::Result<&ConfiguredDatabase> {
        self.db_copies
            .iter()
            .find(|d| d.dbi == database)
            .ok_or(lmdb::Error::NotFound)
    }

    fn get_database_mut(
        &mut self,
        database: LmdbDatabase,
    ) -> lmdb::Result<&mut ConfiguredDatabase> {
        self.db_copies
            .iter_mut()
            .find(|d| d.dbi == database)
            .ok_or(lmdb::Error::NotFound)
    }

    fn commit(self) {
        *self.databases.lock().unwrap() = self.db_copies;
    }

    fn del(&mut self, database: LmdbDatabase, key: &[u8]) -> lmdb::Result<()> {
        self.get_database_mut(database)?.entries.remove(key);
        Ok(())
    }
}
