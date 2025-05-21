use crate::serialize::{Serialize, Deserialize, BitSerialize, BitDeserialize};
use std::io;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
#[serialize_all]
pub struct PacketHeader {
    #[bits(16)]
    pub sequence: u16,
    #[bits(16)]
    pub ack: u16,
    #[bits(16)]
    pub ack_bits: u16,
    #[bits(16)]
    pub connection_id: u16,
    pub timestamp: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
#[bits(4)] // 7 variants, needs 4 bits
pub enum PacketType {
    ConnectRequest,
    ConnectAccept,
    Disconnect,
    KeepAlive,
    Data { data: Vec<u8> },
    Snapshot { data: Vec<u8> },
    SnapshotDelta { data: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, BitSerialize, BitDeserialize)]
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
                connection_id: connection_id as u16,
                timestamp: Some(Instant::now()),
            },
            packet_type: PacketType::ConnectRequest,
        }
    }
}