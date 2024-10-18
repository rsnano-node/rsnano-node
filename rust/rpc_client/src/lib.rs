use crate::AccountBalanceDto;
use anyhow::{bail, Result};
use reqwest::Client;
pub use reqwest::Url;
use rsnano_core::{
    Account, Amount, BlockHash, HashOrAccount, JsonBlock, PublicKey, RawKey, WalletId, WorkNonce,
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

    pub async fn republish(&self, args: impl Into<RepublishArgs>) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::republish(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_generate(
        &self,
        args: impl Into<WorkGenerateArgs>,
    ) -> Result<WorkGenerateDto> {
        let cmd = RpcCommand::work_generate(args.into());
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
        args: impl Into<ConfirmationInfoArgs>,
    ) -> Result<ConfirmationInfoDto> {
        let cmd = RpcCommand::confirmation_info(args.into());
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
        args: RepresentativesOnlineArgs,
    ) -> Result<RepresentativesOnlineDto> {
        let cmd = RpcCommand::representatives_online(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn receivable_exists(
        &self,
        args: impl Into<ReceivableExistsArgs>,
    ) -> Result<ExistsDto> {
        let cmd = RpcCommand::receivable_exists(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn receivable(&self, args: ReceivableArgs) -> Result<ReceivableDto> {
        let cmd = RpcCommand::receivable(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_receivable(&self, args: AccountsReceivableArgs) -> Result<ReceivableDto> {
        let cmd = RpcCommand::accounts_receivable(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_ledger(
        &self,
        args: impl Into<WalletLedgerArgs>,
    ) -> Result<WalletLedgerDto> {
        let cmd = RpcCommand::wallet_ledger(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_history(
        &self,
        args: impl Into<WalletHistoryArgs>,
    ) -> Result<WalletHistoryDto> {
        let cmd = RpcCommand::wallet_history(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_republish(&self, wallet: WalletId, count: u64) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::wallet_republish(wallet, count);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn search_receivable(&self, wallet: WalletId) -> Result<ExistsDto> {
        let cmd = RpcCommand::search_receivable(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_representative_set(
        &self,
        args: WalletRepresentativeSetArgs,
    ) -> Result<SetDto> {
        let cmd = RpcCommand::wallet_representative_set(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_receivable(&self, args: WalletReceivableArgs) -> Result<ReceivableDto> {
        let cmd = RpcCommand::wallet_receivable(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap_lazy(
        &self,
        args: impl Into<BootstrapLazyArgs>,
    ) -> Result<BootstrapLazyDto> {
        let cmd = RpcCommand::bootstrap_lazy(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap_any(&self, args: BootstrapAnyArgs) -> Result<SuccessDto> {
        let cmd = RpcCommand::bootstrap_any(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn bootstrap(&self, args: BootstrapArgs) -> Result<SuccessDto> {
        let cmd = RpcCommand::bootstrap(args);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn work_cancel(&self, hash: BlockHash) -> Result<SuccessDto> {
        let cmd = RpcCommand::work_cancel(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn process(&self, process_args: impl Into<ProcessArgs>) -> Result<HashRpcMessage> {
        let cmd = RpcCommand::process(process_args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn sign(&self, args: impl Into<SignArgs>) -> Result<SignDto> {
        let cmd = RpcCommand::sign(args.into());
        let json = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(json)?)
    }

    pub async fn account_history(
        &self,
        args: impl Into<AccountHistoryArgs>,
    ) -> Result<AccountHistoryDto> {
        let cmd = RpcCommand::account_history(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_balance(
        &self,
        args: impl Into<AccountBalanceArgs>,
    ) -> Result<AccountBalanceDto> {
        let cmd = RpcCommand::account_balance(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_create(
        &self,
        args: impl Into<AccountCreateArgs>,
    ) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::account_create(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(from_value(result)?)
    }

    pub async fn accounts_create(
        &self,
        args: impl Into<AccountsCreateArgs>,
    ) -> Result<AccountsRpcMessage> {
        let cmd = RpcCommand::accounts_create(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_remove(&self, wallet: WalletId, account: Account) -> Result<RemovedDto> {
        let cmd = RpcCommand::account_remove(wallet, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_move(
        &self,
        wallet: WalletId,
        source: WalletId,
        account: Vec<Account>,
    ) -> Result<MovedDto> {
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

    pub async fn wallet_contains(&self, wallet: WalletId, account: Account) -> Result<ExistsDto> {
        let cmd = RpcCommand::wallet_contains(wallet, account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_destroy(&self, wallet: WalletId) -> Result<DestroyedDto> {
        let cmd = RpcCommand::wallet_destroy(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_lock(&self, wallet: WalletId) -> Result<LockedDto> {
        let cmd = RpcCommand::wallet_lock(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_locked(&self, wallet: WalletId) -> Result<LockedDto> {
        let cmd = RpcCommand::wallet_locked(wallet);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn stop(&self) -> Result<SuccessDto> {
        let cmd = RpcCommand::stop();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_block_count(&self, account: Account) -> Result<AccountBlockCountDto> {
        let cmd = RpcCommand::account_block_count(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_key(&self, account: Account) -> Result<KeyRpcMessage> {
        let cmd = RpcCommand::account_key(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_representative(
        &self,
        account: Account,
    ) -> Result<AccountRepresentativeDto> {
        let cmd = RpcCommand::account_representative(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn account_weight(&self, account: Account) -> Result<WeightDto> {
        let cmd = RpcCommand::account_weight(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn available_supply(&self) -> Result<AvailableSupplyDto> {
        let cmd = RpcCommand::AvailableSupply;
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_account(&self, hash: BlockHash) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::block_account(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_confirm(&self, hash: BlockHash) -> Result<StartedDto> {
        let cmd = RpcCommand::block_confirm(hash);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_count(&self) -> Result<BlockCountDto> {
        let cmd = RpcCommand::BlockCount;
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn uptime(&self) -> Result<UptimeDto> {
        let cmd = RpcCommand::uptime();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn frontier_count(&self) -> Result<CountRpcMessage> {
        let cmd = RpcCommand::frontier_count();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn validate_account_number(&self, account: Account) -> Result<SuccessDto> {
        let cmd = RpcCommand::validate_account_number(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn nano_to_raw(&self, amount: Amount) -> Result<AmountRpcMessage> {
        let cmd = RpcCommand::nano_to_raw(amount);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn raw_to_nano(&self, amount: Amount) -> Result<AmountRpcMessage> {
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

    pub async fn wallet_representative(&self, wallet: WalletId) -> Result<WalletRepresentativeDto> {
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

    pub async fn password_enter(&self, wallet: WalletId, password: String) -> Result<ValidDto> {
        let cmd = RpcCommand::password_enter(wallet, password);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn password_valid(&self, wallet: WalletId) -> Result<ValidDto> {
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
    ) -> Result<RepresentativesDto> {
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

    pub async fn unopened(&self, args: impl Into<UnopenedArgs>) -> Result<AccountsWithAmountsDto> {
        let cmd = RpcCommand::unopened(args.into());
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

    pub async fn receive_minimum(&self) -> Result<AmountRpcMessage> {
        let cmd = RpcCommand::receive_minimum();
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_change_seed(
        &self,
        args: impl Into<WalletChangeSeedArgs>,
    ) -> Result<WalletChangeSeedDto> {
        let cmd = RpcCommand::wallet_change_seed(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn delegators(&self, args: impl Into<DelegatorsArgs>) -> Result<DelegatorsDto> {
        let cmd = RpcCommand::delegators(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn delegators_count(&self, account: Account) -> Result<CountRpcMessage> {
        let cmd = RpcCommand::delegators_count(account);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn block_hash(&self, block: JsonBlock) -> Result<HashRpcMessage> {
        let cmd = RpcCommand::block_hash(block);
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn accounts_balances(
        &self,
        args: impl Into<AccountsBalancesArgs>,
    ) -> Result<AccountsBalancesDto> {
        let cmd = RpcCommand::accounts_balances(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn wallet_balances(
        &self,
        args: impl Into<WalletBalancesArgs>,
    ) -> Result<AccountsBalancesDto> {
        let cmd = RpcCommand::wallet_balances(args.into());
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

    pub async fn successors(&self, args: impl Into<ChainArgs>) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::successors(args.into());
        let result = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn chain(&self, args: ChainArgs) -> Result<BlockHashesDto> {
        let cmd = RpcCommand::chain(args);
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

    pub async fn account_info(&self, args: impl Into<AccountInfoArgs>) -> Result<AccountInfoDto> {
        let account_info_args = args.into();
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

    pub async fn send(&self, args: SendArgs) -> Result<BlockDto> {
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

    pub async fn keepalive(&self, address: Ipv6Addr, port: u16) -> Result<StartedDto> {
        let cmd = RpcCommand::keepalive(address, port);
        let json = self.rpc_request(&cmd).await?;
        Ok(serde_json::from_value(json)?)
    }

    pub async fn key_create(&self) -> Result<KeyPairDto> {
        let cmd = RpcCommand::KeyCreate;
        let json = self.rpc_request(&cmd).await?;
        Ok(from_value(json)?)
    }

    pub async fn wallet_add(&self, args: WalletAddArgs) -> Result<AccountRpcMessage> {
        let cmd = RpcCommand::wallet_add(args);
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
