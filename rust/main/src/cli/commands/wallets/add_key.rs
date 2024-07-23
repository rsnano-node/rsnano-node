use crate::cli::get_path;
use anyhow::{anyhow, Result};
use clap::{ArgGroup, Parser};
use rsnano_core::{RawKey, WalletId};
use rsnano_node::wallets::{Wallets, WalletsExt};
use rsnano_store_lmdb::LmdbEnv;
use std::sync::Arc;

#[derive(Parser)]
#[command(group = ArgGroup::new("input")
    .args(&["data_path", "network"]))]
pub(crate) struct AddKeyArgs {
    /// Adds the key to the supplied <wallet>
    #[arg(long)]
    wallet: String,
    /// Adds the supplied <key> to the wallet
    #[arg(long)]
    key: String,
    /// Optional <password> to unlock the wallet
    #[arg(long)]
    password: Option<String>,
    /// Uses the supplied path as the data directory
    #[arg(long, group = "input")]
    data_path: Option<String>,
    /// Uses the supplied network (live, test, beta or dev)
    #[arg(long, group = "input")]
    network: Option<String>,
}

impl AddKeyArgs {
    pub(crate) fn add_key(&self) -> Result<()> {
        let wallet_id = WalletId::decode_hex(&self.wallet)?;

        let key = RawKey::decode_hex(&self.key)?;

        let path = get_path(&self.data_path, &self.network).join("wallets.ldb");

        let env = Arc::new(LmdbEnv::new(&path)?);

        let wallets = Arc::new(Wallets::new_with_env(env)?);

        let password = self.password.clone().unwrap_or_default();

        wallets.ensure_wallet_is_unlocked(wallet_id, &password);

        wallets
            .insert_adhoc2(&wallet_id, &key, false)
            .map_err(|e| anyhow!("Failed to insert key: {:?}", e))?;

        Ok(())
    }
}