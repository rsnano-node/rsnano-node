use rsnano_node::Node;
use rsnano_rpc_messages::{AccountsWithWorkDto, ErrorDto, RpcDto, WalletRpcMessage};
use std::{collections::HashMap, sync::Arc};

pub async fn wallet_work_get(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletRpcMessage,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let accounts = match node.wallets.get_accounts_of_wallet(&args.wallet) {
        Ok(accounts) => accounts,
        Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
    };

    let mut works = HashMap::new();

    for account in accounts {
        match node.wallets.work_get2(&args.wallet, &account.into()) {
            Ok(work) => {
                works.insert(account, work.into());
            }
            Err(e) => return RpcDto::Error(ErrorDto::WalletsError(e)),
        }
    }

    RpcDto::WalletWorkGet(AccountsWithWorkDto::new(works))
}
