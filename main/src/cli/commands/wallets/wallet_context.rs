use crate::cli::{GlobalArgs, build_node};
use anyhow::{Result, anyhow};
use rsnano_node::Node;
use rsnano_types::WalletId;

/// Helper that guarantees the wallet exists and is unlocked before continuing.
pub(crate) struct WalletContext {
    node: Node,
    wallet_id: WalletId,
}

impl WalletContext {
    pub(crate) fn from_args(
        global_args: &GlobalArgs,
        wallet: &str,
        password: &Option<String>,
    ) -> Result<Self> {
        let node = build_node(global_args)?;
        Self::new(node, wallet, password.clone().unwrap_or_default())
    }

    pub(crate) fn new(node: Node, wallet: &str, password: String) -> Result<Self> {
        let wallet_id = WalletId::decode_hex(wallet).ok_or_else(|| anyhow!("Invalid wallet id"))?;
        Self::from_existing(node, wallet_id, password)
    }

    pub(crate) fn from_existing(node: Node, wallet_id: WalletId, password: String) -> Result<Self> {
        if !node.wallets.wallet_exists(&wallet_id) {
            return Err(anyhow!("Wallet not found"));
        }

        if !node.wallets.ensure_wallet_is_unlocked(wallet_id, &password) {
            return Err(anyhow!("Invalid wallet password"));
        }

        Ok(Self { node, wallet_id })
    }

    pub(crate) fn into_parts(self) -> (Node, WalletId) {
        (self.node, self.wallet_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_node::Node;

    fn create_wallet(node: &Node, password: &str) -> WalletId {
        let wallet_id = WalletId::random();
        node.wallets.create(wallet_id);
        node.wallets.rekey(&wallet_id, password).unwrap();
        wallet_id
    }

    #[test]
    fn fails_for_invalid_wallet_id() {
        let node = Node::new_null();
        let err = match WalletContext::new(node, "xyz", String::new()) {
            Ok(_) => panic!("expected invalid wallet id"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Invalid wallet id"));
    }

    #[test]
    fn fails_when_wallet_missing() {
        let node = Node::new_null();
        let missing_wallet_hex = WalletId::random().encode_hex();
        let err = match WalletContext::new(node, &missing_wallet_hex, String::new()) {
            Ok(_) => panic!("expected missing wallet"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Wallet not found"));
    }

    #[test]
    fn fails_when_password_is_wrong() {
        let node = Node::new_null();
        let wallet_id = create_wallet(&node, "correct-password");
        node.wallets.lock(&wallet_id).unwrap();
        let err = match WalletContext::new(node, &wallet_id.encode_hex(), "bad".into()) {
            Ok(_) => panic!("expected invalid password"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Invalid wallet password"));
    }

    #[test]
    fn returns_context_when_unlocked() {
        let node = Node::new_null();
        let wallet_id = create_wallet(&node, "pw");
        let context = WalletContext::new(node, &wallet_id.encode_hex(), "pw".into()).unwrap();
        let (_, returned_wallet_id) = context.into_parts();
        assert_eq!(returned_wallet_id, wallet_id);
    }
}
