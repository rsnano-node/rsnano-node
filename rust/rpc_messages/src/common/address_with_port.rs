use serde::{Deserialize, Serialize};
use std::net::Ipv6Addr;

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AddressWithPortArgs {
    pub address: Ipv6Addr,
    pub port: u16,
}

impl AddressWithPortArgs {
    pub fn new(address: Ipv6Addr, port: u16) -> Self {
        Self { address, port }
    }
}

#[cfg(test)]
mod tests {
    use super::AddressWithPortArgs;
    use serde_json::to_string_pretty;
    use std::{net::Ipv6Addr, str::FromStr};

    #[test]
    fn serialize_address_with_port_arg() {
        assert_eq!(
            to_string_pretty(&AddressWithPortArgs::new(
                Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap(),
                1024
            ))
            .unwrap(),
            r#"{
  "address": "::ffff:192.169.0.1",
  "port": 1024
}"#
        )
    }

    #[test]
    fn deserialize_address_with_port_arg() {
        let json_str = r#"{
"address": "::ffff:192.169.0.1",
"port": 1024
}"#;
        let deserialized: AddressWithPortArgs = serde_json::from_str(json_str).unwrap();
        let expected_arg =
            AddressWithPortArgs::new(Ipv6Addr::from_str("::ffff:192.169.0.1").unwrap(), 1024);
        assert_eq!(deserialized, expected_arg);
    }
}
