use crate::{
    app_model::{make_node_callbacks, AppModel, NodeState},
    node_factory::NodeFactory,
    nullable_runtime::NullableRuntime,
};
use rsnano_core::Networks;
use rsnano_node::{Node, NodeExt};
use std::sync::Arc;

pub(crate) struct NodeRunner {
    node_factory: NodeFactory,
    runtime: Arc<NullableRuntime>,
    pub node: Option<Arc<Node>>,
}

impl NodeRunner {
    pub(crate) fn new(runtime: Arc<NullableRuntime>, node_factory: NodeFactory) -> Self {
        Self {
            node_factory,
            runtime,
            node: None,
        }
    }

    pub(crate) fn start_beta_node(&mut self, model: Arc<AppModel>) {
        let callbacks = make_node_callbacks(model.clone());

        let node = self
            .node_factory
            .create_node(Networks::NanoBetaNetwork, callbacks);

        let node2 = node.clone();

        model.set_node_state(NodeState::Starting);
        self.node = Some(node);

        self.runtime.spawn_blocking(move || {
            node2.start();
            model.set_node_state(NodeState::Started)
        });
    }

    pub(crate) fn stop_node(&mut self, model: Arc<AppModel>) {
        if let Some(node) = self.node.take() {
            {
                model.set_node_state(NodeState::Stopping);
                self.runtime.spawn_blocking(move || {
                    node.stop();
                    model.set_node_state(NodeState::Stopped)
                });
            }
        }
    }
}
