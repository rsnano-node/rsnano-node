use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;
use rsnano_types::Account;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct GetWalletRepresentativeArgs {
    /// Gets the representative of the supplied <wallet>
    #[arg(long)]
    wallet: String,
    /// Optional password to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl GetWalletRepresentativeArgs {
    pub(crate) fn get_wallet_representative(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();

        let representative = node
            .wallets
            .get_representative(wallet_id)
            .map_err(|e| anyhow!("Failed to get wallet representative: {:?}", e))?;

        println!(
            "Representative: {:?}",
            Account::from(representative).encode_account()
        );

        Ok(())
    }
}
