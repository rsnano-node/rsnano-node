use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;
use rsnano_types::Account;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct RemoveAccountArgs {
    /// Removes the account from the supplied wallet
    #[arg(long)]
    wallet: String,
    /// Removes the account from the supplied wallet
    #[arg(long)]
    account: String,
    /// Optional password to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl RemoveAccountArgs {
    pub(crate) fn remove_account(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();
        let account = Account::parse(&self.account)
            .ok_or_else(|| anyhow!("Invalid account"))?
            .into();

        node.wallets
            .remove_key(&wallet_id, &account)
            .map_err(|e| anyhow!("Failed to remove account: {:?}", e))?;

        Ok(())
    }
}
