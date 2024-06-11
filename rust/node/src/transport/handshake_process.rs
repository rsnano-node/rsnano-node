use super::{Socket, SynCookies};
use crate::stats::{DetailType, Direction, StatType, Stats};
use rsnano_core::{BlockHash, KeyPair, PublicKey};
use rsnano_messages::{
    Message, MessageSerializer, NodeIdHandshake, NodeIdHandshakeQuery, NodeIdHandshakeResponse,
    ProtocolInfo,
};
use std::{
    net::SocketAddrV6,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tracing::debug;

pub enum HandshakeStatus {
    Abort,
    Handshake,
    Realtime(PublicKey),
    Bootstrap,
}

/// Responsible for performing a correct handshake when connecting to another node
pub(crate) struct HandshakeProcess {
    genesis_hash: BlockHash,
    node_id: KeyPair,
    syn_cookies: Arc<SynCookies>,
    stats: Arc<Stats>,
    handshake_received: AtomicBool,
    remote_endpoint: SocketAddrV6,
    protocol: ProtocolInfo,
}

impl HandshakeProcess {
    pub(crate) fn new(
        genesis_hash: BlockHash,
        node_id: KeyPair,
        syn_cookies: Arc<SynCookies>,
        stats: Arc<Stats>,
        remote_endpoint: SocketAddrV6,
        protocol: ProtocolInfo,
    ) -> Self {
        Self {
            genesis_hash,
            node_id,
            syn_cookies,
            stats,
            handshake_received: AtomicBool::new(false),
            remote_endpoint,
            protocol,
        }
    }

    pub(crate) fn was_handshake_received(&self) -> bool {
        self.handshake_received.load(Ordering::SeqCst)
    }

    pub(crate) async fn process_handshake(
        &self,
        message: &NodeIdHandshake,
        socket: &Socket,
    ) -> HandshakeStatus {
        if message.query.is_none() && message.response.is_none() {
            self.stats.inc_dir(
                StatType::TcpServer,
                DetailType::HandshakeError,
                Direction::In,
            );
            debug!(
                "Invalid handshake message received ({})",
                self.remote_endpoint
            );
            return HandshakeStatus::Abort;
        }
        if message.query.is_some() && self.handshake_received.load(Ordering::SeqCst) {
            // Second handshake message should be a response only
            self.stats.inc_dir(
                StatType::TcpServer,
                DetailType::HandshakeError,
                Direction::In,
            );
            debug!(
                "Detected multiple handshake queries ({})",
                self.remote_endpoint
            );
            return HandshakeStatus::Abort;
        }

        self.handshake_received.store(true, Ordering::SeqCst);

        self.stats.inc_dir(
            StatType::TcpServer,
            DetailType::NodeIdHandshake,
            Direction::In,
        );

        let log_type = match (message.query.is_some(), message.response.is_some()) {
            (true, true) => "query + response",
            (true, false) => "query",
            (false, true) => "response",
            (false, false) => "none",
        };
        debug!(
            "Handshake message received: {} ({})",
            log_type, self.remote_endpoint
        );

        if let Some(query) = message.query.clone() {
            // Send response + our own query
            if self
                .send_response(&query, message.is_v2, socket)
                .await
                .is_err()
            {
                // Stop invalid handshake
                return HandshakeStatus::Abort;
            }
            // Fall through and continue handshake
        }
        if let Some(response) = &message.response {
            if self.verify_response(response, &self.remote_endpoint) {
                return HandshakeStatus::Realtime(response.node_id); // Switch to realtime
            } else {
                self.stats.inc_dir(
                    StatType::TcpServer,
                    DetailType::HandshakeResponseInvalid,
                    Direction::In,
                );
                debug!(
                    "Invalid handshake response received ({})",
                    self.remote_endpoint
                );
                return HandshakeStatus::Abort;
            }
        }
        HandshakeStatus::Handshake // Handshake is in progress
    }

    pub(crate) async fn send_response(
        &self,
        query: &NodeIdHandshakeQuery,
        v2: bool,
        socket: &Socket,
    ) -> anyhow::Result<()> {
        let response = self.prepare_response(query, v2);
        let own_query = self.prepare_query(&self.remote_endpoint);

        let handshake_response = Message::NodeIdHandshake(NodeIdHandshake {
            is_v2: own_query.is_some() || response.v2.is_some(),
            query: own_query,
            response: Some(response),
        });

        debug!("Responding to handshake ({})", self.remote_endpoint);

        let mut serializer = MessageSerializer::new(self.protocol);
        let buffer = serializer.serialize(&handshake_response);
        match socket.write_raw(buffer).await {
            Ok(_) => {
                self.stats
                    .inc_dir(StatType::TcpServer, DetailType::Handshake, Direction::Out);
                self.stats.inc_dir(
                    StatType::TcpServer,
                    DetailType::HandshakeResponse,
                    Direction::Out,
                );
                Ok(())
            }
            Err(e) => {
                self.stats.inc_dir(
                    StatType::TcpServer,
                    DetailType::HandshakeNetworkError,
                    Direction::In,
                );
                debug!(
                    "Error sending handshake response: {} ({:?})",
                    self.remote_endpoint, e
                );
                Err(e)
            }
        }
    }

    pub(crate) fn verify_response(
        &self,
        response: &NodeIdHandshakeResponse,
        remote_endpoint: &SocketAddrV6,
    ) -> bool {
        // Prevent connection with ourselves
        if response.node_id == self.node_id.public_key() {
            self.stats.inc_dir(
                StatType::Handshake,
                DetailType::InvalidNodeId,
                Direction::In,
            );
            return false; // Fail
        }

        // Prevent mismatched genesis
        if let Some(v2) = &response.v2 {
            if v2.genesis != self.genesis_hash {
                self.stats.inc_dir(
                    StatType::Handshake,
                    DetailType::InvalidGenesis,
                    Direction::In,
                );
                return false; // Fail
            }
        }

        let Some(cookie) = self.syn_cookies.cookie(remote_endpoint) else {
            self.stats.inc_dir(
                StatType::Handshake,
                DetailType::MissingCookie,
                Direction::In,
            );
            return false; // Fail
        };

        if response.validate(&cookie).is_err() {
            self.stats.inc_dir(
                StatType::Handshake,
                DetailType::InvalidSignature,
                Direction::In,
            );
            return false; // Fail
        }

        self.stats
            .inc_dir(StatType::Handshake, DetailType::Ok, Direction::In);
        true // OK
    }

    pub(crate) fn prepare_response(
        &self,
        query: &NodeIdHandshakeQuery,
        v2: bool,
    ) -> NodeIdHandshakeResponse {
        if v2 {
            NodeIdHandshakeResponse::new_v2(&query.cookie, &self.node_id, self.genesis_hash)
        } else {
            NodeIdHandshakeResponse::new_v1(&query.cookie, &self.node_id)
        }
    }

    pub(crate) fn prepare_query(
        &self,
        remote_endpoint: &SocketAddrV6,
    ) -> Option<NodeIdHandshakeQuery> {
        self.syn_cookies
            .assign(remote_endpoint)
            .map(|cookie| NodeIdHandshakeQuery { cookie })
    }
}
