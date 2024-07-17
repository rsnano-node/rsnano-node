use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use wallet::{
    add_adhoc::AddAdhocArgs, change_seed::ChangeSeedArgs, create::CreateArgs,
    decrypt_unsafe::DecryptUnsafeArgs, destroy::DestroyArgs, import::ImportArgs, list::ListArgs,
    remove::RemoveArgs, representative_get::RepresentativeGetArgs,
    representative_set::RepresentativeSetArgs,
};

pub(crate) mod accounts;
pub(crate) mod clear;
pub(crate) mod database;
pub(crate) mod keys;
pub(crate) mod node;
pub(crate) mod wallet;

#[derive(Parser)]
pub(crate) struct WalletCLI {
    #[command(subcommand)]
    pub subcommand: Option<WalletSubcommands>,
}

#[derive(Subcommand)]
pub(crate) enum WalletSubcommands {
    /// Creates a new wallet with optional <seed> and optional <password>, and prints the ID.
    ///
    /// Note the legacy --key option can still be used and will function the same as --seed.
    /// Use --wallet-list to retrieve the wallet ID in the future.
    Create(CreateArgs),
    /// Destroys <wallet> and all keys it contains.
    Destroy(DestroyArgs),
    /// Imports keys in <file> using <password> in to <wallet>.
    Import(ImportArgs),
    /// Insert <key> in to <wallet>.
    AddAdhoc(AddAdhocArgs),
    /// Changes seed for <wallet> to <key>.
    ChangeSeed(ChangeSeedArgs),
    /// Prints default representative for <wallet>.
    RepresentativeGet(RepresentativeGetArgs),
    /// Set <account> as default representative for <wallet>.
    RepresentativeSet(RepresentativeSetArgs),
    /// Remove <account> from <wallet>.
    Remove(RemoveArgs),
    /// Decrypts <wallet> using <password>, !!THIS WILL PRINT YOUR PRIVATE KEY TO STDOUT!
    DecryptUnsafe(DecryptUnsafeArgs),
    /// Dumps wallet IDs and public keys.
    List(ListArgs),
}

impl WalletCLI {
    pub(crate) fn run(&self) -> Result<()> {
        match &self.subcommand {
            Some(WalletSubcommands::List(args)) => args.wallet_list()?,
            Some(WalletSubcommands::Create(args)) => args.wallet_create()?,
            Some(WalletSubcommands::Destroy(args)) => args.wallet_destroy()?,
            Some(WalletSubcommands::AddAdhoc(args)) => args.wallet_add_adhoc()?,
            Some(WalletSubcommands::ChangeSeed(args)) => args.wallet_change_seed()?,
            Some(WalletSubcommands::Import(args)) => args.wallet_import()?,
            Some(WalletSubcommands::Remove(args)) => args.wallet_remove()?,
            Some(WalletSubcommands::DecryptUnsafe(args)) => args.wallet_decrypt_unsafe()?,
            Some(WalletSubcommands::RepresentativeGet(args)) => args.wallet_representative_get()?,
            Some(WalletSubcommands::RepresentativeSet(args)) => args.wallet_representative_set()?,
            None => WalletCLI::command().print_long_help()?,
        }

        Ok(())
    }
}
