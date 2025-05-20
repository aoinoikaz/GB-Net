use log::trace;
use std::io;
use super::serialize::{BitReader, BitWriter, Serialize};

// Packet header containing networking metadata
#[derive(Debug, Clone, PartialEq)]
pub struct PacketHeader {
    pub sequence: u16,         // Packet sequence number
    pub ack: u16,             // Acknowledged sequence number
    pub ack_bits: u32,        // Bitfield of previous acks
    pub channel_id: u8,       // Channel identifier
    pub fragment_id: Option<u16>, // Fragment index for large packets
    pub total_fragments: Option<u16>, // Total fragments in a packet
    pub timestamp: Option<u32>, // Timestamp for snapshots
    pub priority: u8,         // Priority for packet ordering
    pub connection_id: u32,   // Unique connection identifier
}

// Enum defining all packet types for game networking
#[derive(Debug, Clone, PartialEq)]
pub enum PacketType<T: Serialize> {
    Data { data: T, ordered: bool }, // General game data (ordered or unordered)
    KeepAlive,                      // Maintains connection
    Fragment(Vec<u8>),              // Fragment of a large packet
    Snapshot { data: T, timestamp: u32 }, // Game state snapshot
    SnapshotDelta { delta: Vec<u8>, timestamp: u32 }, // Compressed snapshot delta
    Input(T),                       // Player input for lockstep
    ConnectRequest,                 // Initiates connection
    ConnectAccept,                  // Confirms connection
    Disconnect,                     // Terminates connection
}

// Main packet structure combining header and type
#[derive(Debug, Clone, PartialEq)]
pub struct Packet<T: Serialize> {
    pub header: PacketHeader,
    pub packet_type: PacketType<T>,
}

impl<T: Serialize> Packet<T> {
    // Creates a new data packet
    pub fn new_data(sequence: u16, channel_id: u8, data: T, ordered: bool, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::Data { data, ordered },
        }
    }

    // Creates a new keep-alive packet
    pub fn new_keep_alive(sequence: u16, channel_id: u8, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::KeepAlive,
        }
    }

    // Creates a new snapshot packet
    pub fn new_snapshot(sequence: u16, channel_id: u8, data: T, timestamp: u32, priority: u8, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                fragment_id: None,
                total_fragments: None,
                timestamp: Some(timestamp),
                priority,
                connection_id,
            },
            packet_type: PacketType::Snapshot { data, timestamp },
        }
    }

    // Creates a new input packet
    pub fn new_input(sequence: u16, channel_id: u8, data: T, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::Input(data),
        }
    }

    // Creates a new connect request packet
    pub fn new_connect_request(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::ConnectRequest,
        }
    }

    // Creates a new connect accept packet
    pub fn new_connect_accept(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::ConnectAccept,
        }
    }

    // Creates a new disconnect packet
    pub fn new_disconnect(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                fragment_id: None,
                total_fragments: None,
                timestamp: None,
                priority: 0,
                connection_id,
            },
            packet_type: PacketType::Disconnect,
        }
    }

    // Updates the connection ID
    pub fn with_connection_id(self, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader { connection_id, ..self.header },
            packet_type: self.packet_type,
        }
    }

    // Serializes the packet to a bit stream
    pub fn serialize(&self, writer: &mut BitWriter) -> io::Result<()> {
        writer.write_bits(self.header.sequence as u64, 16)?;
        writer.write_bits(self.header.ack as u64, 16)?;
        writer.write_bits(self.header.ack_bits as u64, 32)?;
        writer.write_bits(self.header.channel_id as u64, 8)?;
        writer.write_bits(self.header.fragment_id.unwrap_or(0) as u64, 16)?;
        writer.write_bits(self.header.total_fragments.unwrap_or(0) as u64, 16)?;
        writer.write_bits(self.header.timestamp.unwrap_or(0) as u64, 32)?;
        writer.write_bits(self.header.priority as u64, 8)?;
        writer.write_bits(self.header.connection_id as u64, 32)?;

        match &self.packet_type {
            PacketType::Data { data, ordered } => {
                writer.write_bits(1, 4)?;
                writer.write_bits(*ordered as u64, 1)?;
                data.serialize(writer)?;
                trace!("Serialized data packet: sequence {}, channel {}", self.header.sequence, self.header.channel_id);
            }
            PacketType::KeepAlive => {
                writer.write_bits(0, 4)?;
                trace!("Serialized keep-alive packet: sequence {}, channel {}", self.header.sequence, self.header.channel_id);
            }
            PacketType::Fragment(data) => {
                writer.write_bits(2, 4)?;
                writer.write_bits(data.len() as u64, 16)?;
                for &byte in data {
                    writer.write_bits(byte as u64, 8)?;
                }
                trace!("Serialized fragment packet: sequence {}, fragment_id {:?}", self.header.sequence, self.header.fragment_id);
            }
            PacketType::Snapshot { data, timestamp } => {
                writer.write_bits(3, 4)?;
                data.serialize(writer)?;
                writer.write_bits(*timestamp as u64, 32)?;
                trace!("Serialized snapshot packet: sequence {}, timestamp {}", self.header.sequence, timestamp);
            }
            PacketType::SnapshotDelta { delta, timestamp } => {
                writer.write_bits(4, 4)?;
                writer.write_bits(delta.len() as u64, 16)?;
                for &byte in delta {
                    writer.write_bits(byte as u64, 8)?;
                }
                writer.write_bits(*timestamp as u64, 32)?;
                trace!("Serialized snapshot delta packet: sequence {}, timestamp {}", self.header.sequence, timestamp);
            }
            PacketType::Input(data) => {
                writer.write_bits(5, 4)?;
                data.serialize(writer)?;
                trace!("Serialized input packet: sequence {}", self.header.sequence);
            }
            PacketType::ConnectRequest => {
                writer.write_bits(6, 4)?;
                trace!("Serialized connect request: sequence {}", self.header.sequence);
            }
            PacketType::ConnectAccept => {
                writer.write_bits(7, 4)?;
                trace!("Serialized connect accept: sequence {}", self.header.sequence);
            }
            PacketType::Disconnect => {
                writer.write_bits(8, 4)?;
                trace!("Serialized disconnect: sequence {}", self.header.sequence);
            }
        }
        writer.flush()?;
        Ok(())
    }

    // Deserializes a packet from a bit stream
    pub fn deserialize(reader: &mut BitReader) -> io::Result<Self> {
        let sequence = reader.read_bits(16)? as u16;
        let ack = reader.read_bits(16)? as u16;
        let ack_bits = reader.read_bits(32)? as u32;
        let channel_id = reader.read_bits(8)? as u8;
        let fragment_id = reader.read_bits(16)? as u16;
        let total_fragments = reader.read_bits(16)? as u16;
        let timestamp = reader.read_bits(32)? as u32;
        let priority = reader.read_bits(8)? as u8;
        let connection_id = reader.read_bits(32)? as u32;
        let packet_type_flag = reader.read_bits(4)? as u8;

        let (fragment_id, total_fragments) = if fragment_id == 0 && total_fragments == 0 {
            (None, None)
        } else {
            (Some(fragment_id), Some(total_fragments))
        };
        let timestamp = if timestamp == 0 { None } else { Some(timestamp) };

        trace!("Deserialized header: sequence {}, ack {}, channel_id {}, connection_id {}", 
               sequence, ack, channel_id, connection_id);

        let packet_type = match packet_type_flag {
            0 => PacketType::KeepAlive,
            1 => {
                let ordered = reader.read_bits(1)? != 0;
                let data = T::deserialize(reader)?;
                PacketType::Data { data, ordered }
            }
            2 => {
                let len = reader.read_bits(16)? as usize;
                let mut data = Vec::with_capacity(len);
                for _ in 0..len {
                    data.push(reader.read_bits(8)? as u8);
                }
                PacketType::Fragment(data)
            }
            3 => {
                let data = T::deserialize(reader)?;
                let timestamp = reader.read_bits(32)? as u32;
                PacketType::Snapshot { data, timestamp }
            }
            4 => {
                let len = reader.read_bits(16)? as usize;
                let mut delta = Vec::with_capacity(len);
                for _ in 0..len {
                    delta.push(reader.read_bits(8)? as u8);
                }
                let timestamp = reader.read_bits(32)? as u32;
                PacketType::SnapshotDelta { delta, timestamp }
            }
            5 => {
                let data = T::deserialize(reader)?;
                PacketType::Input(data)
            }
            6 => PacketType::ConnectRequest,
            7 => PacketType::ConnectAccept,
            8 => PacketType::Disconnect,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid packet type")),
        };

        Ok(Packet {
            header: PacketHeader {
                sequence,
                ack,
                ack_bits,
                channel_id,
                fragment_id,
                total_fragments,
                timestamp,
                priority,
                connection_id,
            },
            packet_type,
        })
    }
}