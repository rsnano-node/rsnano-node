use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use clap::Parser;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct DestroyWalletArgs {
    /// The wallet to be destroyed
    #[arg(long)]
    wallet: String,
    /// Optional password to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl DestroyWalletArgs {
    pub(crate) fn destroy_wallet(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();
        node.wallets.destroy(&wallet_id);
        Ok(())
    }
}
