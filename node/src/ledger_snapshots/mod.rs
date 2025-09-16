mod consensus;
mod preconsensus;
mod tally;

use std::sync::{Arc, Mutex};

use rsnano_ledger::Ledger;
use rsnano_types::PrivateKey;

use crate::ledger_snapshots::consensus::Consensus;
use crate::ledger_snapshots::preconsensus::Preconsensus;
use crate::ledger_snapshots::tally::Aggregator;
use crate::representatives::OnlineReps;
use crate::transport::MessageFlooder;

pub struct LedgerSnapshots {
    pub preconsensus: Arc<Preconsensus>,
    pub consensus: Arc<Consensus>,
}

impl LedgerSnapshots {
    pub fn new(
        ledger: Arc<Ledger>,
        get_private_key: Arc<dyn Fn() -> Option<PrivateKey> + Send + Sync>,
        flooder: Arc<Mutex<MessageFlooder>>,
        online_reps: Arc<Mutex<OnlineReps>>,
    ) -> Self {
        Self {
            preconsensus: Preconsensus::new(
                ledger,
                get_private_key.clone(),
                flooder.clone(),
                online_reps.clone(),
            )
            .into(),
            consensus: Consensus::new(get_private_key, flooder.clone(), online_reps.clone()).into(),
        }
    }

    pub fn new_null() -> Self {
        Self {
            preconsensus: Preconsensus::new_null().into(),
            consensus: Consensus::new_null().into(),
        }
    }
}
