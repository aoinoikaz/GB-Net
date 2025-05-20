use super::reliability::Reliability;
use super::packet::{Packet, PacketType};
use std::net::SocketAddr;
use std::time::Instant;
use log::info;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelType {
    Reliable,
    Unreliable,
    Snapshot,
}

pub type ChannelId = u8;

#[derive(Debug)]
pub struct Channel<T: super::serialize::Serialize + Clone> {
    id: ChannelId,
    channel_type: ChannelType,
    reliability: Reliability<T>,
}

impl<T: super::serialize::Serialize + Clone> Channel<T> {
    pub fn new(id: ChannelId, channel_type: ChannelType) -> Self {
        info!("Creating channel {}: {:?}", id, channel_type);
        Channel {
            id,
            channel_type,
            reliability: Reliability::new(),
        }
    }

    pub fn prepare_packet(&mut self, packet: Packet<T>, addr: SocketAddr) -> Packet<T> {
        let mut packet = match self.channel_type {
            ChannelType::Reliable => {
                let reliable = matches!(packet.packet_type, 
                    PacketType::Data { data: _, ordered: true } | PacketType::Input(_));
                self.reliability.prepare_packet(packet, addr, reliable)
            }
            ChannelType::Snapshot => {
                self.reliability.prepare_packet(packet, addr, false)
            }
            ChannelType::Unreliable => packet,
        };
        packet.header.channel_id = self.id;
        packet
    }

    pub fn on_packet_sent(&mut self, packet: Packet<T>, sent_time: Instant, addr: SocketAddr) {
        if self.channel_type == ChannelType::Reliable {
            self.reliability.on_packet_sent(packet, sent_time, addr);
        }
    }

    pub fn on_packet_received(&mut self, packet: Packet<T>, addr: SocketAddr) -> Option<Packet<T>> {
        match self.channel_type {
            ChannelType::Reliable => {
                let ordered = matches!(packet.packet_type, 
                    PacketType::Data { data: _, ordered: true } | PacketType::Input(_));
                self.reliability.on_packet_received(packet, addr, ordered)
            }
            ChannelType::Unreliable => {
                if matches!(packet.packet_type, PacketType::Data { data: _, ordered: false }) {
                    self.reliability.on_packet_received_sequenced(packet, addr)
                } else {
                    Some(packet)
                }
            }
            ChannelType::Snapshot => self.reliability.on_snapshot_received(packet, addr),
        }
    }

    pub fn on_packet_acked(&mut self, sequence: u16, addr: SocketAddr) {
        if self.channel_type == ChannelType::Reliable {
            self.reliability.on_packet_acked(sequence, addr);
        }
    }

    pub fn check_retransmissions(&mut self, now: Instant) -> Vec<super::reliability::ReliablePacket<T>> {
        if self.channel_type == ChannelType::Reliable {
            self.reliability.check_retransmissions(now)
        } else {
            Vec::new()
        }
    }
}