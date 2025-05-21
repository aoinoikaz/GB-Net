use super::{Serialize, Deserialize, bit_io::{BitWriter, BitReader}};
use std::time::Instant;
use log::trace;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serialize_all]
pub struct PacketHeader {
    pub sequence: u16,
    pub ack: u16,
    pub ack_bits: u16,
    pub channel_id: u8,
    pub connection_id: u16,
    pub timestamp: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[bits(4)]
pub enum PacketType {
    ConnectRequest,
    ConnectAccept,
    Disconnect,
    KeepAlive,
    Data { data: Vec<u8>, ordered: bool },
    Snapshot { data: Vec<u8>, timestamp: u32 },
    SnapshotDelta { delta: Vec<u8>, timestamp: u32 },
    Fragment { data: Vec<u8>, fragment_id: u8, total_fragments: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serialize_all]
pub struct Packet {
    pub header: PacketHeader,
    pub packet_type: PacketType,
}

impl Packet {
    pub fn new_connect_request(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::ConnectRequest,
        }
    }

    pub fn new_connect_accept(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::ConnectAccept,
        }
    }

    pub fn new_keep_alive(sequence: u16, channel_id: u8, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::KeepAlive,
        }
    }

    pub fn new_data(sequence: u16, channel_id: u8, data: Vec<u8>, ordered: bool, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::Data { data, ordered },
        }
    }

    pub fn new_snapshot(sequence: u16, channel_id: u8, data: Vec<u8>, timestamp: u32, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::Snapshot { data, timestamp },
        }
    }

    pub fn with_connection_id(self, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                connection_id: connection_id as u16,
                ..self.header
            },
            ..self
        }
    }
}