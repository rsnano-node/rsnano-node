use rsnano_core::Amount;
use rsnano_node::Node;
use rsnano_rpc_messages::{AccountBalanceArgs, AccountBalanceDto, RpcDto};
use std::sync::Arc;

pub async fn account_balance(node: Arc<Node>, args: AccountBalanceArgs) -> RpcDto {
    let tx = node.ledger.read_txn();
    let include_unconfirmed_blocks = args.include_only_confirmed.unwrap_or(false);

    let balance = if !include_unconfirmed_blocks {
        node.ledger
            .confirmed()
            .account_balance(&tx, &args.account)
            .unwrap_or(Amount::zero())
    } else {
        node.ledger
            .any()
            .account_balance(&tx, &args.account)
            .unwrap_or(Amount::zero())
    };

    let pending = node
        .ledger
        .account_receivable(&tx, &args.account, !include_unconfirmed_blocks);

    let account_balance_dto = AccountBalanceDto::new(balance, pending, pending);

    RpcDto::AccountBalance(account_balance_dto)
}
