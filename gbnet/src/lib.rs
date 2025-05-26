// Core networking modules
pub mod socket;
pub mod packet;
pub mod connection;
pub mod reliability;
pub mod channel;
pub mod config;

// Serialization module (add this to your gbnet crate)
pub mod serialize;

// Re-export main types for convenience
pub use socket::{UdpSocket, SocketError};
pub use packet::{Packet, PacketHeader, PacketType};
pub use connection::{Connection, ConnectionState, ConnectionError};
pub use reliability::{ReliableEndpoint, SequenceBuffer};
pub use channel::{Channel, ChannelConfig, Reliability, Ordering};
pub use config::NetworkConfig;

// Re-export serialization traits
pub use serialize::{BitSerialize, BitDeserialize, ByteAlignedSerialize, ByteAlignedDeserialize};

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packet_loss: f32,
    pub rtt: f32,
    pub bandwidth_up: f32,
    pub bandwidth_down: f32,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            packet_loss: 0.0,
            rtt: 0.0,
            bandwidth_up: 0.0,
            bandwidth_down: 0.0,
        }
    }
}