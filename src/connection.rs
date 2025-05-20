use std::net::SocketAddr;
use std::time::{Duration, Instant};
use log::{info, warn};
use rand::RngCore;

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(1);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const RTT_SMOOTHING_FACTOR: f32 = 0.1;

#[derive(Debug)]
pub struct Connection {
    pub addr: SocketAddr,
    pub state: ConnectionState,
    pub last_sent: Instant,
    pub last_received: Instant,
    pub sequence: u16,
    pub remote_sequence: u16,
    pub remote_ack: u16,
    pub remote_ack_bits: u16, // Changed to u16 to match PacketHeader
    pub connection_id: u32,
    pub rtt: f32,
}

impl Connection {
    pub fn new(addr: SocketAddr) -> Self {
        let now = Instant::now();
        let mut rng = rand::thread_rng();
        let connection_id = rng.next_u32();
        Connection {
            addr,
            state: ConnectionState::Connecting,
            last_sent: now,
            last_received: now,
            sequence: 0,
            remote_sequence: 0,
            remote_ack: 0,
            remote_ack_bits: 0,
            connection_id,
            rtt: 0.0,
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

    pub fn on_receive(&mut self, sequence: u16, ack: u16, ack_bits: u16, now: Instant) {
        self.remote_sequence = sequence;
        self.remote_ack = ack;
        self.remote_ack_bits = ack_bits;
        let packet_rtt = now.duration_since(self.last_sent).as_secs_f32() * 1000.0;
        self.rtt = if self.rtt == 0.0 {
            packet_rtt
        } else {
            self.rtt * (1.0 - RTT_SMOOTHING_FACTOR) + packet_rtt * RTT_SMOOTHING_FACTOR
        };
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