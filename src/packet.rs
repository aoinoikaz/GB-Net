use std::io;
use super::serialize::{BitReader, BitWriter, Serialize};
use log::trace;

// Immutable packet header
#[derive(Debug, Clone, PartialEq)]
pub struct PacketHeader {
    pub sequence: u16,       // 8 bits
    pub ack: u16,           // 8 bits
    pub ack_bits: u16,      // 8 bits
    pub channel_id: u8,     // 2 bits
    pub connection_id: u16, // 12 bits
}

// Stateless packet type
#[derive(Debug, Clone, PartialEq)]
pub enum PacketType<T: Serialize> {
    ConnectRequest,
    ConnectAccept,
    Disconnect,
    KeepAlive,
    Data { data: T, ordered: bool },
    Snapshot { data: T, timestamp: u32 },
    SnapshotDelta { delta: Vec<u8>, timestamp: u32 },
    Fragment { data: Vec<u8>, fragment_id: u8, total_fragments: u8 },
    Input(T),
}

// Functional packet
#[derive(Debug, Clone, PartialEq)]
pub struct Packet<T: Serialize> {
    pub header: PacketHeader,
    pub packet_type: PacketType<T>,
}

impl<T: Serialize + std::fmt::Debug + Clone> Packet<T> {
    pub fn new_connect_request(sequence: u16, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id: 0,
                connection_id: connection_id as u16,
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
            },
            packet_type: PacketType::KeepAlive,
        }
    }

    pub fn new_data(sequence: u16, channel_id: u8, data: T, ordered: bool, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
            },
            packet_type: PacketType::Data { data, ordered },
        }
    }

    pub fn new_snapshot(sequence: u16, channel_id: u8, data: T, timestamp: u32, _priority: u8, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
            },
            packet_type: PacketType::Snapshot { data, timestamp },
        }
    }

    pub fn new_input(sequence: u16, channel_id: u8, data: T, connection_id: u32) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
                connection_id: connection_id as u16,
            },
            packet_type: PacketType::Input(data),
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

impl<T: Serialize + std::fmt::Debug + Clone> Serialize for Packet<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        let writer = writer
            .write_bits(self.header.sequence as u64, 8)?
            .write_bits(self.header.ack as u64, 8)?
            .write_bits(self.header.ack_bits as u64, 8)?
            .write_bits(self.header.channel_id as u64, 2)?
            .write_bits(self.header.connection_id as u64, 12)?;

        match &self.packet_type {
            PacketType::ConnectRequest => writer.write_bits(0, 4),
            PacketType::ConnectAccept => writer.write_bits(1, 4),
            PacketType::Disconnect => writer.write_bits(2, 4),
            PacketType::KeepAlive => writer.write_bits(3, 4),
            PacketType::Data { data, ordered } => {
                let writer = writer.write_bits(4, 4)?.write_bit(*ordered)?;
                data.serialize(writer)
            }
            PacketType::Snapshot { data, timestamp } => {
                let writer = writer.write_bits(5, 4)?.write_bits(*timestamp as u64, 24)?;
                data.serialize(writer)
            }
            PacketType::SnapshotDelta { delta, timestamp } => {
                let mut writer = writer.write_bits(6, 4)?.write_bits(*timestamp as u64, 24)?;
                writer = writer.write_bits(delta.len() as u64, 10)?; // Up to 1024 bytes
                for &byte in delta {
                    writer = writer.write_bits(byte as u64, 8)?;
                }
                Ok(writer)
            }
            PacketType::Fragment { data, fragment_id, total_fragments } => {
                let mut writer = writer.write_bits(7, 4)?
                    .write_bits(*fragment_id as u64, 6)?
                    .write_bits(*total_fragments as u64, 6)?
                    .write_bits(data.len() as u64, 10)?;
                for &byte in data {
                    writer = writer.write_bits(byte as u64, 8)?;
                }
                Ok(writer)
            }
            PacketType::Input(data) => {
                let writer = writer.write_bits(8, 4)?;
                data.serialize(writer)
            }
        }
    }

    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (sequence, reader) = reader.read_bits(8)?;
        let (ack, reader) = reader.read_bits(8)?;
        let (ack_bits, reader) = reader.read_bits(8)?;
        let (channel_id, reader) = reader.read_bits(2)?;
        let (connection_id, reader) = reader.read_bits(12)?;

        let header = PacketHeader {
            sequence: sequence as u16,
            ack: ack as u16,
            ack_bits: ack_bits as u16,
            channel_id: channel_id as u8,
            connection_id: connection_id as u16,
        };

        let (packet_type_code, reader) = reader.read_bits(4)?;
        let (packet_type, reader) = match packet_type_code {
            0 => (PacketType::ConnectRequest, reader),
            1 => (PacketType::ConnectAccept, reader),
            2 => (PacketType::Disconnect, reader),
            3 => (PacketType::KeepAlive, reader),
            4 => {
                let (ordered, reader) = reader.read_bit()?;
                let (data, reader) = T::deserialize(reader)?;
                (PacketType::Data { data, ordered }, reader)
            }
            5 => {
                let (timestamp, reader) = reader.read_bits(24)?;
                let (data, reader) = T::deserialize(reader)?;
                (PacketType::Snapshot { data, timestamp: timestamp as u32 }, reader)
            }
            6 => {
                let (timestamp, reader) = reader.read_bits(24)?;
                let (len, reader) = reader.read_bits(10)?;
                let mut delta = vec![0u8; len as usize];
                let mut reader = reader;
                for byte in delta.iter_mut() {
                    let (b, r) = reader.read_bits(8)?;
                    *byte = b as u8;
                    reader = r;
                }
                (PacketType::SnapshotDelta { delta, timestamp: timestamp as u32 }, reader)
            }
            7 => {
                let (fragment_id, reader) = reader.read_bits(6)?;
                let (total_fragments, reader) = reader.read_bits(6)?;
                let (len, reader) = reader.read_bits(10)?;
                let mut data = vec![0u8; len as usize];
                let mut reader = reader;
                for byte in data.iter_mut() {
                    let (b, r) = reader.read_bits(8)?;
                    *byte = b as u8;
                    reader = r;
                }
                (PacketType::Fragment {
                    data,
                    fragment_id: fragment_id as u8,
                    total_fragments: total_fragments as u8,
                }, reader)
            }
            8 => {
                let (data, reader) = T::deserialize(reader)?;
                (PacketType::Input(data), reader)
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Unknown packet type")),
        };

        let packet = Packet { header: header.clone(), packet_type: packet_type.clone() };
        trace!("Deserialized packet: sequence {}, type {:?}", header.sequence, packet_type);
        Ok((packet, reader))
    }
}