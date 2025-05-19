use super::reliability::Reliability;
use std::net::SocketAddr;
use std::time::Instant;
use log::info;

#[derive(Debug, Clone, Copy)]
pub enum ChannelType {
    ReliableOrdered,
    ReliableUnordered,
    Unreliable,
    Sequenced,
    ReliableSequenced,
}

pub type ChannelId = u8;

#[derive(Debug)]
pub struct Channel {
    id: ChannelId,
    channel_type: ChannelType,
    reliability: Reliability,
}

impl Channel {
    pub fn new(id: ChannelId, channel_type: ChannelType) -> Self {
        info!("Creating channel {}: {:?}", id, channel_type);
        Channel {
            id,
            channel_type,
            reliability: Reliability::new(),
        }
    }

    pub fn prepare_packet(&mut self, packet: super::packet::Packet, addr: SocketAddr) -> super::packet::Packet {
        let mut packet = match self.channel_type {
            ChannelType::ReliableOrdered | ChannelType::ReliableUnordered | ChannelType::ReliableSequenced => {
                self.reliability.prepare_packet(packet, addr)
            }
            _ => packet,
        };
        packet.header.channel_id = self.id;
        packet
    }

    pub fn on_packet_sent(&mut self, packet: super::packet::Packet, sent_time: Instant, addr: SocketAddr) {
        if matches!(self.channel_type, ChannelType::ReliableOrdered | ChannelType::ReliableUnordered | ChannelType::ReliableSequenced) {
            self.reliability.on_packet_sent(packet, sent_time, addr);
        }
    }

    pub fn on_packet_received(&mut self, packet: super::packet::Packet, addr: SocketAddr) -> Option<super::packet::Packet> {
        match self.channel_type {
            ChannelType::ReliableOrdered => self.reliability.on_packet_received(packet, addr, true),
            ChannelType::ReliableUnordered => self.reliability.on_packet_received(packet, addr, false),
            ChannelType::Sequenced | ChannelType::ReliableSequenced => {
                let is_reliable = matches!(self.channel_type, ChannelType::ReliableSequenced);
                self.reliability.on_packet_received_sequenced(packet, addr, is_reliable)
            }
            ChannelType::Unreliable => Some(packet),
        }
    }

    pub fn on_packet_acked(&mut self, sequence: u16, addr: SocketAddr) {
        if matches!(self.channel_type, ChannelType::ReliableOrdered | ChannelType::ReliableUnordered | ChannelType::ReliableSequenced) {
            self.reliability.on_packet_acked(sequence, addr);
        }
    }

    pub fn check_retransmissions(&mut self, now: Instant) -> Vec<super::reliability::ReliablePacket> {
        if matches!(self.channel_type, ChannelType::ReliableOrdered | ChannelType::ReliableUnordered | ChannelType::ReliableSequenced) {
            self.reliability.check_retransmissions(now)
        } else {
            Vec::new()
        }
    }
}