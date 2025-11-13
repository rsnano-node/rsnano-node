use crate::TcpStream;
use std::{io, net::SocketAddr};

#[cfg(windows)]
use socket2::{Domain, Protocol, Socket, Type};

pub struct TcpSocket {
    strategy: Strategy,
}

impl TcpSocket {
    pub fn new_v6() -> io::Result<Self> {
        #[cfg(windows)]
        {
            // On Windows, we need explicit control over IPV6_V6ONLY
            // Windows defaults to IPV6_V6ONLY=true, preventing IPv4 connections
            let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;

            // Enable dual-stack (IPv4 and IPv6) support
            socket.set_only_v6(false)?;
            socket.set_reuse_address(true)?;

            // Convert socket2::Socket to std::net::TcpStream, then to tokio::net::TcpSocket
            socket.set_nonblocking(true)?;
            let std_stream: std::net::TcpStream = socket.into();
            let tokio_socket = tokio::net::TcpSocket::from_std_stream(std_stream);

            Ok(Self {
                strategy: Strategy::Real(tokio_socket),
            })
        }

        #[cfg(not(windows))]
        {
            // On Unix systems, use tokio's default which already handles dual-stack correctly
            let tokio_socket = tokio::net::TcpSocket::new_v6()?;
            tokio_socket.set_reuseaddr(true)?;
            tokio_socket.set_reuseport(true)?;

            Ok(Self {
                strategy: Strategy::Real(tokio_socket),
            })
        }
    }

    pub fn bind(&self, addr: SocketAddr) -> io::Result<()> {
        match &self.strategy {
            Strategy::Real(socket) => socket.bind(addr),
        }
    }

    pub fn listen(self, backlog: u32) -> io::Result<tokio::net::TcpListener> {
        match self.strategy {
            Strategy::Real(socket) => socket.listen(backlog),
        }
    }

    pub async fn connect(self, addr: SocketAddr) -> io::Result<TcpStream> {
        let stream = match self.strategy {
            Strategy::Real(socket) => socket.connect(addr).await?,
        };
        Ok(TcpStream::new(stream))
    }
}

enum Strategy {
    Real(tokio::net::TcpSocket),
}
