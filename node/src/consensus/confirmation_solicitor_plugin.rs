use std::{
    any::Any,
    sync::{Arc, Mutex},
};

use super::{
    AecTickerPlugin, AecTickerRead, ConfirmationSolicitor, confirm_req_sender::ConfirmReqSender,
    election::ElectionState, winner_block_broadcaster::WinnerBlockBroadcaster,
};
use crate::{representatives::OnlineReps, transport::MessageFlooder};

pub(crate) struct ConfirmationSolicitorPlugin {
    pub(crate) message_flooder: MessageFlooder,
    pub(crate) online_reps: Arc<Mutex<OnlineReps>>,
    pub(crate) winner_block_broadcaster: Arc<Mutex<WinnerBlockBroadcaster>>,
    pub(crate) confirm_req_sender: ConfirmReqSender,
}

impl ConfirmationSolicitorPlugin {
    #[allow(dead_code)]
    pub fn new_null() -> Self {
        Self {
            message_flooder: MessageFlooder::new_null(),
            online_reps: Arc::new(Mutex::new(OnlineReps::new_test_instance())),
            winner_block_broadcaster: Arc::new(Mutex::new(WinnerBlockBroadcaster::new_null())),
            confirm_req_sender: ConfirmReqSender::new_null(),
        }
    }
}

impl AecTickerPlugin for ConfirmationSolicitorPlugin {
    fn run(&mut self, aec: &dyn AecTickerRead) {
        let peered_prs = self.online_reps.lock().unwrap().peered_principal_reps();

        // TODO don't clone flooder!'
        let flooder = self.message_flooder.clone();
        let mut solicitor = ConfirmationSolicitor::new(flooder);
        solicitor.prepare(&peered_prs);

        /*
         * Loop through active elections requesting confirmation
         *
         * Only up to a certain amount of elections are queued for confirmation request and block rebroadcasting.
         * The remaining elections can still be confirmed if votes arrive
         * Elections extending the soft config.size limit are flushed after a certain time-to-live cutoff
         * Flushed elections are later re-activated via frontier confirmation
         */
        aec.for_each_confirmation_solicitation_election(&mut |election| {
            debug_assert_eq!(election.state(), ElectionState::Active);
            self.winner_block_broadcaster
                .lock()
                .unwrap()
                .try_broadcast_winner(&election.winner().clone(), election.votes());
            self.confirm_req_sender
                .send_confirm_req(&mut solicitor, election);
        });

        solicitor.flush();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
