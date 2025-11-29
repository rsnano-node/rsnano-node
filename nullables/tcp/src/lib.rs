mod tcp_socket;
mod tcp_stream;
mod tcp_stream_factory;

use std::net::TcpListener;
pub use tcp_socket::*;
pub use tcp_stream::TcpStream;
pub use tcp_stream_factory::TcpStreamFactory;

pub fn get_available_port() -> u16 {
    // Let the OS pick an ephemeral port on loopback to minimize collisions and
    // avoid hardcoded ranges. Try IPv6 first (covers dual-stack), fall back to
    // IPv4 if needed.
    pick_ephemeral_port(["::1", "127.0.0.1"]).expect("Could not find an available port")
}

fn pick_ephemeral_port<const N: usize>(hosts: [&str; N]) -> Option<u16> {
    hosts.iter().find_map(|host| {
        TcpListener::bind((*host, 0))
            .ok()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.port())
    })
}
