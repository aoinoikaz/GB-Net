use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use log::{info, trace, warn};
use super::packet::{Packet, PacketHeader, PacketType};
use super::serialize::{BitReader, BitWriter, Serialize};

const RETRANSMIT_TIMEOUT: Duration = Duration::from_millis(200);
const MAX_ACK_BITS: u32 = 8; // Matches 8-bit ack_bits in packet.rs
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

#[derive(Debug, Clone)]
struct FragmentBuffer<T: Serialize + Clone> {
    fragments: HashMap<u16, Packet<T>>,
    total_fragments: u16,
    received_fragments: u16,
    sequence: u16,
}

#[derive(Debug, Clone)]
struct SnapshotBuffer {
    previous_data: Option<Vec<u8>>,
    latest_sequence: u16,
}

#[derive(Debug, Clone)]
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

    pub fn prepare_packet(self, mut packet: Packet<T>, addr: SocketAddr, reliable: bool) -> (Packet<T>, Self) {
        let mut state = self;
        let sequence = state.next_sequence;
        packet.header.sequence = sequence;

        // Generate ACKs
        let pending = state.pending_acks.get(&addr).cloned().unwrap_or_else(VecDeque::new);
        let latest_sequence = pending.back().copied().unwrap_or(0);
        let mut ack_bits: u16 = 0;

        for i in 1..=MAX_ACK_BITS {
            let seq = latest_sequence.wrapping_sub(i as u16);
            if pending.contains(&seq) {
                ack_bits |= 1 << (i - 1);
            }
        }

        let mut new_pending = pending;
        new_pending.retain(|&seq| latest_sequence.wrapping_sub(seq) <= MAX_ACK_BITS as u16);
        state.pending_acks.insert(addr, new_pending);

        packet.header.ack = latest_sequence;
        packet.header.ack_bits = ack_bits;
        state.next_sequence = state.next_sequence.wrapping_add(1);

        // Handle reliable window
        let mut state = if reliable {
            let mut window = state.send_window.get(&addr).cloned().unwrap_or_else(VecDeque::new);
            if window.len() >= WINDOW_SIZE {
                trace!("Send window full for {}, delaying packet sequence {}", addr, sequence);
                return (packet, state);
            }
            window.push_back(sequence);
            state.send_window.insert(addr, window);
            state
        } else {
            state
        };

        // Handle snapshot delta compression
        let snapshot_data = match &packet.packet_type {
            PacketType::Snapshot { data, timestamp } => {
                let writer = BitWriter::new();
                if let Ok(writer) = data.serialize(writer) {
                    Some((writer.into_bytes(), *timestamp))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((curr_bytes, timestamp)) = snapshot_data {
            let mut snapshot_buffer = state.snapshot_buffers.get(&addr).cloned().unwrap_or_else(|| SnapshotBuffer {
                previous_data: None,
                latest_sequence: 0,
            });
            if let Some(prev_data) = snapshot_buffer.previous_data.as_ref() {
                let delta = Self::compute_delta(prev_data, &curr_bytes);
                if delta.len() + 1 < curr_bytes.len().saturating_sub(SNAPSHOT_DELTA_THRESHOLD) {
                    packet.packet_type = PacketType::SnapshotDelta { delta, timestamp };
                    trace!("Applied delta compression for snapshot sequence {}", sequence);
                }
            }
            snapshot_buffer.previous_data = Some(curr_bytes.to_vec());
            state.snapshot_buffers.insert(addr, snapshot_buffer);
        }

        // Handle fragmentation
        let fragment_bytes = match &packet.packet_type {
            PacketType::Data { data, .. } | PacketType::Snapshot { data, .. } => {
                let writer = BitWriter::new();
                if let Ok(writer) = data.serialize(writer) {
                    Some(writer.into_bytes())
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(bytes) = fragment_bytes {
            if bytes.len() > MAX_FRAGMENT_SIZE {
                let fragments = Self::fragment_packet(&packet, &bytes, sequence);
                return (fragments.into_iter().next().unwrap(), state);
            }
        }

        (packet, state)
    }

    fn compute_delta(prev: &[u8], curr: &[u8]) -> Vec<u8> {
        let len = prev.len().min(curr.len());
        let mut delta = Vec::with_capacity(len);
        for i in 0..len {
            delta.push(curr[i].wrapping_sub(prev[i]));
        }
        if curr.len() > len {
            delta.extend_from_slice(&curr[len..]);
        }
        delta
    }

    fn apply_delta(prev: &[u8], delta: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(prev.len().max(delta.len()));
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
        let total_fragments = ((data.len() as f32) / (MAX_FRAGMENT_SIZE as f32)).ceil() as u8;
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
                    connection_id: packet.header.connection_id,
                },
                packet_type: PacketType::Fragment {
                    data: fragment_data,
                    fragment_id,
                    total_fragments,
                },
            };
            fragments.push(fragment_packet);
        }
        trace!("Fragmented packet sequence {} into {} fragments", sequence, total_fragments);
        fragments
    }

    pub fn on_packet_sent(self, packet: Packet<T>, sent_time: Instant, addr: SocketAddr) -> Self {
        let sequence = packet.header.sequence;
        let mut state = self;

        if matches!(packet.packet_type, 
            PacketType::Data { ordered: _, .. } | PacketType::Fragment { .. } | PacketType::Input(_)) {
            state.sent_packets.insert((addr, sequence), ReliablePacket {
                packet,
                sent_time,
                sequence,
                addr,
            });
            info!("Tracking packet for retransmission: sequence {} to {}", sequence, addr);
        }

        state
    }

    pub fn on_packet_received(self, packet: Packet<T>, addr: SocketAddr, ordered: bool) -> (Option<Packet<T>>, Self) {
        let sequence = packet.header.sequence;
        let mut state = self;

        if let PacketType::Fragment { data: _, fragment_id, total_fragments } = &packet.packet_type {
            let key = (addr, sequence);
            let fragment_buffer = state.fragment_buffers.get(&key).cloned().unwrap_or_else(|| FragmentBuffer {
                fragments: HashMap::new(),
                total_fragments: *total_fragments as u16,
                received_fragments: 0,
                sequence,
            });

            let mut new_fragment_buffer = fragment_buffer.clone();
            new_fragment_buffer.fragments.insert(*fragment_id as u16, packet.clone());
            new_fragment_buffer.received_fragments += 1;
            trace!("Received fragment {}/{} for sequence {} from {}", 
                   fragment_id, total_fragments, sequence, addr);

            if new_fragment_buffer.received_fragments == *total_fragments as u16 {
                let mut fragment_data = Vec::new();
                for i in 0..*total_fragments as u16 {
                    if let Some(fragment) = new_fragment_buffer.fragments.get(&i) {
                        if let PacketType::Fragment { data, .. } = &fragment.packet_type {
                            fragment_data.extend_from_slice(data);
                        }
                    }
                }
                let reader = BitReader::new(fragment_data);
                if let Ok((reassembled_data, _)) = T::deserialize(reader) {
                    let first_fragment = new_fragment_buffer.fragments.get(&0);
                    let reassembled_packet = Packet {
                        header: PacketHeader {
                            sequence,
                            ack: first_fragment.map(|p| p.header.ack).unwrap_or(0),
                            ack_bits: first_fragment.map(|p| p.header.ack_bits).unwrap_or(0),
                            channel_id: first_fragment.map(|p| p.header.channel_id).unwrap_or(0),
                            connection_id: first_fragment.map(|p| p.header.connection_id).unwrap_or(0),
                        },
                        packet_type: PacketType::Data { data: reassembled_data, ordered: false },
                    };
                    state.fragment_buffers.remove(&key);
                    trace!("Reassembled packet sequence {} from {} fragments", sequence, total_fragments);
                    return state.process_packet(reassembled_packet, addr, ordered);
                }
                state.fragment_buffers.insert(key, new_fragment_buffer);
                return (None, state);
            }
            state.fragment_buffers.insert(key, new_fragment_buffer);
            return (None, state);
        }

        state.process_packet(packet, addr, ordered)
    }

    fn process_packet(self, packet: Packet<T>, addr: SocketAddr, ordered: bool) -> (Option<Packet<T>>, Self) {
        let sequence = packet.header.sequence;
        let mut state = self;

        let mut pending_acks = state.pending_acks.get(&addr).cloned().unwrap_or_else(VecDeque::new);
        pending_acks.push_back(sequence);
        state.pending_acks.insert(addr, pending_acks);
        info!("Received packet from {}: sequence {}", addr, sequence);

        if ordered {
            let mut buffer = state.ordered_buffer.get(&addr).cloned().unwrap_or_else(VecDeque::new);
            let last_delivered = state.last_delivered_sequence.get(&addr).copied().unwrap_or(0);
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
                return (None, state);
            }

            if let Some(next_packet) = buffer.front() {
                if next_packet.header.sequence == expected_sequence {
                    let delivered_packet = buffer.pop_front().unwrap();
                    state.last_delivered_sequence.insert(addr, delivered_packet.header.sequence);
                    state.ordered_buffer.insert(addr, buffer);
                    info!("Delivered ordered packet: sequence {}", delivered_packet.header.sequence);
                    return (Some(delivered_packet), state);
                }
            }
            state.ordered_buffer.insert(addr, buffer);
            (None, state)
        } else {
            (Some(packet), state)
        }
    }

    pub fn on_snapshot_received(self, packet: Packet<T>, addr: SocketAddr) -> (Option<Packet<T>>, Self) {
        let sequence = packet.header.sequence;
        let mut state = self;
        let mut snapshot_buffer = state.snapshot_buffers.get(&addr).cloned().unwrap_or_else(|| SnapshotBuffer {
            previous_data: None,
            latest_sequence: 0,
        });

        if sequence <= snapshot_buffer.latest_sequence {
            info!("Discarded old snapshot from {}: sequence {} (latest: {})", addr, sequence, snapshot_buffer.latest_sequence);
            return (None, state);
        }

        let (packet_type, snapshot_buffer) = match &packet.packet_type {
            PacketType::Snapshot { data, timestamp: _ } => {
                let writer = BitWriter::new();
                if let Ok(writer) = data.serialize(writer) {
                    snapshot_buffer.previous_data = Some(writer.into_bytes());
                    snapshot_buffer.latest_sequence = sequence;
                    (packet.packet_type.clone(), snapshot_buffer)
                } else {
                    (packet.packet_type.clone(), snapshot_buffer)
                }
            }
            PacketType::SnapshotDelta { delta, timestamp } => {
                if let Some(prev_data) = snapshot_buffer.previous_data.as_ref() {
                    let data_bytes = Self::apply_delta(prev_data, delta);
                    let reader = BitReader::new(data_bytes.clone());
                    if let Ok((data, reader)) = T::deserialize(reader) {
                        snapshot_buffer.previous_data = Some(reader.into_bytes());
                        snapshot_buffer.latest_sequence = sequence;
                        (PacketType::Snapshot { data, timestamp: *timestamp }, snapshot_buffer)
                    } else {
                        trace!("Failed to deserialize delta snapshot: sequence {}", sequence);
                        (packet.packet_type.clone(), snapshot_buffer)
                    }
                } else {
                    trace!("Ignoring delta snapshot without previous data: sequence {}", sequence);
                    (packet.packet_type.clone(), snapshot_buffer)
                }
            }
            pt => (pt.clone(), snapshot_buffer),
        };

        state.snapshot_buffers.insert(addr, snapshot_buffer);
        let packet = Packet {
            header: packet.header,
            packet_type,
        };
        (Some(packet), state)
    }

    pub fn on_packet_received_sequenced(self, packet: Packet<T>, addr: SocketAddr) -> (Option<Packet<T>>, Self) {
        let sequence = packet.header.sequence;
        let mut state = self;
        let latest = state.latest_sequence.get(&addr).copied().unwrap_or(0);

        if sequence > latest {
            state.latest_sequence.insert(addr, sequence);
            info!("Received sequenced packet from {}: sequence {}", addr, sequence);
            (Some(packet), state)
        } else {
            info!("Discarded old sequenced packet from {}: sequence {} (latest: {})", addr, sequence, latest);
            (None, state)
        }
    }

    pub fn on_packet_acked(self, sequence: u16, addr: SocketAddr) -> Self {
        let mut state = self;
        if let Some(packet) = state.sent_packets.remove(&(addr, sequence)) {
            info!("Packet acknowledged: sequence {} from {}", sequence, addr);
            let mut window = state.send_window.get(&addr).cloned().unwrap_or_else(VecDeque::new);
            window.retain(|&s| s != sequence);
            for i in 0..MAX_ACK_BITS {
                if (packet.packet.header.ack_bits & (1 << i)) != 0 {
                    let acked_sequence = sequence.wrapping_sub(i as u16 + 1);
                    if state.sent_packets.remove(&(addr, acked_sequence)).is_some() {
                        info!("Packet acknowledged via ack_bits: sequence {} from {}", acked_sequence, addr);
                        window.retain(|&s| s != acked_sequence);
                    }
                }
            }
            state.send_window.insert(addr, window);
        }
        state
    }

    pub fn check_retransmissions(self, now: Instant) -> (Vec<ReliablePacket<T>>, Self) {
        let mut state = self;
        let mut retransmit = Vec::new();
        let mut to_remove = Vec::new();

        for packet in state.sent_packets.values() {
            if now.duration_since(packet.sent_time) > RETRANSMIT_TIMEOUT {
                retransmit.push(ReliablePacket {
                    packet: packet.packet.clone(),
                    sent_time: packet.sent_time,
                    sequence: packet.sequence,
                    addr: packet.addr,
                });
                warn!("Retransmitting packet: sequence {} to {}", packet.sequence, packet.addr);
                to_remove.push((packet.addr, packet.sequence));
            }
        }

        for key in to_remove {
            state.sent_packets.remove(&key);
        }

        (retransmit, state)
    }
}