mod add_private_key;
mod change_wallet_seed;
mod create_account;
mod create_wallet;
mod decrypt_wallet;
mod destroy_wallet;
mod get_wallet_representative;
mod import_keys;
mod remove_account;
mod set_wallet_representative;
mod wallet_context;

use crate::cli::{GlobalArgs, build_node};
use add_private_key::AddPrivateKeyArgs;
use anyhow::{Result, anyhow};
use change_wallet_seed::ChangeWalletSeedArgs;
use clap::{CommandFactory, Parser, Subcommand};
use create_account::CreateAccountArgs;
use create_wallet::CreateWalletArgs;
use decrypt_wallet::DecryptWalletArgs;
use destroy_wallet::DestroyWalletArgs;
use get_wallet_representative::GetWalletRepresentativeArgs;
use import_keys::ImportKeysArgs;
use remove_account::RemoveAccountArgs;
use rsnano_types::Account;
use set_wallet_representative::SetWalletRepresentativeArgs;
pub(crate) use wallet_context::WalletContext;

#[derive(Parser, PartialEq, Debug)]
pub(crate) struct WalletsCommand {
    #[command(subcommand)]
    pub subcommand: Option<WalletSubcommands>,
}

#[derive(Subcommand, PartialEq, Debug)]
pub(crate) enum WalletSubcommands {
    /// Creates a new account in a wallet
    CreateAccount(CreateAccountArgs),
    /// Creates a new wallet
    CreateWallet(CreateWalletArgs),
    /// Destroys a wallet
    Destroy(DestroyWalletArgs),
    /// Imports keys from a file to a wallet
    ImportKeys(ImportKeysArgs),
    /// Adds a private_key to a wallet
    AddPrivateKey(AddPrivateKeyArgs),
    /// Changes the seed of a wallet
    ChangeWalletSeed(ChangeWalletSeedArgs),
    /// Prints the representative of a wallet
    GetWalletRepresentative(GetWalletRepresentativeArgs),
    /// Sets the representative of a wallet
    SetWalletRepresentative(SetWalletRepresentativeArgs),
    /// Removes an account from a wallet
    RemoveAccount(RemoveAccountArgs),
    /// Decrypts a wallet (WARNING: THIS WILL PRINT YOUR PRIVATE KEY TO STDOUT!)
    DecryptWallet(DecryptWalletArgs),
    /// List all wallets and their public keys
    List,
    /// Removes all send IDs from the wallets (dangerous: not intended for production use)
    ClearSendIds,
}

pub(crate) fn run_wallets_command(global_args: GlobalArgs, cmd: WalletsCommand) -> Result<()> {
    match cmd.subcommand {
        Some(WalletSubcommands::List) => list_wallets(global_args)?,
        Some(WalletSubcommands::CreateWallet(args)) => args.create_wallet(global_args)?,
        Some(WalletSubcommands::CreateAccount(args)) => args.create_account(global_args)?,
        Some(WalletSubcommands::Destroy(args)) => args.destroy_wallet(global_args)?,
        Some(WalletSubcommands::AddPrivateKey(args)) => args.add_key(global_args)?,
        Some(WalletSubcommands::ChangeWalletSeed(args)) => args.change_wallet_seed(global_args)?,
        Some(WalletSubcommands::ImportKeys(args)) => args.import_keys(global_args)?,
        Some(WalletSubcommands::RemoveAccount(args)) => args.remove_account(global_args)?,
        Some(WalletSubcommands::DecryptWallet(args)) => args.decrypt_wallet(global_args)?,
        Some(WalletSubcommands::GetWalletRepresentative(args)) => {
            args.get_wallet_representative(global_args)?
        }
        Some(WalletSubcommands::SetWalletRepresentative(args)) => {
            args.set_representative_wallet(global_args)?
        }
        Some(WalletSubcommands::ClearSendIds) => clear_send_ids(global_args)?,
        None => WalletsCommand::command().print_long_help()?,
    }

    Ok(())
}

impl WalletsCommand {}

fn list_wallets(global_args: GlobalArgs) -> Result<()> {
    let node = build_node(&global_args)?;
    let wallet_ids = node.wallets.get_wallet_ids();

    for wallet_id in wallet_ids {
        println!("{:?}", wallet_id);
        let accounts = node
            .wallets
            .get_accounts_of_wallet(&wallet_id)
            .map_err(|e| anyhow!("Failed to get accounts of wallets: {:?}", e))?;
        if !accounts.is_empty() {
            for account in accounts {
                println!("{:?}", Account::encode_account(&account));
            }
        }
    }

    Ok(())
}

fn clear_send_ids(global_args: GlobalArgs) -> anyhow::Result<()> {
    let node = build_node(&global_args)?;
    node.wallets.clear_send_ids();
    println!("Send IDs deleted");
    Ok(())
}
