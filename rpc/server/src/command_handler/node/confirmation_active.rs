use crate::command_handler::RpcCommandHandler;
use rsnano_rpc_messages::{ConfirmationActiveArgs, ConfirmationActiveResponse, unwrap_u64_or_zero};

impl RpcCommandHandler {
    pub(crate) fn confirmation_active(
        &self,
        args: ConfirmationActiveArgs,
    ) -> ConfirmationActiveResponse {
        let announcements = unwrap_u64_or_zero(args.announcements);
        let result = self.node.active.confirmation_active(announcements);
        let unconfirmed = result.unconfirmed_roots.len() as u64;
        ConfirmationActiveResponse {
            confirmations: result.unconfirmed_roots,
            unconfirmed: unconfirmed.into(),
            confirmed: result.confirmed.into(),
        }
    }
}
