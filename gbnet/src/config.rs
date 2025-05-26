// config.rs - Network configuration constants and structures
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    // Protocol
    pub protocol_id: u32,
    pub max_clients: usize,
    
    // Timing
    pub connection_timeout: Duration,
    pub keepalive_interval: Duration,
    pub connection_request_timeout: Duration,
    pub connection_request_max_retries: u32,
    
    // Packet settings
    pub mtu: usize,
    pub fragment_threshold: usize,
    pub fragment_timeout: Duration,
    pub max_fragments: usize,
    
    // Reliability
    pub packet_buffer_size: usize,
    pub ack_buffer_size: usize,
    pub max_sequence_distance: u16,
    pub reliable_retry_time: Duration,
    pub max_reliable_retries: u32,
    
    // Channels
    pub max_channels: usize,
    pub default_channel_config: ChannelConfig,
    
    // Rate limiting
    pub send_rate: f32,
    pub max_packet_rate: f32,
    pub congestion_threshold: f32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            protocol_id: 0x12345678, // Change this for your game
            max_clients: 64,
            
            connection_timeout: Duration::from_secs(10),
            keepalive_interval: Duration::from_secs(1),
            connection_request_timeout: Duration::from_secs(5),
            connection_request_max_retries: 5,
            
            mtu: 1200,
            fragment_threshold: 1024,
            fragment_timeout: Duration::from_secs(5),
            max_fragments: 256,
            
            packet_buffer_size: 256,
            ack_buffer_size: 256,
            max_sequence_distance: 32768,
            reliable_retry_time: Duration::from_millis(100),
            max_reliable_retries: 10,
            
            max_channels: 8,
            default_channel_config: ChannelConfig::default(),
            
            send_rate: 60.0, // 60 packets per second
            max_packet_rate: 120.0,
            congestion_threshold: 0.1, // 10% packet loss
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelConfig {
    pub reliability: Reliability,
    pub ordering: Ordering,
    pub max_message_size: usize,
    pub message_buffer_size: usize,
    pub block_on_full: bool,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            reliability: Reliability::Reliable,
            ordering: Ordering::Ordered,
            max_message_size: 1024 * 1024, // 1MB
            message_buffer_size: 1024,
            block_on_full: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reliability {
    Unreliable,
    Reliable,
    UnreliableOrdered,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ordering {
    Unordered,
    Ordered,
    Sequenced,
}