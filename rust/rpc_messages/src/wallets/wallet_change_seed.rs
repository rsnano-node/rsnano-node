use crate::RpcCommand;
use rsnano_core::{Account, RawKey, WalletId};
use serde::{Deserialize, Serialize};

impl RpcCommand {
    pub fn wallet_change_seed(args: WalletChangeSeedArgs) -> Self {
        RpcCommand::WalletChangeSeed(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletChangeSeedArgs {
    pub wallet: WalletId,
    pub seed: RawKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

impl WalletChangeSeedArgs {
    pub fn new(wallet: WalletId, seed: RawKey) -> WalletChangeSeedArgs {
        WalletChangeSeedArgs {
            wallet,
            seed,
            count: None,
        }
    }

    pub fn builder(wallet: WalletId, seed: RawKey) -> WalletChangeSeedArgsBuilder {
        WalletChangeSeedArgsBuilder::new(wallet, seed)
    }
}

pub struct WalletChangeSeedArgsBuilder {
    args: WalletChangeSeedArgs,
}

impl WalletChangeSeedArgsBuilder {
    fn new(wallet: WalletId, seed: RawKey) -> Self {
        Self {
            args: WalletChangeSeedArgs {
                wallet,
                seed,
                count: None,
            },
        }
    }

    pub fn count(mut self, count: u32) -> Self {
        self.args.count = Some(count);
        self
    }

    pub fn build(self) -> WalletChangeSeedArgs {
        self.args
    }
}

pub struct WalletWithSeedArgs {
    pub wallet: WalletId,
    pub seed: RawKey,
}

impl WalletWithSeedArgs {
    pub fn new(wallet: WalletId, seed: RawKey) -> Self {
        Self { wallet, seed }
    }
}

impl From<WalletWithSeedArgs> for WalletChangeSeedArgs {
    fn from(wallet_with_seed: WalletWithSeedArgs) -> Self {
        Self {
            wallet: wallet_with_seed.wallet,
            seed: wallet_with_seed.seed,
            count: None,
        }
    }
}
#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WalletChangeSeedDto {
    pub success: String,
    pub last_restored_account: Account,
    pub restored_count: u32,
}

impl WalletChangeSeedDto {
    pub fn new(last_restored_account: Account, restored_count: u32) -> Self {
        Self {
            success: String::new(),
            last_restored_account,
            restored_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_core::{RawKey, WalletId};

    #[test]
    fn serialize_wallet_change_seed_command() {
        let wallet = WalletId::zero();
        let seed = RawKey::zero();
        let count = 10;

        let args = WalletChangeSeedArgsBuilder::new(wallet, seed)
            .count(count)
            .build();

        let command = RpcCommand::wallet_change_seed(args);
        let serialized = serde_json::to_string(&command).unwrap();
        let deserialized: RpcCommand = serde_json::from_str(&serialized).unwrap();

        assert_eq!(command, deserialized);
    }

    #[test]
    fn deserialize_wallet_change_seed_command() {
        let json = r#"{"action":"wallet_change_seed","wallet":"000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F","seed":"74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE"}"#;
        let deserialized: RpcCommand = serde_json::from_str(json).unwrap();

        match deserialized {
            RpcCommand::WalletChangeSeed(args) => {
                assert_eq!(
                    args.wallet,
                    WalletId::decode_hex(
                        "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
                    )
                    .unwrap()
                );
                assert_eq!(
                    args.seed,
                    RawKey::decode_hex(
                        "74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE"
                    )
                    .unwrap()
                );
                assert_eq!(args.count, None);
            }
            _ => panic!("Deserialized to wrong variant"),
        }
    }

    #[test]
    fn deserialize_wallet_change_seed_args_count_none() {
        let json = r#"{"wallet":"000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F","seed":"74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE"}"#;
        let deserialized: WalletChangeSeedArgs = serde_json::from_str(json).unwrap();

        assert_eq!(
            deserialized.wallet,
            WalletId::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
            )
            .unwrap()
        );
        assert_eq!(
            deserialized.seed,
            RawKey::decode_hex("74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE")
                .unwrap()
        );
        assert_eq!(deserialized.count, None);
    }

    #[test]
    fn deserialize_wallet_change_seed_args_count_some() {
        let json = r#"{"wallet":"000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F","seed":"74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE","count":5}"#;
        let deserialized: WalletChangeSeedArgs = serde_json::from_str(json).unwrap();

        assert_eq!(
            deserialized.wallet,
            WalletId::decode_hex(
                "000D1BAEC8EC208142C99059B393051BAC8380F9B5A2E6B2489A277D81789F3F"
            )
            .unwrap()
        );
        assert_eq!(
            deserialized.seed,
            RawKey::decode_hex("74F2B37AAD20F4A260F0A5B3CB3D7FB51673212263E58A380BC10474BB039CEE")
                .unwrap()
        );
        assert_eq!(deserialized.count, Some(5));
    }

    #[test]
    fn wallet_change_seed_args_builder() {
        let wallet = WalletId::zero();
        let seed = RawKey::zero();
        let count = 10;

        let args = WalletChangeSeedArgs::builder(wallet, seed)
            .count(count)
            .build();

        assert_eq!(args.wallet, wallet);
        assert_eq!(args.seed, seed);
        assert_eq!(args.count, Some(count));
    }

    #[test]
    fn wallet_change_seed_args_builder_without_count() {
        let wallet = WalletId::zero();
        let seed = RawKey::zero();

        let args = WalletChangeSeedArgs::builder(wallet, seed).build();

        assert_eq!(args.wallet, wallet);
        assert_eq!(args.seed, seed);
        assert_eq!(args.count, None);
    }

    #[test]
    fn wallet_change_seed_args_from_wallet_with_seed() {
        let wallet = WalletId::zero();
        let seed = RawKey::zero();
        let wallet_with_seed = WalletWithSeedArgs::new(wallet, seed);

        let args: WalletChangeSeedArgs = wallet_with_seed.into();

        assert_eq!(args.wallet, wallet);
        assert_eq!(args.seed, seed);
        assert_eq!(args.count, None);
    }

    #[test]
    fn deserialize_wallet_change_seed_dto() {
        let json = r#"{"success":"","last_restored_account":"nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3","restored_count":15}"#;
        let deserialized: WalletChangeSeedDto = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.success, "");
        assert_eq!(
            deserialized.last_restored_account,
            Account::decode_account(
                "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3"
            )
            .unwrap()
        );
        assert_eq!(deserialized.restored_count, 15);
    }
}
