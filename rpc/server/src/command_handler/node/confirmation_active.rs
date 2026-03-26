use crate::command_handler::RpcCommandHandler;
use rsnano_rpc_messages::{ConfirmationActiveArgs, ConfirmationActiveResponse, unwrap_u64_or_zero};

impl RpcCommandHandler {
    pub(crate) fn confirmation_active(
        &self,
        args: ConfirmationActiveArgs,
    ) -> ConfirmationActiveResponse {
        let announcements = unwrap_u64_or_zero(args.announcements);
        let mut confirmed = 0;
        let mut elections = Vec::new();

        for election in self.node.active.elections_round_robin() {
            let req_count = 0; // not supported in RsNano
            if req_count as u64 >= announcements {
                if !election.is_confirmed() {
                    elections.push(election.qualified_root().clone());
                } else {
                    confirmed += 1;
                }
            }
        }

        let unconfirmed = elections.len() as u64;
        ConfirmationActiveResponse {
            confirmations: elections,
            unconfirmed: unconfirmed.into(),
            confirmed: confirmed.into(),
        }
    }
}
