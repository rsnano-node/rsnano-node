use crate::{
    consensus::ActiveElections,
    representatives::OnlineReps,
    transport::Network,
    utils::{CancellationToken, Runnable},
};
use rsnano_ledger::Ledger;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tracing::info;

pub struct Monitor {
    ledger: Arc<Ledger>,
    network: Arc<Network>,
    representative_register: Arc<Mutex<OnlineReps>>,
    active: Arc<ActiveElections>,
    last_time: Option<Instant>,
    last_blocks_cemented: u64,
    last_blocks_total: u64,
}

impl Monitor {
    pub fn new(
        ledger: Arc<Ledger>,
        network: Arc<Network>,
        representative_register: Arc<Mutex<OnlineReps>>,
        active: Arc<ActiveElections>,
    ) -> Self {
        Self {
            ledger,
            network,
            representative_register,
            active,
            last_time: None,
            last_blocks_total: 0,
            last_blocks_cemented: 0,
        }
    }

    fn log(&self, last: Instant, blocks_cemented: u64, blocks_total: u64) {
        // TODO: Maybe emphasize somehow that confirmed doesn't need to be equal to total; backlog is OK
        info!(
            "Blocks confirmed: {} | total: {}",
            blocks_cemented, blocks_total
        );

        // Calculate the rates
        let elapsed_secs = last.elapsed().as_secs() as f64;
        let blocks_confirmed_rate =
            (blocks_cemented - self.last_blocks_cemented) as f64 / elapsed_secs;
        let blocks_checked_rate = (blocks_total - self.last_blocks_total) as f64 / elapsed_secs;

        info!(
            "Blocks rate (average over last {}s: confirmed: {:.2}/s | total {:.2}/s)",
            elapsed_secs, blocks_confirmed_rate, blocks_checked_rate
        );

        let channels = self.network.channels_info();
        info!("Peers: {} (realtime: {} | bootstrap: {} | inbound connections: {} | outbound connections: {})",
            channels.total, channels.realtime, channels.bootstrap, channels.inbound, channels.outbound);

        let quorum_info = self.representative_register.lock().unwrap().quorum_info();

        info!(
            "Quorum: {} (stake peered: {} | online stake: {})",
            quorum_info.quorum_delta.format_balance(0),
            quorum_info.online_weight.format_balance(0),
            quorum_info.peers_weight.format_balance(0)
        );

        let elections = self.active.info();
        info!(
            "Elections active: {} (priority: {} | hinted: {} | optimistic: {})",
            elections.total, elections.priority, elections.hinted, elections.optimistic
        );
    }
}

impl Runnable for Monitor {
    fn run(&mut self, _cancel_token: &CancellationToken) {
        let blocks_cemented = self.ledger.cemented_count();
        let blocks_total = self.ledger.block_count();

        if let Some(last) = self.last_time {
            self.log(last, blocks_cemented, blocks_total);
        } else {
            // Wait for node to warm up before logging
        }
        self.last_time = Some(Instant::now());
        self.last_blocks_cemented = blocks_cemented;
        self.last_blocks_total = blocks_total;
    }
}