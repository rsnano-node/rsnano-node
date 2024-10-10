use num::FromPrimitive;
use num_derive::FromPrimitive;
use rsnano_messages::Message;
use rsnano_network::{ChannelDirection, ChannelId};
use rsnano_node::NodeCallbacks;
use std::sync::{
    atomic::{AtomicU8, AtomicUsize, Ordering},
    Arc, RwLock,
};

#[derive(Clone)]
pub(crate) struct RecordedMessage {
    pub channel_id: ChannelId,
    pub message: Message,
    pub direction: ChannelDirection,
}

#[derive(FromPrimitive, PartialEq, Eq)]
pub enum NodeState {
    Starting,
    Started,
    Stopping,
    Stopped,
}

pub(crate) struct AppModel {
    node_state: AtomicU8,
    pub published: AtomicUsize,
    pub inbound: AtomicUsize,
    pub messages: RwLock<Vec<RecordedMessage>>,
}

impl AppModel {
    pub(crate) fn new() -> Self {
        Self {
            node_state: AtomicU8::new(NodeState::Stopped as u8),
            published: AtomicUsize::new(0),
            inbound: AtomicUsize::new(0),
            messages: RwLock::new(Vec::new()),
        }
    }

    pub fn record_message(&self, msg: RecordedMessage) {
        let direction = msg.direction;
        self.messages.write().unwrap().push(msg);
        match direction {
            ChannelDirection::Inbound => self.inbound.fetch_add(1, Ordering::SeqCst),
            ChannelDirection::Outbound => self.published.fetch_add(1, Ordering::SeqCst),
        };
    }

    pub fn get_message(&self, index: usize) -> Option<RecordedMessage> {
        self.messages.read().unwrap().get(index).cloned()
    }

    pub(crate) fn message_count(&self) -> usize {
        self.messages.read().unwrap().len()
    }

    pub(crate) fn node_state(&self) -> NodeState {
        FromPrimitive::from_u8(self.node_state.load(Ordering::SeqCst)).unwrap()
    }

    pub(crate) fn set_node_state(&self, state: NodeState) {
        self.node_state.store(state as u8, Ordering::SeqCst);
    }
}

pub(crate) fn make_node_callbacks(model: Arc<AppModel>) -> NodeCallbacks {
    let model2 = model.clone();
    let model3 = model.clone();
    NodeCallbacks::builder()
        .on_publish(move |channel_id, message| {
            let recorded = RecordedMessage {
                channel_id,
                message: message.clone(),
                direction: ChannelDirection::Outbound,
            };
            model.record_message(recorded);
        })
        .on_inbound(move |channel_id, message| {
            let recorded = RecordedMessage {
                channel_id,
                message: message.clone(),
                direction: ChannelDirection::Inbound,
            };
            model2.record_message(recorded);
        })
        .on_inbound_dropped(move |channel_id, message| {
            let recorded = RecordedMessage {
                channel_id,
                message: message.clone(),
                direction: ChannelDirection::Inbound,
            };
            model3.record_message(recorded);
        })
        .finish()
}
