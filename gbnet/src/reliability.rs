// reliability.rs - Reliable packet delivery system
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Tracks sent packets for reliability and acknowledgment
#[derive(Debug)]
pub struct ReliableEndpoint {
    /// Sequence number for the next packet to send
    local_sequence: u16,
    /// Last received remote sequence number
    remote_sequence: u16,
    /// Bitfield of acknowledged packets (relative to remote_sequence)
    ack_bits: u32,
    
    /// Sent packets awaiting acknowledgment
    sent_packets: HashMap<u16, SentPacketData>,
    /// Received packets for duplicate detection
    received_packets: SequenceBuffer<bool>,
    
    /// Configuration
    max_sequence_distance: u16,
    retry_timeout: Duration,
    max_retries: u32,
}

#[derive(Debug, Clone)]
struct SentPacketData {
    send_time: Instant,
    retry_count: u32,
    data: Vec<u8>,
}

impl ReliableEndpoint {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            local_sequence: 0,
            remote_sequence: 0,
            ack_bits: 0,
            sent_packets: HashMap::new(),
            received_packets: SequenceBuffer::new(buffer_size),
            max_sequence_distance: 32768,
            retry_timeout: Duration::from_millis(100),
            max_retries: 10,
        }
    }
    
    /// Gets the next sequence number to use for outgoing packets
    pub fn next_sequence(&mut self) -> u16 {
        let seq = self.local_sequence;
        self.local_sequence = self.local_sequence.wrapping_add(1);
        seq
    }
    
    /// Records a packet as sent for reliability tracking
    pub fn on_packet_sent(&mut self, sequence: u16, send_time: Instant, data: Vec<u8>) {
        self.sent_packets.insert(sequence, SentPacketData {
            send_time,
            retry_count: 0,
            data,
        });
    }
    
    /// Processes an incoming packet and updates ack information
    pub fn on_packet_received(&mut self, sequence: u16, _receive_time: Instant) {
        // Check if sequence is too far from what we expect (max_sequence_distance)
        let distance = sequence_diff(sequence, self.remote_sequence).abs() as u16;
        if distance > self.max_sequence_distance {
            // Sequence too far out of range, ignore it
            return;
        }
        
        // Check if this is a new packet (not a duplicate)
        if !self.received_packets.exists(sequence) {
            self.received_packets.insert(sequence, true);
            
            // Update remote sequence if this is newer
            if sequence_greater_than(sequence, self.remote_sequence) {
                // Update ack bits for the gap
                let diff = sequence_diff(sequence, self.remote_sequence) as u32;
                if diff <= 32 {
                    self.ack_bits = (self.ack_bits << diff) | 1;
                } else {
                    self.ack_bits = 1;
                }
                self.remote_sequence = sequence;
            } else {
                // This is an older packet, set the appropriate bit
                let diff = sequence_diff(self.remote_sequence, sequence) as u32;
                if diff > 0 && diff <= 32 {
                    self.ack_bits |= 1 << (diff - 1);
                }
            }
        }
    }
    
    /// Processes acknowledgments from the remote endpoint
    pub fn process_acks(&mut self, ack: u16, ack_bits: u32) {
        // Acknowledge the main sequence
        self.sent_packets.remove(&ack);
        
        // Process ack bits
        for i in 0..32 {
            if (ack_bits & (1 << i)) != 0 {
                let acked_seq = ack.wrapping_sub(i + 1);
                self.sent_packets.remove(&acked_seq);
            }
        }
    }
    
    /// Updates the reliability system, retrying timed-out packets
    pub fn update(&mut self, current_time: Instant) -> Vec<(u16, Vec<u8>)> {
        let mut packets_to_resend = Vec::new();
        let mut packets_to_remove = Vec::new();
        
        for (&sequence, packet_data) in &mut self.sent_packets {
            let elapsed = current_time.duration_since(packet_data.send_time);
            if elapsed >= self.retry_timeout {
                if packet_data.retry_count >= self.max_retries {
                    // Packet failed after max retries
                    packets_to_remove.push(sequence);
                } else {
                    // Retry the packet
                    packet_data.retry_count += 1;
                    packet_data.send_time = current_time;
                    packets_to_resend.push((sequence, packet_data.data.clone()));
                }
            }
        }
        
        // Remove failed packets
        for sequence in packets_to_remove {
            self.sent_packets.remove(&sequence);
        }
        
        packets_to_resend
    }
    
    /// Gets current ack information to include in outgoing packets
    pub fn get_ack_info(&self) -> (u16, u32) {
        (self.remote_sequence, self.ack_bits)
    }
    
    /// Gets statistics about the reliability system
    pub fn stats(&self) -> ReliabilityStats {
        ReliabilityStats {
            packets_in_flight: self.sent_packets.len(),
            local_sequence: self.local_sequence,
            remote_sequence: self.remote_sequence,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReliabilityStats {
    pub packets_in_flight: usize,
    pub local_sequence: u16,
    pub remote_sequence: u16,
}

/// A circular buffer for tracking sequence numbers
#[derive(Debug)]
pub struct SequenceBuffer<T> {
    entries: Vec<Option<T>>,
    sequence: u16,
    size: usize,
}

impl<T> SequenceBuffer<T> {
    pub fn new(size: usize) -> Self {
        let mut entries = Vec::with_capacity(size);
        for _ in 0..size {
            entries.push(None);
        }
        
        Self {
            entries,
            sequence: 0,
            size,
        }
    }
    
    pub fn insert(&mut self, sequence: u16, data: T) {
        if sequence_greater_than(sequence, self.sequence) {
            // Advance the buffer
            let diff = sequence_diff(sequence, self.sequence) as usize;
            if diff < self.size {
                // Shift existing entries
                for _ in 0..diff {
                    self.sequence = self.sequence.wrapping_add(1);
                    let index = self.sequence as usize % self.size;
                    self.entries[index] = None;
                }
            } else {
                // Clear all entries
                for entry in &mut self.entries {
                    *entry = None;
                }
                self.sequence = sequence;
            }
        }
        
        let index = sequence as usize % self.size;
        self.entries[index] = Some(data);
    }
    
    pub fn exists(&self, sequence: u16) -> bool {
        let index = sequence as usize % self.size;
        self.entries[index].is_some()
    }
    
    pub fn get(&self, sequence: u16) -> Option<&T> {
        let index = sequence as usize % self.size;
        self.entries[index].as_ref()
    }
}

// Utility functions (these should match the ones in packet.rs)
fn sequence_greater_than(s1: u16, s2: u16) -> bool {
    ((s1 > s2) && (s1 - s2 <= 32768)) || ((s1 < s2) && (s2 - s1 > 32768))
}

fn sequence_diff(s1: u16, s2: u16) -> i32 {
    let diff = s1 as i32 - s2 as i32;
    if diff > 32768 {
        diff - 65536
    } else if diff < -32768 {
        diff + 65536
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_buffer() {
        let mut buffer = SequenceBuffer::new(32);
        
        buffer.insert(0, true);
        buffer.insert(1, true);
        buffer.insert(2, true);
        
        assert!(buffer.exists(0));
        assert!(buffer.exists(1));
        assert!(buffer.exists(2));
        assert!(!buffer.exists(3));
    }
    
    #[test]
    fn test_reliable_endpoint() {
        let mut endpoint = ReliableEndpoint::new(256);
        let now = Instant::now();
        
        // Send some packets
        let seq1 = endpoint.next_sequence();
        let seq2 = endpoint.next_sequence();
        endpoint.on_packet_sent(seq1, now, vec![1, 2, 3]);
        endpoint.on_packet_sent(seq2, now, vec![4, 5, 6]);
        
        // Simulate receiving acks
        endpoint.process_acks(seq1, 0);
        
        let stats = endpoint.stats();
        assert_eq!(stats.packets_in_flight, 1); // Only seq2 should remain
    }
}