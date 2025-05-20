use super::reliability::Reliability;
use super::packet::{Packet, PacketType};
use std::net::SocketAddr;
use std::time::Instant;
use log::info;
use std::marker::PhantomData;

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
    _phantom: PhantomData<T>,
}

impl<T: super::serialize::Serialize + Clone> Channel<T> {
    pub fn new(id: ChannelId, channel_type: ChannelType) -> Self {
        info!("Creating channel {}: {:?}", id, channel_type);
        Channel {
            id,
            channel_type,
            _phantom: PhantomData,
        }
    }

    pub fn prepare_packet(
        &self,
        packet: Packet<T>,
        addr: SocketAddr,
        reliability: Reliability<T>,
    ) -> (Packet<T>, Reliability<T>) {
        let reliable = matches!(self.channel_type, ChannelType::Reliable)
            && matches!(packet.packet_type, PacketType::Data { ordered: true, .. } | PacketType::Input(_));
        let (mut packet, reliability) = reliability.prepare_packet(packet, addr, reliable);
        packet.header.channel_id = self.id;
        (packet, reliability)
    }

    pub fn on_packet_sent(
        &self,
        packet: Packet<T>,
        sent_time: Instant,
        addr: SocketAddr,
        reliability: Reliability<T>,
    ) -> Reliability<T> {
        if self.channel_type == ChannelType::Reliable {
            reliability.on_packet_sent(packet, sent_time, addr)
        } else {
            reliability
        }
    }

    pub fn on_packet_received(
        &self,
        packet: Packet<T>,
        addr: SocketAddr,
        reliability: Reliability<T>,
    ) -> (Option<Packet<T>>, Reliability<T>) {
        match self.channel_type {
            ChannelType::Reliable => {
                let ordered = matches!(packet.packet_type, PacketType::Data { ordered: true, .. } | PacketType::Input(_));
                reliability.on_packet_received(packet, addr, ordered)
            }
            ChannelType::Unreliable => {
                if matches!(packet.packet_type, PacketType::Data { ordered: false, .. }) {
                    reliability.on_packet_received_sequenced(packet, addr)
                } else {
                    (Some(packet), reliability)
                }
            }
            ChannelType::Snapshot => reliability.on_snapshot_received(packet, addr),
        }
    }

    pub fn on_packet_acked(
        &self,
        sequence: u16,
        addr: SocketAddr,
        reliability: Reliability<T>,
    ) -> Reliability<T> {
        if self.channel_type == ChannelType::Reliable {
            reliability.on_packet_acked(sequence, addr)
        } else {
            reliability
        }
    }

    pub fn check_retransmissions(
        &self,
        now: Instant,
        reliability: Reliability<T>,
    ) -> (Vec<super::reliability::ReliablePacket<T>>, Reliability<T>) {
        if self.channel_type == ChannelType::Reliable {
            reliability.check_retransmissions(now)
        } else {
            (Vec::new(), reliability)
        }
    }
}