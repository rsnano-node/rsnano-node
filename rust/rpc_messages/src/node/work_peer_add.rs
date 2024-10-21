use crate::{AddressWithPortArgs, RpcCommand};

impl RpcCommand {
    pub fn work_peer_add(args: AddressWithPortArgs) -> Self {
        Self::WorkPeerAdd(args)
    }
}
