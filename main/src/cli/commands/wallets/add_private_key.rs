use crate::cli::GlobalArgs;
use crate::cli::commands::wallets::WalletContext;
use anyhow::anyhow;
use clap::Parser;
use rsnano_types::RawKey;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct AddPrivateKeyArgs {
    /// Adds the key to the supplied wallet
    #[arg(long)]
    wallet: String,
    /// Adds the supplied <private_key> to the wallet
    #[arg(long)]
    private_key: String,
    /// Optional <password> to unlock the wallet
    #[arg(long)]
    password: Option<String>,
}

impl AddPrivateKeyArgs {
    pub(crate) fn add_key(&self, global_args: GlobalArgs) -> anyhow::Result<()> {
        let context = WalletContext::from_args(&global_args, &self.wallet, &self.password)?;
        let (node, wallet_id) = context.into_parts();
        let private_key =
            RawKey::decode_hex(&self.private_key).ok_or_else(|| anyhow!("Invalid private key"))?;

        node.wallets
            .insert_adhoc2(&wallet_id, &private_key, false)
            .map_err(|e| anyhow!("Failed to insert key: {:?}", e))?;

        Ok(())
    }
}
