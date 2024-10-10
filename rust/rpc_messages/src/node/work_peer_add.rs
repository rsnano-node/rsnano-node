use std::net::Ipv6Addr;
use crate::RpcCommand;

use super::AddressWithPortArg;

impl RpcCommand {
    pub fn work_peer_add(address: Ipv6Addr, port: u16) -> Self {
        Self::WorkPeerAdd(AddressWithPortArg::new(address, port))
    }
}