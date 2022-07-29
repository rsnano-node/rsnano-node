use crate::network::PeerExclusion;
use std::{net::SocketAddr, sync::Arc};

use super::EndpointDto;

pub struct PeerExclusionHandle(Arc<PeerExclusion>);

#[no_mangle]
pub extern "C" fn rsn_peer_exclusion_create() -> *mut PeerExclusionHandle {
    Box::into_raw(Box::new(PeerExclusionHandle(
        Arc::new(PeerExclusion::new()),
    )))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_destroy(handle: *mut PeerExclusionHandle) {
    drop(Box::from_raw(handle))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_add(
    handle: *mut PeerExclusionHandle,
    endpoint: *const EndpointDto,
    network_peers_count: usize,
) -> u64 {
    (*handle)
        .0
        .peer_misbehaved(&SocketAddr::from(&*endpoint), network_peers_count)
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_check(
    handle: *mut PeerExclusionHandle,
    endpoint: *const EndpointDto,
) -> bool {
    (*handle).0.is_excluded(&SocketAddr::from(&*endpoint))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_contains(
    handle: *mut PeerExclusionHandle,
    endpoint: *const EndpointDto,
) -> bool {
    (*handle).0.contains(&SocketAddr::from(&*endpoint))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_remove(
    handle: *mut PeerExclusionHandle,
    endpoint: *const EndpointDto,
) {
    (*handle).0.remove(&SocketAddr::from(&*endpoint))
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_size(handle: *mut PeerExclusionHandle) -> usize {
    (*handle).0.size()
}

#[no_mangle]
pub unsafe extern "C" fn rsn_peer_exclusion_element_size() -> usize {
    PeerExclusion::element_size()
}
