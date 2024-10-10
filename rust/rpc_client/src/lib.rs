use crate::AccountBalanceDto;
use anyhow::{bail, Result};
use reqwest::Client;
pub use reqwest::Url;
use rsnano_core::{
    Account, Amount, BlockHash, HashOrAccount, JsonBlock, PublicKey, QualifiedRoot, RawKey,
    WalletId, WorkNonce,
};
use rsnano_rpc_messages::*;
use serde::Serialize;
use serde_json::{from_str, from_value, Value};
use std::{net::Ipv6Addr, time::Duration};

pub struct NanoRpcClient {
    url: Url,
    client: Client,
}

impl NanoRpcClient {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            client: reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }

    pub async fn telemetry(
        &self,
        address: Option<Ipv6Addr>,
        port: Option<u16>,
        raw: Option<bool>,
    ) -> Result<TelemetryDtos> {
        let cmd = RpcCommand::telemetry(raw, address, port);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_get(&self, key: PublicKey) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::account_get(key);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_create(&self, block_create_args: BlockCreateArgs) -> Result<BlockCreateDto> {
        let cmd = RpcCommand::block_create(block_create_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn republish(
        &self,
        hash: BlockHash,
        sources: Option<u64>,
        destinations: Option<u64>,
        count: Option<u64>,
    ) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::republish(hash, sources, destinations, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_generate(
        &self,
        work_generate_args: WorkGenerateArgs,
    ) -> Result<WorkGenerateDto> {
        let cmd = RpcCommand::work_generate(work_generate_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn ledger(&self, ledger_args: LedgerArgs) -> Result<LedgerDto> {
        let cmd = RpcCommand::ledger(ledger_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn confirmation_info(
        &self,
        root: QualifiedRoot,
        contents: Option<bool>,
        representatives: Option<bool>,
    ) -> Result<ConfirmationInfoDto> {
        let cmd = RpcCommand::confirmation_info(root, contents, representatives);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn unchecked_keys(&self, key: HashOrAccount, count: u64) -> Result<UncheckedKeysDto> {
        let cmd = RpcCommand::unchecked_keys(key, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn unchecked_get(&self, hash: BlockHash) -> Result<UncheckedGetDto> {
        let cmd = RpcCommand::unchecked_get(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn unchecked(&self, count: u64) -> Result<UncheckedDto> {
        let cmd = RpcCommand::unchecked(count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn representatives_online(
        &self,
        weight: Option<bool>,
        accounts: Option<Vec<Account>>,
    ) -> Result<AccountsWithAmountsDto> {
        let cmd = RpcCommand::representatives_online(weight, accounts);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn receivable_exists(
        &self,
        hash: BlockHash,
        include_active: Option<bool>,
        include_only_confirmed: Option<bool>,
    ) -> Result<BoolDto> {
        let cmd = RpcCommand::receivable_exists(hash, include_active, include_only_confirmed);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn receivable(
        &self,
        account: Account,
        count: u64,
        threshold: Option<Amount>,
        source: Option<bool>,
        min_version: Option<bool>,
        sorting: Option<bool>,
        include_only_confirmed: Option<bool>,
    ) -> Result<ReceivableDto> {
        let cmd = RpcCommand::receivable(
            account,
            count,
            threshold,
            source,
            min_version,
            sorting,
            include_only_confirmed,
        );
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_receivable(
        &self,
        accounts: Vec<Account>,
        count: u64,
        threshold: Option<Amount>,
        source: Option<bool>,
        sorting: Option<bool>,
        include_only_confirmed: Option<bool>,
    ) -> Result<ReceivableDto> {
        let cmd = RpcCommand::accounts_receivable(
            accounts,
            count,
            threshold,
            source,
            sorting,
            include_only_confirmed,
        );
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_ledger(
        &self,
        wallet: WalletId,
        representative: Option<bool>,
        weight: Option<bool>,
        receivable: Option<bool>,
        modified_since: Option<u64>,
    ) -> Result<WalletLedgerDto> {
        let cmd =
            RpcCommand::wallet_ledger(wallet, representative, weight, receivable, modified_since);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_history(
        &self,
        wallet: WalletId,
        modified_since: Option<u64>,
    ) -> Result<WalletHistoryDto> {
        let cmd = RpcCommand::wallet_history(wallet, modified_since);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_republish(&self, wallet: WalletId, count: u64) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::wallet_republish(wallet, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn search_receivable(&self, wallet: WalletId) -> Result<BoolDto> {
        let cmd = RpcCommand::search_receivable(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_representative_set(
        &self,
        wallet: WalletId,
        representative: Account,
        update_existing_accounts: Option<bool>,
    ) -> Result<BoolDto> {
        let cmd = RpcCommand::wallet_representative_set(
            WalletWithAccountArgs::new(wallet, representative),
            update_existing_accounts,
        );
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_receivable(
        &self,
        wallet: WalletId,
        count: u64,
        threshold: Option<Amount>,
        source: Option<bool>,
        min_version: Option<bool>,
        include_only_confirmed: Option<bool>,
    ) -> Result<ReceivableDto> {
        let cmd = RpcCommand::wallet_receivable(
            WalletWithCountArgs::new(wallet, count),
            threshold,
            source,
            min_version,
            include_only_confirmed,
        );
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap_lazy(
        &self,
        hash: BlockHash,
        force: Option<bool>,
        id: Option<String>,
    ) -> Result<BootstrapLazyDto> {
        let cmd = RpcCommand::bootstrap_lazy(hash, force, id);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap_any(
        &self,
        force: Option<bool>,
        id: Option<String>,
        account: Option<Account>,
    ) -> Result<SuccessDto> {
        let cmd = RpcCommand::bootstrap_any(force, id, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap(
        &self,
        address: Ipv6Addr,
        port: u16,
        id: Option<String>,
    ) -> Result<SuccessDto> {
        let cmd = RpcCommand::bootstrap(address, port, id);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_cancel(&self, hash: BlockHash) -> Result<SuccessDto> {
        let cmd = RpcCommand::work_cancel(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn process(&self, process_args: ProcessArgs) -> Result<BlockHashRpcMessage> {
        let cmd = RpcCommand::process(process_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn sign(
        &self,
        key: Option<RawKey>,
        wallet: Option<WalletId>,
        account: Option<Account>,
        block: JsonBlock,
    ) -> Result<SignDto> {
        let cmd = RpcCommand::sign(key, wallet, account, block);
        let json = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(json)?)
    }

    pub async fn account_history(
        &self,
        account_history_args: AccountHistoryArgs,
    ) -> Result<AccountHistoryDto> {
        let cmd = RpcCommand::account_history(account_history_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_balance(
        &self,
        account: Account,
        include_only_confirmed: Option<bool>,
    ) -> Result<AccountBalanceDto> {
        let cmd = RpcCommand::account_balance(account, include_only_confirmed);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_create(
        &self,
        wallet: WalletId,
        index: Option<u32>,
        work: Option<bool>,
    ) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::account_create(wallet, index, work);
        let result = self.rpc_request(&cmd).await?;
        Ok(from_value(result)?)
    }

    pub async fn accounts_create(
        &self,
        wallet: WalletId,
        count: u64,
        work: Option<bool>,
    ) -> Result<AccountsRpcMessage> {
        let cmd = RpcCommand::accounts_create(wallet, count, work);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_remove(&self, wallet: WalletId, account: Account) -> Result<BoolDto> {
        let cmd = RpcCommand::account_remove(wallet, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_move(
        &self,
        wallet: WalletId,
        source: WalletId,
        account: Vec<Account>,
    ) -> Result<BoolDto> {
        let cmd = RpcCommand::account_move(wallet, source, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_list(&self, wallet: WalletId) -> Result<AccountsRpcMessage> {
        let cmd = RpcCommand::account_list(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_create(&self, seed: Option<RawKey>) -> Result<WalletCreateDto> {
        let cmd = RpcCommand::wallet_create(seed);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_contains(&self, wallet: WalletId, account: Account) -> Result<BoolDto> {
        let cmd = RpcCommand::wallet_contains(wallet, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_destroy(&self, wallet: WalletId) -> Result<BoolDto> {
        let cmd = RpcCommand::wallet_destroy(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_lock(&self, wallet: WalletId) -> Result<BoolDto> {
        let cmd = RpcCommand::wallet_lock(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_locked(&self, wallet: WalletId) -> Result<BoolDto> {
        let cmd = RpcCommand::wallet_locked(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn stop(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::stop();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_block_count(&self, account: Account) -> Result<U64RpcMessage> {
        let cmd = RpcCommand::account_block_count(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_key(&self, account: Account) -> Result<KeyRpcMessage> {
        let cmd = RpcCommand::account_key(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_representative(&self, account: Account) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::account_representative(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_weight(&self, account: Account) -> Result<AmountDto> {
        let cmd = RpcCommand::account_weight(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn available_supply(&self) -> Result<AmountDto> {
        let cmd = RpcCommand::AvailableSupply;
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_account(&self, hash: BlockHash) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::block_account(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_confirm(&self, hash: BlockHash) -> Result<BoolDto> {
        let cmd = RpcCommand::block_confirm(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_count(&self) -> Result<BlockCountDto> {
        let cmd = RpcCommand::BlockCount;
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn uptime(&self) -> Result<U64RpcMessage> {
        let cmd = RpcCommand::uptime();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn frontier_count(&self) -> Result<U64RpcMessage> {
        let cmd = RpcCommand::frontier_count();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn validate_account_number(&self, account: Account) -> Result<SuccessDto> {
        let cmd = RpcCommand::validate_account_number(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn nano_to_raw(&self, amount: Amount) -> Result<AmountDto> {
        let cmd = RpcCommand::nano_to_raw(amount);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn raw_to_nano(&self, amount: Amount) -> Result<AmountDto> {
        let cmd = RpcCommand::raw_to_nano(amount);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_add_watch(
        &self,
        wallet: WalletId,
        accounts: Vec<Account>,
    ) -> Result<SuccessDto> {
        let cmd = RpcCommand::wallet_add_watch(wallet, accounts);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_representative(&self, wallet: WalletId) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::wallet_representative(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_set(
        &self,
        wallet: WalletId,
        account: Account,
        work: WorkNonce,
    ) -> Result<SuccessDto> {
        let cmd = RpcCommand::work_set(wallet, account, work);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_get(&self, wallet: WalletId, account: Account) -> Result<WorkDto> {
        let cmd = RpcCommand::work_get(wallet, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_work_get(&self, wallet: WalletId) -> Result<AccountsWithWorkDto> {
        let cmd = RpcCommand::wallet_work_get(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_frontiers(&self, accounts: Vec<Account>) -> Result<FrontiersDto> {
        let cmd = RpcCommand::accounts_frontiers(accounts);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_frontiers(&self, wallet: WalletId) -> Result<FrontiersDto> {
        let cmd = RpcCommand::wallet_frontiers(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn frontiers(&self, account: Account, count: u64) -> Result<FrontiersDto> {
        let cmd = RpcCommand::frontiers(account, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_info(&self, wallet: WalletId) -> Result<WalletInfoDto> {
        let cmd = RpcCommand::wallet_info(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_export(&self, wallet: WalletId) -> Result<JsonDto> {
        let cmd = RpcCommand::wallet_export(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn password_change(&self, wallet: WalletId, password: String) -> Result<SuccessDto> {
        let cmd = RpcCommand::password_change(wallet, password);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn password_enter(&self, wallet: WalletId, password: String) -> Result<BoolDto> {
        let cmd = RpcCommand::password_enter(wallet, password);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn password_valid(&self, wallet: WalletId) -> Result<BoolDto> {
        let cmd = RpcCommand::password_valid(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn deterministic_key(&self, seed: RawKey, index: u32) -> Result<KeyPairDto> {
        let cmd = RpcCommand::deterministic_key(seed, index);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn key_expand(&self, key: RawKey) -> Result<KeyPairDto> {
        let cmd = RpcCommand::key_expand(key);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn peers(&self, peer_details: Option<bool>) -> Result<PeersDto> {
        let cmd = RpcCommand::peers(peer_details);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn populate_backlog(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::populate_backlog();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn representatives(
        &self,
        count: Option<u64>,
        sorting: Option<bool>,
    ) -> Result<AccountsWithAmountsDto> {
        let cmd = RpcCommand::representatives(count, sorting);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_representatives(
        &self,
        accounts: Vec<Account>,
    ) -> Result<AccountsRepresentativesDto> {
        let cmd = RpcCommand::accounts_representatives(accounts);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn stats_clear(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::stats_clear();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn unchecked_clear(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::unchecked_clear();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn unopened(
        &self,
        account: Account,
        count: u64,
        threshold: Option<Amount>,
    ) -> Result<AccountsWithAmountsDto> {
        let cmd = RpcCommand::unopened(account, count, threshold);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn node_id(&self) -> Result<NodeIdDto> {
        let cmd = RpcCommand::node_id();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn search_receivable_all(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::search_receivable_all();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn receive_minimum(&self) -> Result<AmountDto> {
        let cmd = RpcCommand::receive_minimum();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_change_seed(
        &self,
        wallet: WalletId,
        seed: RawKey,
        count: Option<u32>,
    ) -> Result<WalletChangeSeedDto> {
        let cmd = RpcCommand::wallet_change_seed(wallet, seed, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn delegators(
        &self,
        account: Account,
        threshold: Option<Amount>,
        count: Option<u64>,
        start: Option<Account>,
    ) -> Result<AccountsWithAmountsDto> {
        let cmd = RpcCommand::delegators(account, threshold, count, start);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn delegators_count(&self, account: Account) -> Result<CountDto> {
        let cmd = RpcCommand::delegators_count(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_hash(&self, block: JsonBlock) -> Result<BlockHashRpcMessage> {
        let cmd = RpcCommand::block_hash(block);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_balances(
        &self,
        accounts: Vec<Account>,
        include_only_confirmed: Option<bool>,
    ) -> Result<AccountsBalancesDto> {
        let cmd = RpcCommand::accounts_balances(accounts, include_only_confirmed);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_balances(
        &self,
        wallet: WalletId,
        threshold: Option<Amount>,
    ) -> Result<AccountsBalancesDto> {
        let cmd = RpcCommand::wallet_balances(wallet, threshold);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_info(&self, hash: BlockHash) -> Result<BlockInfoDto> {
        let cmd = RpcCommand::block_info(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn blocks(&self, blocks: Vec<BlockHash>) -> Result<BlocksDto> {
        let cmd = RpcCommand::blocks(blocks);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn blocks_info(&self, blocks: Vec<BlockHash>) -> Result<BlocksInfoDto> {
        let cmd = RpcCommand::blocks_info(blocks);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn successors(
        &self,
        block: BlockHash,
        count: u64,
        offset: Option<u64>,
        reverse: Option<bool>,
    ) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::successors(block, count, offset, reverse);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn chain(
        &self,
        block: BlockHash,
        count: u64,
        offset: Option<u64>,
        reverse: Option<bool>,
    ) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::chain(block, count, offset, reverse);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn confirmation_active(
        &self,
        announcements: Option<u64>,
    ) -> Result<ConfirmationActiveDto> {
        let cmd = RpcCommand::confirmation_active(announcements);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn confirmation_quorum(
        &self,
        peer_details: Option<bool>,
    ) -> Result<ConfirmationQuorumDto> {
        let cmd = RpcCommand::confirmation_quorum(peer_details);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_validate(&self, work: WorkNonce, hash: BlockHash) -> Result<WorkValidateDto> {
        let cmd = RpcCommand::work_validate(work, hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_info(&self, account_info_args: AccountInfoArgs) -> Result<AccountInfoDto> {
        let cmd = RpcCommand::account_info(account_info_args);
        let result = self.rpc_request(&cmd).await?;
        Ok(from_value(result)?)
    }

    pub async fn receive_block(
        &self,
        wallet: WalletId,
        destination: Account,
        block: impl Into<JsonBlock>,
    ) -> Result<()> {
        let request = RpcCommand::Receive(ReceiveArgs {
            wallet,
            account: destination,
            block: block.into(),
        });
        self.rpc_request(&request).await?;
        Ok(())
    }

    pub async fn send(&self, args: SendArgs) -> Result<BlockHashRpcMessage> {
        let request = RpcCommand::send(args);
        let result = self.rpc_request(&request).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn send_block(
        &self,
        wallet: WalletId,
        source: Account,
        destination: Account,
    ) -> Result<JsonBlock> {
        let request = RpcCommand::send(SendArgs {
            wallet,
            source,
            destination,
            amount: Amount::raw(1),
            work: None,
            id: None,
        });
        let json = self.rpc_request(&request).await?;
        let block = json["block"].as_str().unwrap().to_owned();
        Ok(from_str(&block)?)
    }

    pub async fn send_receive(
        &self,
        wallet: WalletId,
        source: Account,
        destination: Account,
    ) -> Result<()> {
        let block = self.send_block(wallet, source, destination).await?;
        self.receive_block(wallet, destination, block).await
    }

    pub async fn keepalive(&self, address: Ipv6Addr, port: u16) -> Result<SuccessDto> {
        let cmd = RpcCommand::keepalive(address, port);
        let json = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(json)?)
    }

    pub async fn key_create(&self) -> Result<KeyPairDto> {
        let cmd = RpcCommand::KeyCreate;
        let json = self.rpc_request(&cmd).await?;
        Ok(from_value(json)?)
    }

    pub async fn wallet_add(
        &self,
        wallet: WalletId,
        prv_key: RawKey,
        work: Option<bool>,
    ) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::wallet_add(wallet, prv_key, work);
        let json = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(json)?)
    }

    pub async fn stop_rpc(&self) -> Result<()> {
        self.rpc_request(&RpcCommand::Stop).await?;
        Ok(())
    }

    async fn rpc_request<T>(&self, request: &T) -> Result<serde_json::Value>
    where
        T: Serialize,
    {
        let result = self
            .client
            .post(self.url.clone())
            .json(request)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        if let Some(error) = result.get("error") {
            bail!("node returned error: {}", error);
        }

        Ok(result)
    }
}
