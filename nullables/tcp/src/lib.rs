mod tcp_socket;
mod tcp_stream;
mod tcp_stream_factory;

use std::sync::atomic::{AtomicU16, Ordering};
pub use tcp_socket::*;
pub use tcp_stream::TcpStream;
pub use tcp_stream_factory::TcpStreamFactory;

static START_PORT: AtomicU16 = AtomicU16::new(1025);

pub fn get_available_port() -> u16 {
    // Reserve a range of 10 ports per test to avoid collisions when tests run in parallel
    let start = START_PORT.fetch_add(10, Ordering::SeqCst);
    (start..start + 10)
        .find(|port| is_port_available(*port))
        .expect("Could not find an available port")
}

fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}
