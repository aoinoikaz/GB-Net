// packet.rs - Core packet structures for reliable UDP
use std::io;
use gbnet_macros::NetworkSerialize;
use crate::serialize::{BitSerialize, BitDeserialize, bit_io::{BitBuffer, BitWrite, BitRead}};

#[derive(Debug, Clone, PartialEq, NetworkSerialize)]
pub struct PacketHeader {
    #[bits = 32]
    pub protocol_id: u32,
    #[bits = 16]
    pub sequence: u16,
    #[bits = 16]
    pub ack: u16,
    #[bits = 32]
    pub ack_bits: u32,
}

#[derive(Debug, Clone, PartialEq, NetworkSerialize)]
#[bits = 4] // 16 packet types max
pub enum PacketType {
    ConnectionRequest,
    ConnectionChallenge { 
        #[bits = 64]
        server_salt: u64 
    },
    ConnectionResponse { 
        #[bits = 64]
        client_salt: u64 
    },
    ConnectionAccept,
    ConnectionDeny { 
        #[bits = 8]
        reason: u8 
    },
    Disconnect { 
        #[bits = 8]
        reason: u8 
    },
    KeepAlive,
    Payload { 
        #[bits = 3]
        channel: u8,
        #[bits = 1]
        is_fragment: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub header: PacketHeader,
    pub packet_type: PacketType,
    pub payload: Vec<u8>,
}

impl Packet {
    /// Creates a new packet with the given header and type.
    pub fn new(header: PacketHeader, packet_type: PacketType) -> Self {
        Self {
            header,
            packet_type,
            payload: Vec::new(),
        }
    }
    
    /// Adds a payload to the packet.
    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = payload;
        self
    }
    
    /// Serializes the packet into a byte vector.
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut buffer = BitBuffer::new();
        
        // Serialize header
        self.header.bit_serialize(&mut buffer)?;
        
        // Serialize packet type
        self.packet_type.bit_serialize(&mut buffer)?;
        
        // Pad to byte boundary before payload using BitWrite trait
        while BitWrite::bit_pos(&buffer) % 8 != 0 {
            buffer.write_bit(false)?;
        }
        
        // Get the header bytes
        let header_bytes = buffer.into_bytes(true)?;
        
        // Combine header and payload
        let mut result = header_bytes;
        result.extend_from_slice(&self.payload);
        
        Ok(result)
    }
    
    /// Deserializes a packet from a byte slice.
    pub fn deserialize(data: &[u8]) -> io::Result<Self> {
        if data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty packet"));
        }
        
        let mut buffer = BitBuffer::from_bytes(data.to_vec());
        
        // Deserialize header
        let header = PacketHeader::bit_deserialize(&mut buffer)?;
        
        // Deserialize packet type
        let packet_type = PacketType::bit_deserialize(&mut buffer)?;
        
        // Align to byte boundary using BitRead trait
        while BitRead::bit_pos(&buffer) % 8 != 0 {
            buffer.read_bit()?;
        }
        
        // Calculate where payload starts
        let header_size = BitRead::bit_pos(&buffer) / 8;
        let payload = if header_size < data.len() {
            data[header_size..].to_vec()
        } else {
            Vec::new()
        };
        
        Ok(Self {
            header,
            packet_type,
            payload,
        })
    }
}

// Disconnect reasons
pub mod disconnect_reason {
    pub const TIMEOUT: u8 = 0;
    pub const REQUESTED: u8 = 1;
    pub const KICKED: u8 = 2;
    pub const SERVER_FULL: u8 = 3;
    pub const PROTOCOL_MISMATCH: u8 = 4;
}

// Connection deny reasons
pub mod deny_reason {
    pub const SERVER_FULL: u8 = 0;
    pub const ALREADY_CONNECTED: u8 = 1;
    pub const INVALID_PROTOCOL: u8 = 2;
    pub const BANNED: u8 = 3;
    pub const INVALID_CHALLENGE: u8 = 4;
}

/// Utility function to compare sequence numbers, accounting for wraparound.
pub fn sequence_greater_than(s1: u16, s2: u16) -> bool {
    ((s1 > s2) && (s1 - s2 <= 32768)) || ((s1 < s2) && (s2 - s1 > 32768))
}

/// Utility function to compute the difference between sequence numbers.
pub fn sequence_diff(s1: u16, s2: u16) -> i32 {
    let diff = s1 as i32 - s2 as i32;
    if diff > 32768 {
        diff - 65536
    } else if diff < -32768 {
        diff + 65536
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_serialization() {
        let header = PacketHeader {
            protocol_id: 0x12345678,
            sequence: 100,
            ack: 99,
            ack_bits: 0xFFFFFFFF,
        };
        
        let packet = Packet::new(header.clone(), PacketType::KeepAlive)
            .with_payload(vec![1, 2, 3, 4]);
        
        let serialized = packet.serialize().unwrap();
        let deserialized = Packet::deserialize(&serialized).unwrap();
        
        assert_eq!(packet.header, deserialized.header);
        assert_eq!(packet.packet_type, deserialized.packet_type);
        assert_eq!(packet.payload, deserialized.payload);
    }
    
    #[test]
    fn test_sequence_comparison() {
        assert!(sequence_greater_than(1, 0));
        assert!(sequence_greater_than(0, 65535));
        assert!(!sequence_greater_than(0, 1));
        
        assert_eq!(sequence_diff(1, 0), 1);
        assert_eq!(sequence_diff(0, 1), -1);
        assert_eq!(sequence_diff(0, 65535), 1);
        assert_eq!(sequence_diff(65535, 0), -1);
    }
}