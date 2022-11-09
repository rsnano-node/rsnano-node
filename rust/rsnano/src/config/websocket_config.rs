use crate::utils::TomlWriter;

use super::NetworkConstants;
use anyhow::Result;
use std::net::Ipv6Addr;

pub struct WebsocketConfig {
    pub enabled: bool,
    pub port: u16,
    pub address: String,
}

impl WebsocketConfig {
    pub fn new(network: &NetworkConstants) -> Self {
        Self {
            enabled: false,
            port: network.default_websocket_port,
            address: Ipv6Addr::LOCALHOST.to_string(),
        }
    }

    pub fn serialize_toml(&self, toml: &mut dyn TomlWriter) -> Result<()> {
        toml.put_bool(
            "enable",
            self.enabled,
            "Enable or disable WebSocket server.\ntype:bool",
        )?;
        toml.put_str(
            "address",
            &self.address,
            "WebSocket server bind address.\ntype:string,ip",
        )?;
        toml.put_u16(
            "port",
            self.port,
            "WebSocket server listening port.\ntype:uint16",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::work::WorkThresholds;

    use super::*;

    #[test]
    fn websocket_config() {
        let cfg = WebsocketConfig::new(&NetworkConstants::new(
            WorkThresholds::publish_full().clone(),
            crate::config::Networks::NanoLiveNetwork,
        ));
        assert_eq!(cfg.enabled, false);
        assert_eq!(cfg.port, 7078);
        assert_eq!(cfg.address, "::1");
    }
}
