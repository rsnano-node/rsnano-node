use crate::AccountBalanceDto;
use anyhow::{bail, Result};
use reqwest::Client;
pub use reqwest::Url;
use rsnano_core::{Account, Amount, JsonBlock, RawKey, WalletId};
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

    pub async fn account_info(&self, account: Account) -> Result<AccountInfoDto> {
        let cmd = RpcCommand::account_info(account);
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

    pub async fn send_block(
        &self,
        wallet: WalletId,
        source: Account,
        destination: Account,
    ) -> Result<JsonBlock> {
        let request = RpcCommand::Send(SendArgs {
            wallet,
            source,
            destination,
            amount: Amount::raw(1),
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

    pub async fn keepalive(&self, port: u16) -> Result<()> {
        let request = RpcCommand::keepalive(Ipv6Addr::LOCALHOST, port);
        self.rpc_request(&request).await?;
        Ok(())
    }

    pub async fn key_create_rpc(&self) -> Result<KeyPairDto> {
        let cmd = RpcCommand::KeyCreate;
        let json = self.rpc_request(&cmd).await?;
        Ok(from_value(json)?)
    }

    pub async fn wallet_create_rpc(&self) -> Result<WalletId> {
        let cmd = RpcCommand::WalletCreate;
        let json = self.rpc_request(&cmd).await?;
        WalletId::decode_hex(json["wallet"].as_str().unwrap())
    }

    pub async fn wallet_add(&self, wallet: WalletId, prv_key: RawKey) -> Result<()> {
        let cmd = RpcCommand::wallet_add(wallet, prv_key);
        self.rpc_request(&cmd).await?;
        Ok(())
    }

    pub async fn stop_rpc(&self) -> Result<()> {
        self.rpc_request(&RpcCommand::Stop).await?;
        Ok(())
    }

    async fn rpc_request<T>(&self, request: &T) -> Result<Value>
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
