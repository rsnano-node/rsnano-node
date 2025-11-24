use std::{fs::File, io::Read, path::PathBuf};

use anyhow::{Context, anyhow};
use clap::Parser;

use rsnano_types::WalletId;

use crate::cli::commands::wallets::WalletContext;
use crate::cli::{GlobalArgs, build_node};

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct ImportKeysArgs {
    /// The path of the file that contains the keys
    #[arg(long)]
    file: String,
    #[arg(long)]
    /// Optional password to unlock the wallet
    password: Option<String>,
    #[arg(long)]
    /// Forces the command if the wallet is locked
    force: bool,
    /// The wallet importing the keys
    #[arg(long)]
    wallet: String,
}

impl ImportKeysArgs {
    pub(crate) fn import_keys(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let mut file = File::open(PathBuf::from(&self.file))?;
        let mut contents = String::new();

        file.read_to_string(&mut contents)
            .context("Unable to read <file> contents")?;

        let node = build_node(&global_args)?;
        let wallet_id =
            WalletId::decode_hex(&self.wallet).ok_or_else(|| anyhow!("Invalid wallet id"))?;
        let password = self.password.clone().unwrap_or_default();

        if node.wallets.wallet_exists(&wallet_id) {
            let context = match WalletContext::from_existing(node, wallet_id, password.clone()) {
                Ok(ctx) => ctx,
                Err(_) => {
                    eprintln!(
                        "Invalid password for wallet {}. New wallet should have empty (default) password or passwords for new wallet & json file should match",
                        wallet_id
                    );
                    return Err(anyhow!("Invalid arguments"));
                }
            };
            let (node, wallet_id) = context.into_parts();
            node.wallets
                .import_replace(wallet_id, &contents, &password)?
        } else if !self.force {
            eprintln!("Wallet doesn't exist");
            return Err(anyhow!("Invalid arguments"));
        } else {
            node.wallets.import(wallet_id, &contents)?
        }

        Ok(())
    }
}
