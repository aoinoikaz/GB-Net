use std::net::SocketAddr;
use std::time::{Duration, Instant};
use log::{info, warn};

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(1);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Connection {
    pub addr: SocketAddr,
    pub state: ConnectionState,
    pub last_sent: Instant,
    pub last_received: Instant,
    pub sequence: u16,
    pub remote_sequence: u16,
    pub remote_ack: u16,
    pub remote_ack_bits: u32,
}

impl Connection {
    pub fn new(addr: SocketAddr) -> Self {
        let now = Instant::now();
        Connection {
            addr,
            state: ConnectionState::Connecting,
            last_sent: now,
            last_received: now,
            sequence: 0,
            remote_sequence: 0,
            remote_ack: 0,
            remote_ack_bits: 0,
        }
    }

    pub fn is_timed_out(&self, now: Instant) -> bool {
        if self.state == ConnectionState::Disconnected {
            return true;
        }
        let elapsed = now.duration_since(self.last_received);
        if elapsed > CONNECTION_TIMEOUT {
            warn!("Connection to {} timed out after {:?}", self.addr, elapsed);
            return true;
        }
        false
    }

    pub fn should_send_keep_alive(&self, now: Instant) -> bool {
        if self.state != ConnectionState::Connected {
            return false;
        }
        let elapsed = now.duration_since(self.last_sent);
        elapsed >= KEEP_ALIVE_INTERVAL
    }

    pub fn on_send(&mut self, sequence: u16, now: Instant) {
        self.sequence = sequence;
        self.last_sent = now;
        if self.state == ConnectionState::Connecting {
            self.state = ConnectionState::Connected;
            info!("Connection established to {}", self.addr);
        }
    }

    pub fn on_receive(&mut self, sequence: u16, ack: u16, ack_bits: u32, now: Instant) {
        self.remote_sequence = sequence;
        self.remote_ack = ack;
        self.remote_ack_bits = ack_bits;
        self.last_received = now;
        if self.state == ConnectionState::Connecting {
            self.state = ConnectionState::Connected;
            info!("Connection established to {}", self.addr);
        }
    }

    pub fn disconnect(&mut self) {
        self.state = ConnectionState::Disconnected;
        info!("Connection to {} disconnected", self.addr);
    }
}