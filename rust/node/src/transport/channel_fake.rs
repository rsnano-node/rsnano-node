use super::{
    AsyncBufferReader, BandwidthLimitType, BufferDropPolicy, Channel, ChannelDirection, ChannelId,
    ChannelMode, OutboundBandwidthLimiter, TrafficType, WriteCallback,
};
use crate::{
    stats::{DetailType, Direction, StatType, Stats},
    utils::{AsyncRuntime, ErrorCode},
};
use async_trait::async_trait;
use rsnano_core::Account;
use rsnano_messages::{Message, MessageSerializer, ProtocolInfo};
use std::{
    net::SocketAddrV6,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct FakeChannelData {
    last_bootstrap_attempt: SystemTime,
    last_packet_received: SystemTime,
    last_packet_sent: SystemTime,
    node_id: Option<Account>,
}

pub struct ChannelFake {
    channel_id: ChannelId,
    async_rt: Weak<AsyncRuntime>,
    channel_mutex: Mutex<FakeChannelData>,
    limiter: Arc<OutboundBandwidthLimiter>,
    stats: Arc<Stats>,
    endpoint: SocketAddrV6,
    closed: AtomicBool,
    protocol: ProtocolInfo,
    message_serializer: Mutex<MessageSerializer>, // TODO remove Mutex!
}

impl ChannelFake {
    pub fn new(
        now: SystemTime,
        channel_id: ChannelId,
        async_rt: &Arc<AsyncRuntime>,
        limiter: Arc<OutboundBandwidthLimiter>,
        stats: Arc<Stats>,
        endpoint: SocketAddrV6,
        protocol: ProtocolInfo,
    ) -> Self {
        Self {
            channel_id,
            async_rt: Arc::downgrade(async_rt),
            channel_mutex: Mutex::new(FakeChannelData {
                last_bootstrap_attempt: UNIX_EPOCH,
                last_packet_received: now,
                last_packet_sent: now,
                node_id: None,
            }),
            limiter,
            stats,
            endpoint,
            closed: AtomicBool::new(false),
            protocol,
            message_serializer: Mutex::new(MessageSerializer::new(protocol)),
        }
    }

    pub fn send_buffer(
        &self,
        buffer: &Arc<Vec<u8>>,
        callback_a: Option<WriteCallback>,
        _policy_a: BufferDropPolicy,
        _traffic_type: TrafficType,
    ) {
        let size = buffer.len();
        if let Some(cb) = callback_a {
            if let Some(async_rt) = self.async_rt.upgrade() {
                async_rt.post(Box::new(move || {
                    cb(ErrorCode::new(), size);
                }))
            }
        }
    }
}

#[async_trait]
impl Channel for ChannelFake {
    fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    fn get_last_bootstrap_attempt(&self) -> SystemTime {
        self.channel_mutex.lock().unwrap().last_bootstrap_attempt
    }

    fn set_last_bootstrap_attempt(&self, time: SystemTime) {
        self.channel_mutex.lock().unwrap().last_bootstrap_attempt = time;
    }

    fn get_last_packet_received(&self) -> SystemTime {
        self.channel_mutex.lock().unwrap().last_packet_received
    }

    fn set_last_packet_received(&self, instant: SystemTime) {
        self.channel_mutex.lock().unwrap().last_packet_received = instant;
    }

    fn get_last_packet_sent(&self) -> SystemTime {
        self.channel_mutex.lock().unwrap().last_packet_sent
    }

    fn set_last_packet_sent(&self, instant: SystemTime) {
        self.channel_mutex.lock().unwrap().last_packet_sent = instant;
    }

    fn get_node_id(&self) -> Option<Account> {
        self.channel_mutex.lock().unwrap().node_id
    }

    fn set_node_id(&self, id: Account) {
        self.channel_mutex.lock().unwrap().node_id = Some(id);
    }

    fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    fn get_type(&self) -> super::TransportType {
        super::TransportType::Fake
    }

    fn remote_addr(&self) -> SocketAddrV6 {
        self.endpoint
    }

    fn peering_endpoint(&self) -> Option<SocketAddrV6> {
        Some(self.endpoint)
    }

    fn network_version(&self) -> u8 {
        self.protocol.version_using
    }

    fn direction(&self) -> ChannelDirection {
        ChannelDirection::Inbound
    }

    fn mode(&self) -> ChannelMode {
        ChannelMode::Realtime
    }

    fn set_mode(&self, _mode: ChannelMode) {}

    fn try_send(
        &self,
        _message: &Message,
        _drop_policy: BufferDropPolicy,
        _traffic_type: TrafficType,
    ) {
    }

    async fn send_buffer(
        &self,
        _buffer: &Arc<Vec<u8>>,
        _traffic_type: TrafficType,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send(&self, _message: &Message, _traffic_type: TrafficType) -> anyhow::Result<()> {
        Ok(())
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn local_addr(&self) -> SocketAddrV6 {
        self.endpoint
    }

    fn set_timeout(&self, _timeout: Duration) {}
}

#[async_trait]
impl AsyncBufferReader for ChannelFake {
    async fn read(&self, _buffer: &mut [u8], _count: usize) -> anyhow::Result<()> {
        Err(anyhow!("AsyncBufferReader not implemented for ChannelFake"))
    }
}
