use std::net::SocketAddr;

use rsnano_node::messages::{KeepalivePayload, MessageEnum, Payload, ProtocolInfo};

use super::{
    create_message_handle2, create_message_handle3, downcast_message, downcast_message_mut,
    message_handle_clone, MessageHandle, MessageHeaderHandle,
};
use crate::{transport::EndpointDto, NetworkConstantsDto, StringDto};

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_create(
    constants: *mut NetworkConstantsDto,
    version_using: i16,
) -> *mut MessageHandle {
    create_message_handle3(constants, |protocol_info| {
        if version_using < 0 {
            MessageEnum::new_keepalive(protocol_info)
        } else {
            let protocol_info = ProtocolInfo {
                version_using: version_using as u8,
                ..*protocol_info
            };
            MessageEnum::new_keepalive(&protocol_info)
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_create2(
    header: *mut MessageHeaderHandle,
) -> *mut MessageHandle {
    create_message_handle2(header, |header| MessageEnum {
        header,
        payload: Payload::Keepalive(Default::default()),
    })
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_clone(
    handle: *mut MessageHandle,
) -> *mut MessageHandle {
    message_handle_clone::<MessageEnum>(handle)
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_peers(
    handle: *mut MessageHandle,
    result: *mut EndpointDto,
) {
    let dtos = std::slice::from_raw_parts_mut(result, 8);
    let message = downcast_message::<MessageEnum>(handle);
    let Payload::Keepalive(payload) = &message.payload else {panic!("not a keepalive payload")};
    let peers: Vec<_> = payload.peers.iter().map(EndpointDto::from).collect();
    dtos.clone_from_slice(&peers);
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_set_peers(
    handle: *mut MessageHandle,
    result: *const EndpointDto,
) {
    let dtos = std::slice::from_raw_parts(result, 8);
    let peers: [SocketAddr; 8] = dtos
        .iter()
        .map(SocketAddr::from)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    downcast_message_mut::<MessageEnum>(handle).payload =
        Payload::Keepalive(KeepalivePayload { peers });
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_size() -> usize {
    KeepalivePayload::serialized_size()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_message_keepalive_to_string(
    handle: *mut MessageHandle,
    result: *mut StringDto,
) {
    let s = downcast_message_mut::<MessageEnum>(handle).to_string();
    *result = s.into()
}
