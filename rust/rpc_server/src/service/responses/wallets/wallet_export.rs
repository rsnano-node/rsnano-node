use rsnano_core::WalletId;
use rsnano_rpc_messages::JsonDto;
use serde_json::{to_string, to_string_pretty, Value};

pub async fn wallet_export(wallet: WalletId) -> String {
    to_string_pretty(&JsonDto::new(Value::String(to_string(&wallet).unwrap()))).unwrap()
}

#[cfg(test)]
mod tests {
    use rsnano_core::WalletId;
    use rsnano_node::wallets::WalletsExt;
    use test_helpers::{setup_rpc_client_and_server, System};

    #[test]
    fn wallet_export() {
        let mut system = System::new();
        let node = system.make_node();

        let (rpc_client, server) = setup_rpc_client_and_server(node.clone(), false);

        let wallet = WalletId::zero();

        node.wallets.create(wallet);

        let result = node
            .tokio
            .block_on(async { rpc_client.wallet_export(wallet).await.unwrap() });

        assert_eq!(result.json, serde_json::to_string(&wallet).unwrap());

        server.abort();
    }
}