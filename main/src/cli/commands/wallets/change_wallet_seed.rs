use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;
use rsnano_types::RawKey;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct ChangeWalletSeedArgs {
    /// Changes the seed of the supplied wallet
    #[arg(long)]
    wallet: String,
    /// The new <seed> of the wallet
    #[arg(long)]
    seed: String,
    /// Optional <password> to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl ChangeWalletSeedArgs {
    pub(crate) fn change_wallet_seed(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();
        let seed = RawKey::decode_hex(&self.seed).ok_or_else(|| anyhow!("Invalid seed"))?;

        node.wallets
            .change_seed(wallet_id, &seed, 0)
            .map_err(|e| anyhow!("Failed to change wallet seed: {:?}", e))?;

        Ok(())
    }
}
