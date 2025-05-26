// socket.rs - Platform-agnostic UDP socket wrapper
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::io::{Result as IoResult, Error as IoError, ErrorKind};
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
    send_buffer: Vec<u8>,
    stats: SocketStats,
}

#[derive(Debug, Default)]
struct SocketStats {
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    last_receive_time: Option<Instant>,
    last_send_time: Option<Instant>,
}

impl UdpSocket {
    pub fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let socket = StdUdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        
        Ok(Self {
            socket,
            recv_buffer: vec![0u8; 65536], // Max UDP packet size
            send_buffer: vec![0u8; 65536],
            stats: SocketStats::default(),
        })
    }
    
    pub fn connect(&self, addr: SocketAddr) -> Result<(), SocketError> {
        self.socket.connect(addr)?;
        Ok(())
    }
    
    pub fn local_addr(&self) -> Result<SocketAddr, SocketError> {
        Ok(self.socket.local_addr()?)
    }
    
    pub fn send_to(&mut self, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
        let sent = self.socket.send_to(data, addr)?;
        self.stats.bytes_sent += sent as u64;
        self.stats.packets_sent += 1;
        self.stats.last_send_time = Some(Instant::now());
        Ok(sent)
    }
    
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
    
    pub fn send(&mut self, data: &[u8]) -> Result<usize, SocketError> {
        let sent = self.socket.send(data)?;
        self.stats.bytes_sent += sent as u64;
        self.stats.packets_sent += 1;
        self.stats.last_send_time = Some(Instant::now());
        Ok(sent)
    }
    
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
    
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_read_timeout(dur)?;
        Ok(())
    }
    
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_write_timeout(dur)?;
        Ok(())
    }
    
    pub fn stats(&self) -> &SocketStats {
        &self.stats
    }
    
    pub fn reset_stats(&mut self) {
        self.stats = SocketStats::default();
    }
}