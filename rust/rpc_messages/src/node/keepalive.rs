use crate::{common::AddressWithPortArgs, RpcCommand};
use std::net::Ipv6Addr;

impl RpcCommand {
    pub fn keepalive(address: Ipv6Addr, port: u16) -> Self {
        Self::Keepalive(AddressWithPortArgs::new(address, port))
    }
}

#[cfg(test)]
mod tests {
    use crate::RpcCommand;
    use serde_json::to_string_pretty;
    use std::{net::Ipv6Addr, str::FromStr};

    #[test]
    fn serialize_keepalive_command() {
        assert_eq!(
            to_string_pretty(&RpcCommand::keepalive(
                Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap(),
                1024
            ))
            .unwrap(),
            r#"{
  "action": "keepalive",
  "address": "::ffff:192.169.0.1",
  "port": 1024
}"#
        )
    }

    #[test]
    fn deserialize_keepalive_command() {
        let json_str = r#"{
"action": "keepalive",
"address": "::ffff:192.169.0.1",
"port": 1024
}"#;
        let deserialized: RpcCommand = serde_json::from_str(json_str).unwrap();
        let expected_command =
            RpcCommand::keepalive(Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap(), 1024);
        assert_eq!(deserialized, expected_command);
    }
}
