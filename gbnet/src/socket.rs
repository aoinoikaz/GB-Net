// socket.rs - Platform-agnostic UDP socket wrapper
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::io::{Error as IoError, ErrorKind};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum SocketError {
    Io(IoError),
    InvalidAddress,
    SocketClosed,
    WouldBlock,
}

impl From<IoError> for SocketError {
    fn from(err: IoError) -> Self {
        match err.kind() {
            ErrorKind::WouldBlock => SocketError::WouldBlock,
            _ => SocketError::Io(err),
        }
    }
}

pub struct UdpSocket {
    socket: StdUdpSocket,
    recv_buffer: Vec<u8>,
    stats: SocketStats,
}

#[derive(Debug, Default)]
pub struct SocketStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_receive_time: Option<Instant>,
    pub last_send_time: Option<Instant>,
}

impl UdpSocket {
    /// Creates a new UDP socket bound to the specified address
    pub fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let socket = StdUdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        
        Ok(Self {
            socket,
            recv_buffer: vec![0u8; 65536], // Max UDP packet size
            stats: SocketStats::default(),
        })
    }
    
    /// Connects the socket to a specific remote address
    pub fn connect(&self, addr: SocketAddr) -> Result<(), SocketError> {
        self.socket.connect(addr)?;
        Ok(())
    }
    
    /// Returns the local address this socket is bound to
    pub fn local_addr(&self) -> Result<SocketAddr, SocketError> {
        Ok(self.socket.local_addr()?)
    }
    
    /// Sends data to a specific address
    pub fn send_to(&mut self, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
        let sent = self.socket.send_to(data, addr)?;
        self.stats.bytes_sent += sent as u64;
        self.stats.packets_sent += 1;
        self.stats.last_send_time = Some(Instant::now());
        Ok(sent)
    }
    
    /// Receives data from any address (returns data slice and sender address)
    pub fn recv_from(&mut self) -> Result<(&[u8], SocketAddr), SocketError> {
        match self.socket.recv_from(&mut self.recv_buffer) {
            Ok((len, addr)) => {
                self.stats.bytes_received += len as u64;
                self.stats.packets_received += 1;
                self.stats.last_receive_time = Some(Instant::now());
                Ok((&self.recv_buffer[..len], addr))
            }
            Err(e) => Err(e.into()),
        }
    }
    
    /// Sends data to the connected address (socket must be connected first)
    pub fn send(&mut self, data: &[u8]) -> Result<usize, SocketError> {
        let sent = self.socket.send(data)?;
        self.stats.bytes_sent += sent as u64;
        self.stats.packets_sent += 1;
        self.stats.last_send_time = Some(Instant::now());
        Ok(sent)
    }
    
    /// Receives data from the connected address
    pub fn recv(&mut self) -> Result<&[u8], SocketError> {
        match self.socket.recv(&mut self.recv_buffer) {
            Ok(len) => {
                self.stats.bytes_received += len as u64;
                self.stats.packets_received += 1;
                self.stats.last_receive_time = Some(Instant::now());
                Ok(&self.recv_buffer[..len])
            }
            Err(e) => Err(e.into()),
        }
    }
    
    /// Sets the read timeout for the socket
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_read_timeout(dur)?;
        Ok(())
    }
    
    /// Sets the write timeout for the socket
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_write_timeout(dur)?;
        Ok(())
    }
    
    /// Returns socket statistics
    pub fn stats(&self) -> &SocketStats {
        &self.stats
    }
    
    /// Resets socket statistics
    pub fn reset_stats(&mut self) {
        self.stats = SocketStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_socket_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let socket = UdpSocket::bind(addr);
        assert!(socket.is_ok());
    }
    
    #[test]
    fn test_socket_stats() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let mut socket = UdpSocket::bind(addr).unwrap();
        
        let initial_stats = socket.stats();
        assert_eq!(initial_stats.packets_sent, 0);
        assert_eq!(initial_stats.packets_received, 0);
        
        socket.reset_stats();
        let reset_stats = socket.stats();
        assert_eq!(reset_stats.packets_sent, 0);
    }
}