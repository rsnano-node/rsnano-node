use rsnano_core::Amount;
use rsnano_node::Node;
use rsnano_rpc_messages::{AccountBalanceDto, AccountsBalancesArgs, AccountsBalancesDto, RpcDto};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn accounts_balances(node: Arc<Node>, args: AccountsBalancesArgs) -> RpcDto {
    let tx = node.ledger.read_txn();
    let mut balances = HashMap::new();
    let only_confirmed = args.include_only_confirmed.unwrap_or(true);

    for account in args.accounts {
        let balance = if only_confirmed {
            node.ledger
                .confirmed()
                .account_balance(&tx, &account)
                .unwrap_or(Amount::zero())
        } else {
            node.ledger
                .any()
                .account_balance(&tx, &account)
                .unwrap_or(Amount::zero())
        };

        let pending = node
            .ledger
            .account_receivable(&tx, &account, only_confirmed);

        balances.insert(account, AccountBalanceDto::new(balance, pending, pending));
    }

    let accounts_balances = AccountsBalancesDto { balances };
    RpcDto::AccountsBalances(accounts_balances)
}
