use rsnano_node::{bootstrap::BootstrapInitiatorExt, Node};
use rsnano_rpc_messages::{BootstrapAnyArgs, ErrorDto, RpcDto, SuccessDto};
use std::sync::Arc;

pub async fn bootstrap_any(node: Arc<Node>, args: BootstrapAnyArgs) -> RpcDto {
    if node.flags.disable_legacy_bootstrap {
        return RpcDto::Error(ErrorDto::LegacyBootstrapDisabled);
    }

    let force = args.force.unwrap_or(false);
    let bootstrap_id = args.id.unwrap_or_default();
    let start_account = args.account.unwrap_or_default();

    node.bootstrap_initiator
        .bootstrap(force, bootstrap_id, u32::MAX, start_account);

    RpcDto::BootstrapAny(SuccessDto::new())
}
