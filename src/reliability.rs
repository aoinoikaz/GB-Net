use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use log::{info, trace, warn};

const RETRANSMIT_TIMEOUT: Duration = Duration::from_millis(200);
const MAX_ACK_BITS: u32 = 32;

#[derive(Debug, Clone)]
pub struct ReliablePacket {
    pub packet: super::packet::Packet,
    pub sent_time: Instant,
    pub sequence: u16,
    pub addr: SocketAddr,
}

#[derive(Debug)]
pub struct Reliability {
    sent_packets: HashMap<(SocketAddr, u16), ReliablePacket>,
    pending_acks: HashMap<SocketAddr, VecDeque<u16>>,
    next_sequence: u16,
    ordered_buffer: HashMap<SocketAddr, VecDeque<super::packet::Packet>>,
    last_delivered_sequence: HashMap<SocketAddr, u16>,
    latest_sequence: HashMap<SocketAddr, u16>,
}

impl Reliability {
    pub fn new() -> Self {
        Reliability {
            sent_packets: HashMap::new(),
            pending_acks: HashMap::new(),
            next_sequence: 0,
            ordered_buffer: HashMap::new(),
            last_delivered_sequence: HashMap::new(),
            latest_sequence: HashMap::new(),
        }
    }

    pub fn prepare_packet(&mut self, packet: super::packet::Packet, addr: SocketAddr) -> super::packet::Packet {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let (ack, ack_bits) = self.generate_acks(addr);
        super::packet::Packet {
            header: super::packet::PacketHeader {
                sequence,
                ack,
                ack_bits,
                ..packet.header
            },
            packet_type: packet.packet_type,
        }
    }

    pub fn on_packet_sent(&mut self, packet: super::packet::Packet, sent_time: Instant, addr: SocketAddr) {
        let sequence = packet.header.sequence;
        self.sent_packets.insert((addr, sequence), ReliablePacket {
            packet,
            sent_time,
            sequence,
            addr,
        });
        info!("Tracking packet for retransmission: sequence {} to {}", sequence, addr);
    }

    pub fn on_packet_received(&mut self, packet: super::packet::Packet, addr: SocketAddr, ordered: bool) -> Option<super::packet::Packet> {
        let sequence = packet.header.sequence;
        self.pending_acks
            .entry(addr)
            .or_insert_with(VecDeque::new)
            .push_back(sequence);
        info!("Received packet from {}: sequence {}, data {:?}", addr, sequence, packet.packet_type);

        if ordered {
            let buffer = self.ordered_buffer.entry(addr).or_insert_with(VecDeque::new);
            let last_delivered = self.last_delivered_sequence.entry(addr).or_insert(0);
            let expected_sequence = last_delivered.wrapping_add(1);

            trace!("Processing ordered packet: sequence {}, expected {}, buffer size {}", 
                   sequence, expected_sequence, buffer.len());

            // Buffer all packets with sequence >= expected_sequence
            if sequence >= expected_sequence {
                let mut insert_pos = 0;
                for (i, p) in buffer.iter().enumerate() {
                    if p.header.sequence >= sequence {
                        break;
                    }
                    insert_pos = i + 1;
                }
                buffer.insert(insert_pos, packet);
                trace!("Inserted packet sequence {} at position {}, buffer now: {:?}", 
                       sequence, insert_pos, buffer.iter().map(|p| (p.header.sequence, &p.packet_type)).collect::<Vec<_>>());
            } else {
                trace!("Ignoring old or duplicate packet: sequence {}, expected {}", sequence, expected_sequence);
                return None;
            }

            // Deliver all consecutive packets starting from expected_sequence
            while let Some(next_packet) = buffer.front() {
                if next_packet.header.sequence == expected_sequence {
                    let delivered_packet = buffer.pop_front().unwrap();
                    *last_delivered = delivered_packet.header.sequence;
                    info!("Delivered ordered packet: sequence {}, data {:?}", 
                          delivered_packet.header.sequence, delivered_packet.packet_type);
                    trace!("Buffer after delivery: {:?}", 
                           buffer.iter().map(|p| (p.header.sequence, &p.packet_type)).collect::<Vec<_>>());
                    return Some(delivered_packet);
                } else {
                    trace!("No deliverable packet: front sequence {}, expected {}", 
                           next_packet.header.sequence, expected_sequence);
                    break;
                }
            }
            None
        } else {
            Some(packet)
        }
    }

    pub fn on_packet_received_sequenced(&mut self, packet: super::packet::Packet, addr: SocketAddr, reliable: bool) -> Option<super::packet::Packet> {
        let sequence = packet.header.sequence;
        let latest = self.latest_sequence.entry(addr).or_insert(0);

        if sequence > *latest {
            *latest = sequence;
            if reliable {
                self.pending_acks
                    .entry(addr)
                    .or_insert_with(VecDeque::new)
                    .push_back(sequence);
                info!("Received sequenced packet from {}: sequence {}", addr, sequence);
            }
            Some(packet)
        } else {
            info!("Discarded old sequenced packet from {}: sequence {} (latest: {})", addr, sequence, *latest);
            None
        }
    }

    pub fn generate_acks(&mut self, addr: SocketAddr) -> (u16, u32) {
        let pending = self.pending_acks.entry(addr).or_insert_with(VecDeque::new);
        let latest_sequence = pending.back().copied().unwrap_or(0);
        let mut ack_bits: u32 = 0;

        for i in 1..=MAX_ACK_BITS {
            let seq = latest_sequence.wrapping_sub(i as u16);
            if pending.contains(&seq) {
                ack_bits |= 1 << (i - 1);
            }
        }

        pending.retain(|&seq| {
            let diff = latest_sequence.wrapping_sub(seq);
            diff <= MAX_ACK_BITS as u16
        });

        (latest_sequence, ack_bits)
    }

    pub fn check_retransmissions(&mut self, now: Instant) -> Vec<ReliablePacket> {
        let mut retransmit = Vec::new();
        let mut to_remove = Vec::new();

        for packet in self.sent_packets.values() {
            if now.duration_since(packet.sent_time) > RETRANSMIT_TIMEOUT {
                retransmit.push(packet.clone());
                warn!("Retransmitting packet: sequence {} to {}", packet.sequence, packet.addr);
            }
        }

        for packet in retransmit.iter() {
            to_remove.push((packet.addr, packet.sequence));
        }
        for key in to_remove {
            self.sent_packets.remove(&key);
        }

        retransmit
    }

    pub fn on_packet_acked(&mut self, sequence: u16, addr: SocketAddr) {
        if self.sent_packets.remove(&(addr, sequence)).is_some() {
            info!("Packet acknowledged: sequence {} from {}", sequence, addr);
        }
    }
}