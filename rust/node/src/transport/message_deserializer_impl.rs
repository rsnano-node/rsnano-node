use super::NetworkFilter;
use crate::{config::NetworkConstants, messages::*, utils::BlockUniquer, voting::VoteUniquer};
use rsnano_core::utils::{Stream, StreamAdapter};
use std::sync::Arc;

pub const MAX_MESSAGE_SIZE: usize = 1024 * 65;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseStatus {
    None,
    Success,
    InsufficientWork,
    InvalidHeader,
    InvalidMessageType,
    InvalidKeepaliveMessage,
    InvalidPublishMessage,
    InvalidConfirmReqMessage,
    InvalidConfirmAckMessage,
    InvalidNodeIdHandshakeMessage,
    InvalidTelemetryReqMessage,
    InvalidTelemetryAckMessage,
    InvalidBulkPullMessage,
    InvalidBulkPullAccountMessage,
    InvalidFrontierReqMessage,
    InvalidAscPullReqMessage,
    InvalidAscPullAckMessage,
    InvalidNetwork,
    OutdatedVersion,
    DuplicatePublishMessage,
    MessageSizeTooBig,
}

impl ParseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Success => "success",
            Self::InsufficientWork => "insufficient_work",
            Self::InvalidHeader => "invalid_header",
            Self::InvalidMessageType => "invalid_message_type",
            Self::InvalidKeepaliveMessage => "invalid_keepalive_message",
            Self::InvalidPublishMessage => "invalid_publish_message",
            Self::InvalidConfirmReqMessage => "invalid_confirm_req_message",
            Self::InvalidConfirmAckMessage => "invalid_confirm_ack_message",
            Self::InvalidNodeIdHandshakeMessage => "invalid_node_id_handshake_message",
            Self::InvalidTelemetryReqMessage => "invalid_telemetry_req_message",
            Self::InvalidTelemetryAckMessage => "invalid_telemetry_ack_message",
            Self::InvalidBulkPullMessage => "invalid_bulk_pull_message",
            Self::InvalidBulkPullAccountMessage => "invalid_bulk_pull_account_message",
            Self::InvalidFrontierReqMessage => "invalid_frontier_req_message",
            Self::InvalidAscPullReqMessage => "invalid_asc_pull_req_message",
            Self::InvalidAscPullAckMessage => "invalid_asc_pull_ack_message",
            Self::InvalidNetwork => "invalid_network",
            Self::OutdatedVersion => "outdated_version",
            Self::DuplicatePublishMessage => "duplicate_publish_message",
            Self::MessageSizeTooBig => "message_size_too_big",
        }
    }
}

pub fn validate_header(
    header: &MessageHeader,
    expected_protocol: &ProtocolInfo,
) -> Result<(), ParseStatus> {
    if header.network != expected_protocol.network {
        Err(ParseStatus::InvalidNetwork)
    } else if header.version_using < expected_protocol.version_min {
        Err(ParseStatus::OutdatedVersion)
    } else if !header.is_valid_message_type() {
        Err(ParseStatus::InvalidHeader)
    } else if header.payload_length() > MAX_MESSAGE_SIZE {
        Err(ParseStatus::MessageSizeTooBig)
    } else {
        Ok(())
    }
}

fn at_end(stream: &mut impl Stream) -> bool {
    stream.read_u8().is_err()
}

pub struct MessageDeserializerImpl {
    network_constants: NetworkConstants,
    publish_filter: Arc<NetworkFilter>,
    block_uniquer: Arc<BlockUniquer>,
    vote_uniquer: Arc<VoteUniquer>,
}

impl MessageDeserializerImpl {
    pub fn new(
        network_constants: NetworkConstants,
        publish_filter: Arc<NetworkFilter>,
        block_uniquer: Arc<BlockUniquer>,
        vote_uniquer: Arc<VoteUniquer>,
    ) -> Self {
        Self {
            network_constants,
            publish_filter,
            block_uniquer,
            vote_uniquer,
        }
    }

    pub fn deserialize(
        &self,
        header: MessageHeader,
        payload_bytes: &[u8],
    ) -> Result<MessageEnum, ParseStatus> {
        let mut stream = StreamAdapter::new(payload_bytes);
        match header.message_type {
            MessageType::Keepalive => self.deserialize_keepalive(&mut stream, header),
            MessageType::Publish => {
                // Early filtering to not waste time deserializing duplicate blocks
                let (digest, existed) = self.publish_filter.apply(payload_bytes);
                if !existed {
                    Ok(self.deserialize_publish(&mut stream, header, digest)?)
                } else {
                    Err(ParseStatus::DuplicatePublishMessage)
                }
            }
            MessageType::ConfirmReq => self.deserialize_confirm_req(&mut stream, header),
            MessageType::ConfirmAck => self.deserialize_confirm_ack(&mut stream, header),
            MessageType::NodeIdHandshake => self.deserialize_node_id_handshake(&mut stream, header),
            MessageType::TelemetryReq => self.deserialize_telemetry_req(&mut stream, header),
            MessageType::TelemetryAck => self.deserialize_telemetry_ack(&mut stream, header),
            MessageType::BulkPull => self.deserialize_bulk_pull(&mut stream, header),
            MessageType::BulkPullAccount => self.deserialize_bulk_pull_account(&mut stream, header),
            MessageType::BulkPush => self.deserialize_bulk_push(&mut stream, header),
            MessageType::FrontierReq => self.deserialize_frontier_req(&mut stream, header),
            MessageType::AscPullReq => self.deserialize_asc_pull_req(&mut stream, header),
            MessageType::AscPullAck => self.deserialize_asc_pull_ack(&mut stream, header),
            MessageType::Invalid | MessageType::NotAType => Err(ParseStatus::InvalidMessageType),
        }
    }

    fn deserialize_keepalive(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(
            stream,
            header,
            0,
            Some(&self.block_uniquer),
            Some(&self.vote_uniquer),
        ) {
            if at_end(stream) {
                return Ok(msg);
            }
        }
        Err(ParseStatus::InvalidKeepaliveMessage)
    }

    fn deserialize_publish(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
        digest: u128,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(
            stream,
            header,
            digest,
            Some(&self.block_uniquer),
            Some(&self.vote_uniquer),
        ) {
            if at_end(stream) {
                let Payload::Publish(payload) = &msg.payload else { unreachable!()};
                if !self
                    .network_constants
                    .work
                    .validate_entry_block(&payload.block)
                {
                    return Ok(msg);
                } else {
                    return Err(ParseStatus::InsufficientWork);
                }
            }
        }

        Err(ParseStatus::InvalidPublishMessage)
    }

    fn deserialize_confirm_req(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) =
            MessageEnum::deserialize(stream, header, 0, Some(&self.block_uniquer), None)
        {
            if at_end(stream) {
                let Payload::ConfirmReq(payload) = &msg.payload else {unreachable!()};
                let work_ok = match &payload.block {
                    Some(block) => !self.network_constants.work.validate_entry_block(&block),
                    None => true,
                };
                if work_ok {
                    return Ok(msg);
                } else {
                    return Err(ParseStatus::InsufficientWork);
                }
            }
        }
        Err(ParseStatus::InvalidConfirmReqMessage)
    }

    fn deserialize_confirm_ack(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, Some(&self.vote_uniquer))
        {
            if at_end(stream) {
                return Ok(msg);
            }
        }
        Err(ParseStatus::InvalidConfirmAckMessage)
    }

    fn deserialize_node_id_handshake(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, None) {
            if at_end(stream) {
                return Ok(msg);
            }
        }

        Err(ParseStatus::InvalidNodeIdHandshakeMessage)
    }

    fn deserialize_telemetry_req(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        // Message does not use stream payload (header only)
        MessageEnum::deserialize(stream, header, 0, None, None)
            .map_err(|_| ParseStatus::InvalidTelemetryReqMessage)
    }

    fn deserialize_telemetry_ack(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, None) {
            // Intentionally not checking if at the end of stream, because these messages support backwards/forwards compatibility
            return Ok(msg);
        }
        Err(ParseStatus::InvalidTelemetryAckMessage)
    }

    fn deserialize_bulk_pull(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, None) {
            if at_end(stream) {
                return Ok(msg);
            }
        }
        Err(ParseStatus::InvalidBulkPullMessage)
    }

    fn deserialize_bulk_pull_account(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, None) {
            if at_end(stream) {
                return Ok(msg);
            }
        }
        Err(ParseStatus::InvalidBulkPullAccountMessage)
    }

    fn deserialize_frontier_req(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        if let Ok(msg) = MessageEnum::deserialize(stream, header, 0, None, None) {
            if at_end(stream) {
                return Ok(msg);
            }
        }
        Err(ParseStatus::InvalidFrontierReqMessage)
    }

    fn deserialize_bulk_push(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        // Message does not use stream payload (header only)
        match MessageEnum::deserialize(stream, header, 0, None, None) {
            Ok(msg) => Ok(msg),
            Err(_) => Err(ParseStatus::InvalidMessageType), // TODO correct error type
        }
    }

    fn deserialize_asc_pull_req(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        // Intentionally not checking if at the end of stream, because these messages support backwards/forwards compatibility
        match MessageEnum::deserialize(stream, header, 0, None, None) {
            Ok(msg) => Ok(msg),
            Err(_) => Err(ParseStatus::InvalidAscPullReqMessage),
        }
    }

    fn deserialize_asc_pull_ack(
        &self,
        stream: &mut impl Stream,
        header: MessageHeader,
    ) -> Result<MessageEnum, ParseStatus> {
        // Intentionally not checking if at the end of stream, because these messages support backwards/forwards compatibility
        match MessageEnum::deserialize(stream, header, 0, None, None) {
            Ok(msg) => Ok(msg),
            Err(_) => Err(ParseStatus::InvalidAscPullAckMessage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::STUB_NETWORK_CONSTANTS, voting::Vote};
    use rsnano_core::BlockBuilder;

    #[test]
    fn exact_confirm_ack() {
        test_deserializer(&MessageEnum::new_confirm_ack(
            &Default::default(),
            Arc::new(Vote::create_test_instance()),
        ));
    }

    #[test]
    fn exact_confirm_req() {
        let block = Arc::new(BlockBuilder::legacy_send().build());
        let message = MessageEnum::new_confirm_req_with_block(&Default::default(), block);
        test_deserializer(&message);
    }

    #[test]
    fn exact_publish() {
        let block = Arc::new(BlockBuilder::legacy_send().build());
        let message = MessageEnum::new_publish(&ProtocolInfo::dev_network(), block);
        test_deserializer(&message);
    }

    #[test]
    fn exact_keepalive() {
        test_deserializer(&MessageEnum::new_keepalive(&ProtocolInfo::dev_network()));
    }

    #[test]
    fn exact_frontier_req() {
        test_deserializer(&MessageEnum::new_frontier_req(
            &Default::default(),
            FrontierReqPayload::create_test_instance(),
        ));
    }

    #[test]
    fn exact_telemetry_req() {
        test_deserializer(&MessageEnum::new_telemetry_req(&Default::default()));
    }

    #[test]
    fn exact_telemetry_ack() {
        let mut data = TelemetryData::default();
        data.unknown_data.push(0xFF);

        test_deserializer(&MessageEnum::new_telemetry_ack(&Default::default(), data));
    }

    #[test]
    fn exact_bulk_pull() {
        test_deserializer(&MessageEnum::new_bulk_pull(
            &ProtocolInfo::dev_network(),
            BulkPullPayload::create_test_instance(),
        ));
    }

    #[test]
    fn exact_bulk_pull_account() {
        test_deserializer(&MessageEnum::new_bulk_pull_account(
            &ProtocolInfo::dev_network(),
            BulkPullAccountPayload::create_test_instance(),
        ));
    }

    #[test]
    fn exact_bulk_push() {
        test_deserializer(&MessageEnum::new_bulk_push(&ProtocolInfo::dev_network()));
    }

    #[test]
    fn exact_node_id_handshake() {
        test_deserializer(&MessageEnum::new_node_id_handshake(
            &ProtocolInfo::dev_network(),
            Some(NodeIdHandshakeQuery { cookie: [1; 32] }),
            None,
        ));
    }

    #[test]
    fn exact_asc_pull_req() {
        let message = MessageEnum::new_asc_pull_req_accounts(
            &ProtocolInfo::dev_network(),
            7,
            AccountInfoReqPayload::create_test_instance(),
        );
        test_deserializer(&message);
    }

    #[test]
    fn exact_asc_pull_ack() {
        let message = MessageEnum::new_asc_pull_ack_accounts(
            &ProtocolInfo::dev_network(),
            7,
            AccountInfoAckPayload::create_test_instance(),
        );
        test_deserializer(&message);
    }

    fn test_deserializer(original_message: &MessageEnum) {
        let network_filter = Arc::new(NetworkFilter::new(1));
        let block_uniquer = Arc::new(BlockUniquer::new());
        let vote_uniquer = Arc::new(VoteUniquer::new());

        let deserializer = Arc::new(MessageDeserializerImpl::new(
            STUB_NETWORK_CONSTANTS.clone(),
            network_filter,
            block_uniquer,
            vote_uniquer,
        ));

        let original_bytes = original_message.to_bytes();
        let mut stream = StreamAdapter::new(&original_bytes);
        let deserialized_header = MessageHeader::deserialize(&mut stream).unwrap();

        let deserialized_msg = deserializer
            .deserialize(deserialized_header, stream.remaining())
            .unwrap();

        assert_eq!(deserialized_msg.to_bytes(), original_bytes);
    }
}