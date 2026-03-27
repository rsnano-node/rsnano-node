use std::{
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

use num_format::{Locale, ToFormattedString};
use tracing::info;

use rsnano_ledger::Ledger;
use rsnano_network::Network;
use rsnano_utils::{CancellationToken, ticker::Tickable};

use crate::{
    block_rate_calculator::CurrentBlockRates, consensus::AecService, representatives::OnlineReps,
};

/// Periodically prints info about BPS, CPS, elections, peers,...
pub struct NodeMonitor {
    ledger: Arc<Ledger>,
    network: Arc<RwLock<Network>>,
    online_reps: Arc<Mutex<OnlineReps>>,
    aec_service: Arc<AecService>,
    block_rates: Arc<CurrentBlockRates>,
    last_time: Option<Instant>,
}

impl NodeMonitor {
    pub fn new(
        ledger: Arc<Ledger>,
        network: Arc<RwLock<Network>>,
        online_reps: Arc<Mutex<OnlineReps>>,
        aec_service: Arc<AecService>,
        block_rates: Arc<CurrentBlockRates>,
    ) -> Self {
        Self {
            ledger,
            network,
            online_reps,
            aec_service,
            block_rates,
            last_time: None,
        }
    }

    fn log(&self) {
        let blocks_confirmed = self.ledger.confirmed_count();
        let blocks_total = self.ledger.block_count();

        let channels = self.network.read().unwrap().channels_info();
        info!(
            "Peers: {} (established: {} | inbound: {} | outbound: {})",
            channels.total, channels.established, channels.inbound, channels.outbound
        );

        {
            let (delta, online, peered) = {
                let online_reps = self.online_reps.lock().unwrap();
                (
                    online_reps.quorum_delta(),
                    online_reps.online_weight(),
                    online_reps.peered_weight(),
                )
            };
            info!(
                "Quorum: {} (stake peered: {} | online stake: {})",
                delta.format_balance(0),
                peered.format_balance(0),
                online.format_balance(0)
            );
        }

        let elections = self.aec_service.info();
        info!(
            "Elections active: {} (priority: {} | hinted: {} | optimistic: {})",
            elections.total, elections.priority, elections.hinted, elections.optimistic
        );

        // TODO: Maybe emphasize somehow that confirmed doesn't need to be equal to total; backlog is OK
        info!(
            "Blocks confirmed: {} | total: {} (backlog: {})",
            blocks_confirmed.to_formatted_string(&Locale::en),
            blocks_total.to_formatted_string(&Locale::en),
            (blocks_total - blocks_confirmed).to_formatted_string(&Locale::en)
        );

        let blocks_checked_rate = self.block_rates.bps();
        let blocks_confirmed_rate = self.block_rates.cps();

        info!(
            "Blocks rate: {} bps | {} cps)",
            blocks_checked_rate, blocks_confirmed_rate,
        );
    }
}

impl Tickable for NodeMonitor {
    fn tick(&mut self, _cancel_token: &CancellationToken) {
        if self.last_time.is_some() {
            self.log();
        } else {
            // Wait for node to warm up before logging
        }
        self.last_time = Some(Instant::now());
    }
}
