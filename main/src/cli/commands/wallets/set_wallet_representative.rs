use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;
use rsnano_types::Account;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct SetWalletRepresentativeArgs {
    /// Sets the representative for the supplied <wallet>
    #[arg(long)]
    wallet: String,
    /// Sets the supplied account as the wallet representative
    #[arg(long)]
    account: String,
    /// Optional password to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl SetWalletRepresentativeArgs {
    pub(crate) fn set_representative_wallet(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();
        let representative = Account::parse(&self.account)
            .ok_or_else(|| anyhow!("Invalid account"))?
            .into();

        node.wallets
            .set_representative(wallet_id, representative, false)
            .wait()
            .map_err(|e| anyhow!("Failed to set wallet representative: {:?}", e))?;

        Ok(())
    }
}
