use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use log::{info, trace, warn};
use super::packet::{Packet, PacketHeader, PacketType};
use super::serialize::Serialize;

const RETRANSMIT_TIMEOUT: Duration = Duration::from_millis(200);
const MAX_ACK_BITS: u32 = 32;
const MAX_FRAGMENT_SIZE: usize = 1200;
const SNAPSHOT_DELTA_THRESHOLD: usize = 50;
const WINDOW_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct ReliablePacket<T: Serialize + Clone> {
    pub packet: Packet<T>,
    pub sent_time: Instant,
    pub sequence: u16,
    pub addr: SocketAddr,
}

#[derive(Debug)]
struct FragmentBuffer<T: Serialize + Clone> {
    fragments: HashMap<u16, Packet<T>>,
    total_fragments: u16,
    received_fragments: u16,
    sequence: u16,
}

#[derive(Debug)]
struct SnapshotBuffer {
    previous_data: Option<Vec<u8>>,
    latest_sequence: u16,
}

#[derive(Debug)]
pub struct Reliability<T: Serialize + Clone> {
    sent_packets: HashMap<(SocketAddr, u16), ReliablePacket<T>>,
    pending_acks: HashMap<SocketAddr, VecDeque<u16>>,
    next_sequence: u16,
    ordered_buffer: HashMap<SocketAddr, VecDeque<Packet<T>>>,
    last_delivered_sequence: HashMap<SocketAddr, u16>,
    latest_sequence: HashMap<SocketAddr, u16>,
    fragment_buffers: HashMap<(SocketAddr, u16), FragmentBuffer<T>>,
    snapshot_buffers: HashMap<SocketAddr, SnapshotBuffer>,
    send_window: HashMap<SocketAddr, VecDeque<u16>>,
}

impl<T: Serialize + Clone> Reliability<T> {
    pub fn new() -> Self {
        Reliability {
            sent_packets: HashMap::new(),
            pending_acks: HashMap::new(),
            next_sequence: 0,
            ordered_buffer: HashMap::new(),
            last_delivered_sequence: HashMap::new(),
            latest_sequence: HashMap::new(),
            fragment_buffers: HashMap::new(),
            snapshot_buffers: HashMap::new(),
            send_window: HashMap::new(),
        }
    }

    pub fn prepare_packet(&mut self, mut packet: Packet<T>, addr: SocketAddr, reliable: bool) -> Packet<T> {
        // Pre-compute all packet data
        let snapshot_data = match &packet.packet_type {
            PacketType::Snapshot { data, timestamp } => {
                let mut writer = super::serialize::BitWriter::new();
                data.serialize(&mut writer).unwrap();
                Some((writer.into_bytes(), *timestamp))
            }
            _ => None,
        };

        let fragment_bytes = match &packet.packet_type {
            PacketType::Data { data, .. } | PacketType::Snapshot { data, .. } => {
                let mut writer = super::serialize::BitWriter::new();
                data.serialize(&mut writer).unwrap();
                Some(writer.into_bytes())
            }
            _ => None,
        };

        // Assign sequence and ACKs
        {
            packet.header.sequence = self.next_sequence;
            let pending = self.pending_acks.entry(addr).or_insert_with(VecDeque::new);
            let latest_sequence = pending.back().copied().unwrap_or(0);
            let mut ack_bits: u32 = 0;

            for i in 1..=MAX_ACK_BITS {
                let seq = latest_sequence.wrapping_sub(i as u16);
                if pending.contains(&seq) {
                    ack_bits |= 1 << (i - 1);
                }
            }

            pending.retain(|&seq| latest_sequence.wrapping_sub(seq) <= MAX_ACK_BITS as u16);
            packet.header.ack = latest_sequence;
            packet.header.ack_bits = ack_bits;
            self.next_sequence = self.next_sequence.wrapping_add(1);
        }

        // Handle reliable window
        if reliable {
            let window = self.send_window.entry(addr).or_insert_with(VecDeque::new);
            if window.len() >= WINDOW_SIZE {
                trace!("Send window full for {}, delaying packet sequence {}", addr, packet.header.sequence);
                return packet;
            }
            window.push_back(packet.header.sequence);
        }

        // Handle snapshot delta compression
        if let Some((curr_bytes, timestamp)) = snapshot_data {
            let snapshot_buffer = self.snapshot_buffers.entry(addr).or_insert_with(|| SnapshotBuffer {
                previous_data: None,
                latest_sequence: 0,
            });
            if let Some(prev_data) = snapshot_buffer.previous_data.as_ref() {
                let delta = Self::compute_delta(prev_data, &curr_bytes);
                if delta.len() + 1 < curr_bytes.len().saturating_sub(SNAPSHOT_DELTA_THRESHOLD) {
                    packet.packet_type = PacketType::SnapshotDelta { delta, timestamp };
                    trace!("Applied delta compression for snapshot sequence {}", packet.header.sequence);
                }
            }
            snapshot_buffer.previous_data = Some(curr_bytes);
        }

        // Handle fragmentation
        if let Some(bytes) = fragment_bytes {
            if bytes.len() > MAX_FRAGMENT_SIZE {
                let fragments = Self::fragment_packet(&packet, &bytes, packet.header.sequence);
                return fragments.into_iter().next().unwrap();
            }
        }

        packet
    }

    fn compute_delta(prev: &[u8], curr: &[u8]) -> Vec<u8> {
        let len = prev.len().min(curr.len());
        let mut delta = Vec::new();
        for i in 0..len {
            delta.push(curr[i].wrapping_sub(prev[i]));
        }
        if curr.len() > len {
            delta.extend_from_slice(&curr[len..]);
        }
        delta
    }

    fn apply_delta(prev: &[u8], delta: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        for i in 0..prev.len().min(delta.len()) {
            result.push(prev[i].wrapping_add(delta[i]));
        }
        if delta.len() > prev.len() {
            result.extend_from_slice(&delta[prev.len()..]);
        }
        result
    }

    fn fragment_packet(packet: &Packet<T>, data: &[u8], sequence: u16) -> Vec<Packet<T>> {
        let mut fragments = Vec::new();
        let total_fragments = ((data.len() as f32) / (MAX_FRAGMENT_SIZE as f32)).ceil() as u16;
        for fragment_id in 0..total_fragments {
            let start = fragment_id as usize * MAX_FRAGMENT_SIZE;
            let end = (start + MAX_FRAGMENT_SIZE).min(data.len());
            let fragment_data = data[start..end].to_vec();
            let fragment_packet = Packet {
                header: PacketHeader {
                    sequence,
                    ack: packet.header.ack,
                    ack_bits: packet.header.ack_bits,
                    channel_id: packet.header.channel_id,
                    fragment_id: Some(fragment_id),
                    total_fragments: Some(total_fragments),
                    timestamp: packet.header.timestamp,
                    priority: packet.header.priority,
                    connection_id: packet.header.connection_id,
                },
                packet_type: PacketType::Fragment(fragment_data),
            };
            fragments.push(fragment_packet);
        }
        trace!("Fragmented packet sequence {} into {} fragments", sequence, total_fragments);
        fragments
    }

    pub fn on_packet_sent(&mut self, packet: Packet<T>, sent_time: Instant, addr: SocketAddr) {
        let sequence = packet.header.sequence;
        if matches!(packet.packet_type, 
            PacketType::Data { ordered: _, .. } | PacketType::Fragment(_) | PacketType::Input(_)) {
            self.sent_packets.insert((addr, sequence), ReliablePacket {
                packet,
                sent_time,
                sequence,
                addr,
            });
            info!("Tracking packet for retransmission: sequence {} to {}", sequence, addr);
        }
    }

    pub fn on_packet_received(&mut self, packet: Packet<T>, addr: SocketAddr, ordered: bool) -> Option<Packet<T>> {
        let sequence = packet.header.sequence;

        if let PacketType::Fragment(_data) = &packet.packet_type {
            if let (Some(fragment_id), Some(total_fragments)) = (packet.header.fragment_id, packet.header.total_fragments) {
                let key = (addr, sequence);
                let fragment_buffer = self.fragment_buffers.entry(key).or_insert_with(|| FragmentBuffer {
                    fragments: HashMap::new(),
                    total_fragments,
                    received_fragments: 0,
                    sequence,
                });

                fragment_buffer.fragments.insert(fragment_id, packet.clone());
                fragment_buffer.received_fragments += 1;
                trace!("Received fragment {}/{} for sequence {} from {}", 
                       fragment_id, total_fragments, sequence, addr);

                if fragment_buffer.received_fragments == total_fragments {
                    let mut data = Vec::new();
                    for i in 0..total_fragments {
                        if let Some(fragment) = fragment_buffer.fragments.get(&i) {
                            if let PacketType::Fragment(fragment_data) = &fragment.packet_type {
                                data.extend_from_slice(fragment_data);
                            }
                        }
                    }
                    let mut reader = super::serialize::BitReader::new(data);
                    let reassembled_data = T::deserialize(&mut reader).ok()?;
                    let reassembled_packet = Packet {
                        header: PacketHeader {
                            sequence,
                            ack: packet.header.ack,
                            ack_bits: packet.header.ack_bits,
                            channel_id: packet.header.channel_id,
                            fragment_id: None,
                            total_fragments: None,
                            timestamp: packet.header.timestamp,
                            priority: packet.header.priority,
                            connection_id: packet.header.connection_id,
                        },
                        packet_type: PacketType::Data { data: reassembled_data, ordered: false },
                    };
                    self.fragment_buffers.remove(&key);
                    trace!("Reassembled packet sequence {} from {} fragments", sequence, total_fragments);
                    return self.process_packet(reassembled_packet, addr, ordered);
                }
                return None;
            }
        }

        self.process_packet(packet, addr, ordered)
    }

    fn process_packet(&mut self, packet: Packet<T>, addr: SocketAddr, ordered: bool) -> Option<Packet<T>> {
        let sequence = packet.header.sequence;
        let pending_acks = self.pending_acks.entry(addr).or_insert_with(VecDeque::new);
        pending_acks.push_back(sequence);
        info!("Received packet from {}: sequence {}", addr, sequence);

        if ordered {
            let buffer = self.ordered_buffer.entry(addr).or_insert_with(VecDeque::new);
            let last_delivered = self.last_delivered_sequence.entry(addr).or_insert(0);
            let expected_sequence = last_delivered.wrapping_add(1);

            if sequence.wrapping_sub(expected_sequence) < u16::MAX / 2 {
                let mut insert_pos = 0;
                for (i, p) in buffer.iter().enumerate() {
                    if p.header.sequence >= sequence {
                        break;
                    }
                    insert_pos = i + 1;
                }
                buffer.insert(insert_pos, packet);
                trace!("Inserted packet sequence {} at position {}", sequence, insert_pos);
            } else {
                trace!("Ignoring old packet: sequence {}, expected {}", sequence, expected_sequence);
                return None;
            }

            if let Some(next_packet) = buffer.front() {
                if next_packet.header.sequence == expected_sequence {
                    let delivered_packet = buffer.pop_front().unwrap();
                    *last_delivered = delivered_packet.header.sequence;
                    info!("Delivered ordered packet: sequence {}", delivered_packet.header.sequence);
                    return Some(delivered_packet);
                }
            }
            None
        } else {
            Some(packet)
        }
    }

    pub fn on_snapshot_received(&mut self, packet: Packet<T>, addr: SocketAddr) -> Option<Packet<T>> {
        let sequence = packet.header.sequence;
        let snapshot_buffer = self.snapshot_buffers.entry(addr).or_insert_with(|| SnapshotBuffer {
            previous_data: None,
            latest_sequence: 0,
        });

        if sequence <= snapshot_buffer.latest_sequence {
            info!("Discarded old snapshot from {}: sequence {} (latest: {})", addr, sequence, snapshot_buffer.latest_sequence);
            return None;
        }

        match &packet.packet_type {
            PacketType::Snapshot { data, .. } => {
                let mut writer = super::serialize::BitWriter::new();
                data.serialize(&mut writer).unwrap();
                let curr_bytes = writer.into_bytes();
                snapshot_buffer.previous_data = Some(curr_bytes);
                snapshot_buffer.latest_sequence = sequence;
                info!("Received snapshot from {}: sequence {}", addr, sequence);
                Some(Packet {
                    header: packet.header,
                    packet_type: packet.packet_type.clone(),
                })
            }
            PacketType::SnapshotDelta { delta, timestamp } => {
                if let Some(prev_data) = snapshot_buffer.previous_data.as_ref() {
                    let data_bytes = Self::apply_delta(prev_data, delta);
                    let mut reader = super::serialize::BitReader::new(data_bytes);
                    if let Ok(data) = T::deserialize(&mut reader) {
                        snapshot_buffer.previous_data = Some(reader.into_bytes());
                        snapshot_buffer.latest_sequence = sequence;
                        info!("Received delta snapshot from {}: sequence {}", addr, sequence);
                        Some(Packet {
                            header: packet.header,
                            packet_type: PacketType::Snapshot { data, timestamp: *timestamp },
                        })
                    } else {
                        trace!("Failed to deserialize delta snapshot: sequence {}", sequence);
                        None
                    }
                } else {
                    trace!("Ignoring delta snapshot without previous data: sequence {}", sequence);
                    None
                }
            }
            _ => Some(packet),
        }
    }

    pub fn on_packet_received_sequenced(&mut self, packet: Packet<T>, addr: SocketAddr) -> Option<Packet<T>> {
        let sequence = packet.header.sequence;
        let latest = self.latest_sequence.entry(addr).or_insert(0);

        if sequence > *latest {
            *latest = sequence;
            info!("Received sequenced packet from {}: sequence {}", addr, sequence);
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

        pending.retain(|&seq| latest_sequence.wrapping_sub(seq) <= MAX_ACK_BITS as u16);
        (latest_sequence, ack_bits)
    }

    pub fn check_retransmissions(&mut self, now: Instant) -> Vec<ReliablePacket<T>> {
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
        if let Some(packet) = self.sent_packets.remove(&(addr, sequence)) {
            info!("Packet acknowledged: sequence {} from {}", sequence, addr);
            let window = self.send_window.entry(addr).or_insert_with(VecDeque::new);
            window.retain(|&s| s != sequence);
            for i in 0..MAX_ACK_BITS {
                if (packet.packet.header.ack_bits & (1 << i)) != 0 {
                    let acked_sequence = sequence.wrapping_sub(i as u16 + 1);
                    if self.sent_packets.remove(&(addr, acked_sequence)).is_some() {
                        info!("Packet acknowledged via ack_bits: sequence {} from {}", acked_sequence, addr);
                        window.retain(|&s| s != acked_sequence);
                    }
                }
            }
        }
    }
}