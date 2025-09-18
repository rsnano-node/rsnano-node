use crate::command_handler::RpcCommandHandler;
use rsnano_rpc_messages::StartedResponse;

impl RpcCommandHandler {
    pub(crate) fn start_ledger_snapshot(&self) -> StartedResponse {
        self.node.ledger_snapshots.start_ledger_snapshot();
        StartedResponse::new(true)
    }
}
