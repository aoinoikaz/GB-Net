// channel.rs - Message channels with reliability and ordering guarantees
use std::collections::{VecDeque, HashMap};
use crate::config::{ChannelConfig, Reliability, Ordering};

#[derive(Debug)]
pub enum ChannelError {
    BufferFull,
    MessageTooLarge,
    InvalidSequence,
}

#[derive(Debug)]
pub struct Channel {
    id: u8,
    config: ChannelConfig,
    
    // Send state
    send_sequence: u16,
    send_buffer: VecDeque<ChannelMessage>,
    
    // Receive state
    receive_sequence: u16,
    receive_buffer: HashMap<u16, ChannelMessage>,
    ordered_buffer: VecDeque<Vec<u8>>,
    
    // Stats
    messages_sent: u64,
    messages_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

#[derive(Debug, Clone)]
struct ChannelMessage {
    sequence: u16,
    data: Vec<u8>,
    reliable: bool,
    retry_count: u32,
}

impl Channel {
    pub fn new(id: u8, config: ChannelConfig) -> Self {
        Self {
            id,
            config,
            send_sequence: 0,
            send_buffer: VecDeque::new(),
            receive_sequence: 0,
            receive_buffer: HashMap::new(),
            ordered_buffer: VecDeque::new(),
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
    
    /// Sends data on this channel
    pub fn send(&mut self, data: &[u8], reliable: bool) -> Result<(), ChannelError> {
        if data.len() > self.config.max_message_size {
            return Err(ChannelError::MessageTooLarge);
        }
        
        if self.send_buffer.len() >= self.config.message_buffer_size {
            if self.config.block_on_full {
                return Err(ChannelError::BufferFull);
            } else {
                // Drop oldest message
                self.send_buffer.pop_front();
            }
        }
        
        let message = ChannelMessage {
            sequence: self.send_sequence,
            data: data.to_vec(),
            reliable,
            retry_count: 0,
        };
        
        self.send_sequence = self.send_sequence.wrapping_add(1);
        self.send_buffer.push_back(message);
        self.messages_sent += 1;
        self.bytes_sent += data.len() as u64;
        
        Ok(())
    }
    
    /// Gets the next message to send over the network
    pub fn get_outgoing_message(&mut self) -> Option<Vec<u8>> {
        if let Some(message) = self.send_buffer.front() {
            // For now, just return the data directly
            // In a full implementation, you'd serialize the message with sequence numbers
            Some(message.data.clone())
        } else {
            None
        }
    }
    
    /// Processes an incoming packet for this channel
    pub fn on_packet_received(&mut self, data: Vec<u8>) {
        // For simplicity, we'll assume the data is the message directly
        // In a full implementation, you'd deserialize sequence numbers and handle ordering
        
        match self.config.ordering {
            Ordering::Unordered => {
                // Deliver immediately
                self.ordered_buffer.push_back(data);
                self.messages_received += 1;
                self.bytes_received += self.ordered_buffer.back().unwrap().len() as u64;
            }
            Ordering::Ordered => {
                // For now, just deliver in order received
                // In a full implementation, you'd buffer out-of-order messages
                self.ordered_buffer.push_back(data);
                self.messages_received += 1;
                self.bytes_received += self.ordered_buffer.back().unwrap().len() as u64;
            }
            Ordering::Sequenced => {
                // Only deliver if newer than last received
                // For now, just deliver all messages
                self.ordered_buffer.push_back(data);
                self.messages_received += 1;
                self.bytes_received += self.ordered_buffer.back().unwrap().len() as u64;
            }
        }
    }
    
    /// Receives the next available message
    pub fn receive(&mut self) -> Option<Vec<u8>> {
        self.ordered_buffer.pop_front()
    }
    
    /// Acknowledges a sent message (for reliable delivery)
    pub fn acknowledge_message(&mut self, sequence: u16) {
        if let Some(front) = self.send_buffer.front() {
            if front.sequence == sequence {
                self.send_buffer.pop_front();
            }
        }
    }
    
    /// Updates the channel, handling retries and timeouts
    pub fn update(&mut self) {
        // Handle reliable message retries
        if self.config.reliability == Reliability::Reliable {
            // In a full implementation, you'd check for timed-out messages and retry them
            for message in &mut self.send_buffer {
                if message.reliable && message.retry_count < 5 {
                    // Mark for retry (simplified)
                    message.retry_count += 1;
                }
            }
        }
    }
    
    /// Resets the channel state
    pub fn reset(&mut self) {
        self.send_sequence = 0;
        self.receive_sequence = 0;
        self.send_buffer.clear();
        self.receive_buffer.clear();
        self.ordered_buffer.clear();
    }
    
    /// Returns whether this channel uses reliable delivery
    pub fn is_reliable(&self) -> bool {
        self.config.reliability == Reliability::Reliable
    }
    
    /// Returns channel statistics
    pub fn stats(&self) -> ChannelStats {
        ChannelStats {
            id: self.id,
            messages_sent: self.messages_sent,
            messages_received: self.messages_received,
            bytes_sent: self.bytes_sent,
            bytes_received: self.bytes_received,
            send_buffer_size: self.send_buffer.len(),
            receive_buffer_size: self.receive_buffer.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub id: u8,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_buffer_size: usize,
    pub receive_buffer_size: usize,
}

// Re-export types from config for convenience
pub use crate::config::{ChannelConfig, Reliability, Ordering};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_send_receive() {
        let config = ChannelConfig::default();
        let mut channel = Channel::new(0, config);
        
        // Send a message
        let data = b"Hello, World!";
        channel.send(data, true).unwrap();
        
        // Simulate receiving the message back
        channel.on_packet_received(data.to_vec());
        
        // Receive the message
        let received = channel.receive().unwrap();
        assert_eq!(received, data);
    }
    
    #[test]
    fn test_channel_buffer_limits() {
        let config = ChannelConfig {
            message_buffer_size: 2,
            block_on_full: true,
            ..Default::default()
        };
        let mut channel = Channel::new(0, config);
        
        // Fill the buffer
        channel.send(b"msg1", false).unwrap();
        channel.send(b"msg2", false).unwrap();
        
        // This should fail
        assert!(matches!(channel.send(b"msg3", false), Err(ChannelError::BufferFull)));
    }
}