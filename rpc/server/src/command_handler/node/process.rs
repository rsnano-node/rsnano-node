use anyhow::{anyhow, bail};

use rsnano_ledger::{BlockError, BlockSource, LedgerSet};
use rsnano_network::ChannelId;
use rsnano_node::block_processing::BlockContext;
use rsnano_rpc_messages::{BlockSubTypeDto, HashRpcMessage, ProcessArgs, StartedResponse};
use rsnano_types::{Block, BlockBase, BlockType};

use crate::command_handler::RpcCommandHandler;

impl RpcCommandHandler {
    pub(crate) fn process(&self, args: ProcessArgs) -> anyhow::Result<serde_json::Value> {
        let is_async = args.is_async.unwrap_or_default().inner();
        let block: Block = args.block.into();

        // State blocks subtype check
        if let Block::State(state) = &block
            && let Some(subtype) = args.subtype
        {
            let any = self.node.ledger.any();
            if !state.previous().is_zero() && !any.block_exists(&state.previous()) {
                bail!("Gap previous block")
            } else {
                let balance = any.account_balance(&state.account());
                match subtype {
                    BlockSubTypeDto::Send => {
                        if balance <= state.balance() {
                            bail!("Invalid block balance for given subtype");
                        }
                        // Send with previous == 0 fails balance check. No previous != 0 check required
                    }
                    BlockSubTypeDto::Receive => {
                        if balance > state.balance() {
                            bail!("Invalid block balance for given subtype");
                        }
                        // Receive can be point to open block. No previous != 0 check required
                    }
                    BlockSubTypeDto::Open => {
                        if !state.previous().is_zero() {
                            bail!("Invalid previous block for given subtype");
                        }
                    }
                    BlockSubTypeDto::Change => {
                        if balance != state.balance() {
                            bail!("Invalid block balance for given subtype");
                        } else if state.previous().is_zero() {
                            bail!("Invalid previous block for given subtype");
                        }
                    }
                    BlockSubTypeDto::Epoch => {
                        if balance != state.balance() {
                            bail!("Invalid block balance for given subtype");
                        } else if !self.node.ledger.is_epoch_link(&state.link()) {
                            bail!("Invalid epoch link");
                        }
                    }
                    BlockSubTypeDto::Unknown => bail!("Invalid block subtype"),
                }
            }
        }

        if !self.node.network_params.work.validate_entry_block(&block) {
            bail!("Block work is less than threshold");
        }

        if !is_async {
            let hash = block.hash();
            let result = self.node.process_local(block.clone());
            match result {
                Ok(()) => Ok(serde_json::to_value(HashRpcMessage::new(hash))?),
                Err(BlockError::GapPrevious) => Err(anyhow!("Gap previous block")),
                Err(BlockError::BadSignature) => Err(anyhow!("Bad signature")),
                Err(BlockError::Old) => Err(anyhow!("Old block")),
                Err(BlockError::NegativeSpend) => Err(anyhow!("Negative spend")),
                Err(BlockError::Fork) => {
                    if args.force.unwrap_or_default().inner() {
                        self.node.active.erase(&block.qualified_root());
                        self.node.block_processor_queue.push(BlockContext::new(
                            block,
                            BlockSource::Forced,
                            ChannelId::LOOPBACK,
                        ));
                        Ok(serde_json::to_value(HashRpcMessage::new(hash))?)
                    } else {
                        Err(anyhow!("Fork"))
                    }
                }
                Err(BlockError::Unreceivable) => Err(anyhow!("Unreceivable")),
                Err(BlockError::GapSource) => Err(anyhow!("Gap source block")),
                Err(BlockError::GapEpochOpenPending) => {
                    Err(anyhow!("Gap pending for open epoch block"))
                }
                Err(BlockError::OpenedBurnAccount) => {
                    Err(anyhow!("Block attempts to open the burn account"))
                }
                Err(BlockError::BalanceMismatch) => {
                    Err(anyhow!("Balance and amount delta do not match"))
                }
                Err(BlockError::RepresentativeMismatch) => Err(anyhow!("Representative mismatch")),
                Err(BlockError::BlockPosition) => {
                    Err(anyhow!("This block cannot follow the previous block"))
                }
                Err(BlockError::InsufficientWork) => Err(anyhow!("Block work is insufficient")),
                Err(BlockError::Conflict) => Err(anyhow!("Conflict while processing block")),
            }
        } else if block.block_type() == BlockType::State {
            self.node.block_processor_queue.push(BlockContext::new(
                block,
                BlockSource::Local,
                ChannelId::LOOPBACK,
            ));
            Ok(serde_json::to_value(StartedResponse::new(true))?)
        } else {
            Err(anyhow!("Must be a state block"))
        }
    }
}
