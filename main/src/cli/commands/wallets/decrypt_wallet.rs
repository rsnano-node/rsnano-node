use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct DecryptWalletArgs {
    /// The wallet to be decrypted
    #[arg(long)]
    wallet: String,
    /// Optional password to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl DecryptWalletArgs {
    pub(crate) fn decrypt_wallet(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();

        let seed = node
            .wallets
            .get_seed(wallet_id)
            .map_err(|e| anyhow!("Failed to get wallet seed: {:?}", e))?;

        println!("Seed: {:?}", seed);
        Ok(())
    }
}
