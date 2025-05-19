use log::trace;
use std::io;
use super::serialize::{BitReader, BitWriter};

#[derive(Debug, Clone, PartialEq)]
pub struct PacketHeader {
    pub sequence: u16,
    pub ack: u16,
    pub ack_bits: u32,
    pub channel_id: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PacketType {
    Data(Vec<u8>),
    KeepAlive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub header: PacketHeader,
    pub packet_type: PacketType,
}

impl Packet {
    pub fn new_data(sequence: u16, channel_id: u8, data: Vec<u8>) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
            },
            packet_type: PacketType::Data(data),
        }
    }

    pub fn new_keep_alive(sequence: u16, channel_id: u8) -> Self {
        Packet {
            header: PacketHeader {
                sequence,
                ack: 0,
                ack_bits: 0,
                channel_id,
            },
            packet_type: PacketType::KeepAlive,
        }
    }

    pub fn serialize(&self, writer: &mut BitWriter) -> io::Result<()> {
        writer.write_bits(self.header.sequence as u64, 16)?;
        writer.write_bits(self.header.ack as u64, 16)?;
        writer.write_bits(self.header.ack_bits as u64, 32)?;
        writer.write_bits(self.header.channel_id as u64, 8)?;
        match &self.packet_type {
            PacketType::Data(data) => {
                writer.write_bits(1, 1)?; // is_data flag
                writer.write_bits(data.len() as u64, 16)?;
                for &byte in data {
                    writer.write_bits(byte as u64, 8)?;
                }
                trace!("Serialized data packet: sequence {}, channel {}, data {:?}", 
                       self.header.sequence, self.header.channel_id, data);
            }
            PacketType::KeepAlive => {
                writer.write_bits(0, 1)?; // is_data flag
                trace!("Serialized keep-alive packet: sequence {}, channel {}", 
                       self.header.sequence, self.header.channel_id);
            }
        }
        writer.flush()?;
        Ok(())
    }

    pub fn deserialize(reader: &mut BitReader) -> io::Result<Self> {
        let sequence = reader.read_bits(16)? as u16;
        let ack = reader.read_bits(16)? as u16;
        let ack_bits = reader.read_bits(32)? as u32;
        let channel_id = reader.read_bits(8)? as u8;
        let is_data = reader.read_bits(1)? == 1;
        trace!("Deserialized header: sequence {}, ack {}, ack_bits {:08x}, channel_id {}, is_data {}", 
               sequence, ack, ack_bits, channel_id, is_data);
        
        let packet_type = if is_data {
            let len = reader.read_bits(16)? as usize;
            let mut data = Vec::with_capacity(len);
            for _ in 0..len {
                let byte = reader.read_bits(8)? as u8;
                data.push(byte);
            }
            trace!("Deserialized data: {:?}", data);
            PacketType::Data(data)
        } else {
            trace!("Deserialized keep-alive");
            PacketType::KeepAlive
        };

        Ok(Packet {
            header: PacketHeader {
                sequence,
                ack,
                ack_bits,
                channel_id,
            },
            packet_type,
        })
    }
}