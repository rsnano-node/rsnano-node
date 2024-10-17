use crate::RpcCommand;
use serde::{Deserialize, Serialize};
use std::net::Ipv6Addr;

impl RpcCommand {
    pub fn bootstrap(args: BootstrapArgs) -> Self {
        Self::Bootstrap(args)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BootstrapArgs {
    pub address: Ipv6Addr,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl BootstrapArgs {
    pub fn new(address: Ipv6Addr, port: u16) -> BootstrapArgs {
        BootstrapArgs {
            address,
            port,
            id: None,
        }
    }

    pub fn builder(address: Ipv6Addr, port: u16) -> BootstrapArgsBuilder {
        BootstrapArgsBuilder {
            args: BootstrapArgs::new(address, port),
        }
    }
}

pub struct BootstrapArgsBuilder {
    args: BootstrapArgs,
}

impl BootstrapArgsBuilder {
    pub fn id(mut self, id: String) -> Self {
        self.args.id = Some(id);
        self
    }

    pub fn build(self) -> BootstrapArgs {
        self.args
    }
}

#[cfg(test)]
mod tests {
    use crate::{node::BootstrapArgs, RpcCommand};
    use serde_json::{from_str, to_string_pretty};
    use std::{net::Ipv6Addr, str::FromStr};

    #[test]
    fn serialize_bootstrap_command_id_none() {
        let address = Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap();

        assert_eq!(
            to_string_pretty(&RpcCommand::bootstrap(BootstrapArgs::new(address, 1024))).unwrap(),
            r#"{
  "action": "bootstrap",
  "address": "::ffff:192.169.0.1",
  "port": 1024
}"#
        )
    }

    #[test]
    fn deserialize_bootstrap_command_none() {
        let address = Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap();
        let cmd = RpcCommand::bootstrap(BootstrapArgs::new(address, 1024));
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);
    }

    #[test]
    fn serialize_bootstrap_command_id_some() {
        let address = Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap();
        let args = BootstrapArgs::builder(address, 1024)
            .id("id".to_string())
            .build();
        assert_eq!(
            to_string_pretty(&RpcCommand::bootstrap(args)).unwrap(),
            r#"{
  "action": "bootstrap",
  "address": "::ffff:192.169.0.1",
  "port": 1024,
  "id": "id"
}"#
        )
    }

    #[test]
    fn deserialize_bootstrap_command_id_some() {
        let address = Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap();
        let args = BootstrapArgs::builder(address, 1024)
            .id("id".to_string())
            .build();
        let cmd = RpcCommand::bootstrap(args);
        let serialized = to_string_pretty(&cmd).unwrap();
        let deserialized: RpcCommand = from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);
    }
}
