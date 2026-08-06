use std::net::{AddrParseError, IpAddr, SocketAddr};
use std::path::Path;

use rsnano_node::config::read_toml_file;
use rsnano_types::NetworkType;

const DEFAULT_GRPC_PORT: u16 = 7078;

#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    pub address: String,
    pub port: u16,
    pub enable_tls: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub enable_reflection: bool,
    pub stream_max_lag: usize,
    pub stream_send_timeout_ms: u64,
    pub shutdown_drain_ms: u64,
    pub api_keys: Vec<String>,
}

impl GrpcServerConfig {
    pub fn new() -> Self {
        Self {
            address: "::1".to_string(),
            port: DEFAULT_GRPC_PORT,
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
            enable_reflection: true,
            stream_max_lag: 1024,
            stream_send_timeout_ms: 500,
            shutdown_drain_ms: 5000,
            api_keys: Vec::new(),
        }
    }

    pub fn default_for(network: NetworkType) -> Self {
        let port = match network {
            NetworkType::NanoDevNetwork => 47078,
            NetworkType::NanoBetaNetwork => 57078,
            NetworkType::NanoTestNetwork => 17078,
            NetworkType::NanoLiveNetwork | NetworkType::Invalid => DEFAULT_GRPC_PORT,
        };
        Self {
            port,
            ..Self::new()
        }
    }

    pub fn load_from_data_path(
        network: NetworkType,
        data_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let mut config_path = data_path.as_ref().to_path_buf();
        config_path.push("config-grpc.toml");
        let mut result = Self::default_for(network);
        if config_path.exists() {
            let toml: GrpcServerToml = read_toml_file(&config_path)?;
            result.merge_toml(&toml);
        }
        Ok(result)
    }

    pub fn listening_addr(&self) -> Result<SocketAddr, AddrParseError> {
        let ip_addr: IpAddr = self.address.parse()?;
        Ok(SocketAddr::new(ip_addr, self.port))
    }

    pub fn merge_toml(&mut self, toml: &GrpcServerToml) {
        if let Some(address) = &toml.address {
            self.address = address.clone();
        }
        if let Some(port) = toml.port {
            self.port = port;
        }
        if let Some(enable_tls) = toml.enable_tls {
            self.enable_tls = enable_tls;
        }
        if let Some(cert_path) = &toml.tls_cert_path {
            self.tls_cert_path = Some(cert_path.clone());
        }
        if let Some(key_path) = &toml.tls_key_path {
            self.tls_key_path = Some(key_path.clone());
        }
        if let Some(enable_reflection) = toml.enable_reflection {
            self.enable_reflection = enable_reflection;
        }
        if let Some(max_lag) = toml.stream_max_lag {
            self.stream_max_lag = max_lag;
        }
        if let Some(timeout) = toml.stream_send_timeout_ms {
            self.stream_send_timeout_ms = timeout;
        }
        if let Some(drain_ms) = toml.shutdown_drain_ms {
            self.shutdown_drain_ms = drain_ms;
        }
        if let Some(keys) = &toml.api_keys {
            self.api_keys = keys.clone();
        }
    }
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GrpcServerToml {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub enable_tls: Option<bool>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub enable_reflection: Option<bool>,
    pub stream_max_lag: Option<usize>,
    pub stream_send_timeout_ms: Option<u64>,
    pub shutdown_drain_ms: Option<u64>,
    pub api_keys: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = GrpcServerConfig::new();
        assert_eq!(config.port, 7078);
        assert!(!config.enable_tls);
        assert_eq!(config.stream_max_lag, 1024);
        assert_eq!(config.shutdown_drain_ms, 5000);
    }

    #[test]
    fn network_default_ports() {
        assert_eq!(
            GrpcServerConfig::default_for(NetworkType::NanoDevNetwork).port,
            47078
        );
        assert_eq!(
            GrpcServerConfig::default_for(NetworkType::NanoBetaNetwork).port,
            57078
        );
        assert_eq!(
            GrpcServerConfig::default_for(NetworkType::NanoLiveNetwork).port,
            7078
        );
    }

    #[test]
    fn merge_toml_overrides() {
        let mut config = GrpcServerConfig::new();
        let toml = GrpcServerToml {
            address: Some("0.0.0.0".to_string()),
            port: Some(9999),
            enable_tls: None,
            tls_cert_path: None,
            tls_key_path: None,
            enable_reflection: None,
            stream_max_lag: Some(2048),
            stream_send_timeout_ms: None,
            shutdown_drain_ms: None,
            api_keys: Some(vec!["test-key".to_string()]),
        };
        config.merge_toml(&toml);
        assert_eq!(config.address, "0.0.0.0");
        assert_eq!(config.port, 9999);
        assert_eq!(config.stream_max_lag, 2048);
        assert_eq!(config.api_keys, vec!["test-key"]);
    }
}
