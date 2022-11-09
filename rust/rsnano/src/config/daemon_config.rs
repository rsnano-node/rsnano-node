use crate::{utils::TomlWriter, NetworkParams};
use anyhow::Result;

use super::{Logging, NodeConfig, NodePowServerConfig, NodeRpcConfig, OpenclConfig};

pub struct DaemonConfig {
    pub rpc_enable: bool,
    pub rpc: NodeRpcConfig,
    pub node: NodeConfig,
    pub opencl: OpenclConfig,
    pub opencl_enable: bool,
    pub pow_server: NodePowServerConfig,
}

impl DaemonConfig {
    pub fn new(network_params: &NetworkParams) -> Result<Self> {
        Ok(Self {
            rpc_enable: false,
            node: NodeConfig::new(None, Logging::new(), network_params),
            opencl: OpenclConfig::new(),
            opencl_enable: false,
            pow_server: NodePowServerConfig::new()?,
            rpc: NodeRpcConfig::new()?,
        })
    }

    pub fn serialize_toml(&self, toml: &mut dyn TomlWriter) -> Result<()> {
        toml.put_child("rpc", &mut |rpc| {
            self.rpc.serialize_toml(rpc)?;
            rpc.put_bool(
                "enable",
                self.rpc_enable,
                "Enable or disable RPC\ntype:bool",
            )?;
            Ok(())
        })?;

        toml.put_child("node", &mut |node| self.node.serialize_toml(node))?;

        toml.put_child("opencl", &mut |opencl| {
            self.opencl.serialize_toml(opencl)?;
            opencl.put_bool(
                "enable",
                self.opencl_enable,
                "Enable or disable OpenCL work generation\ntype:bool",
            )?;
            Ok(())
        })?;

        toml.put_child("nano_pow_server", &mut |pow_server| {
            self.pow_server.serialize_toml(pow_server)
        })?;

        Ok(())
    }
}
