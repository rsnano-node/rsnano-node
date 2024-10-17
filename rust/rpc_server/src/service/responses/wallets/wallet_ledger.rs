use rsnano_core::Account;
use rsnano_node::Node;
use rsnano_rpc_messages::{AccountInfo, ErrorDto, RpcDto, WalletLedgerArgs, WalletLedgerDto};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn wallet_ledger(
    node: Arc<Node>,
    enable_control: bool,
    args: WalletLedgerArgs,
) -> RpcDto {
    if !enable_control {
        return RpcDto::Error(ErrorDto::RPCControlDisabled);
    }

    let WalletLedgerArgs {
        wallet,
        representative,
        weight,
        receivable,
        modified_since,
    } = args;

    let representative = representative.unwrap_or(false);
    let weight = weight.unwrap_or(false);
    let receivable = receivable.unwrap_or(false);
    let modified_since = modified_since.unwrap_or(0);

    match node.wallets.get_accounts_of_wallet(&wallet) {
        Ok(accounts) => {
            let accounts_json = get_accounts_info(
                node,
                accounts,
                representative,
                weight,
                receivable,
                modified_since,
            );
            RpcDto::WalletLedger(WalletLedgerDto {
                accounts: accounts_json,
            })
        }
        Err(e) => RpcDto::Error(ErrorDto::WalletsError(e)),
    }
}

fn get_accounts_info(
    node: Arc<Node>,
    accounts: Vec<Account>,
    representative: bool,
    weight: bool,
    receivable: bool,
    modified_since: u64,
) -> HashMap<Account, AccountInfo> {
    let block_transaction = node.store.tx_begin_read();
    let mut accounts_json = HashMap::new();

    for account in accounts {
        if let Some(info) = node.ledger.any().get_account(&block_transaction, &account) {
            if info.modified >= modified_since {
                let entry = AccountInfo::new(
                    info.head,
                    info.open_block,
                    node.ledger
                        .representative_block_hash(&block_transaction, &info.head),
                    info.balance,
                    info.modified,
                    info.block_count,
                    representative.then(|| info.representative.as_account()),
                    weight.then(|| node.ledger.weight_exact(&block_transaction, account.into())),
                    receivable.then(|| {
                        node.ledger
                            .account_receivable(&block_transaction, &account, false)
                    }),
                    receivable.then(|| {
                        node.ledger
                            .account_receivable(&block_transaction, &account, false)
                    }),
                );

                accounts_json.insert(account, entry);
            }
        }
    }

    accounts_json
}
