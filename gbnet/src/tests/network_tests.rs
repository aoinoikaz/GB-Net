#[test]
fn test_reliable_endpoint_tracking() {
    let mut endpoint = ReliableEndpoint::new(256);
    let now = Instant::now();
    
    // Test packet tracking
    endpoint.on_packet_sent(0, now, vec![1, 2, 3]);
    endpoint.on_packet_sent(1, now, vec![4, 5, 6]);
    
    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 2);
    
    // Test acknowledgment
    endpoint.process_acks(0, 0);
    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 1);
}// src/tests/network_tests.rs - Network component unit tests

use crate::{
    socket::UdpSocket,
    packet::{Packet, PacketHeader, PacketType, sequence_greater_than, sequence_diff},
    connection::{Connection, ConnectionError},
    reliability::{ReliableEndpoint, SequenceBuffer},
    channel::{Channel, ChannelError},
    config::{NetworkConfig, ChannelConfig, Reliability, Ordering},
};
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::time::Instant;

#[test]
fn test_socket_basic() {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let socket = UdpSocket::bind(addr).unwrap();
    assert!(socket.local_addr().is_ok());
}

#[test]
fn test_packet_construction() {
    let header = PacketHeader {
        protocol_id: 0x12345678,
        sequence: 100,
        ack: 99,
        ack_bits: 0xFFFFFFFF,
    };
    
    let packet = Packet::new(header.clone(), PacketType::KeepAlive);
    assert_eq!(packet.header.protocol_id, 0x12345678);
    assert_eq!(packet.header.sequence, 100);
    assert!(packet.payload.is_empty());
}

#[test]
fn test_sequence_math() {
    // Basic increment
    assert!(sequence_greater_than(1, 0));
    assert!(!sequence_greater_than(0, 1));
    
    // Wraparound
    assert!(sequence_greater_than(0, 65535));
    assert!(!sequence_greater_than(65535, 0));
    
    // Difference
    assert_eq!(sequence_diff(5, 3), 2);
    assert_eq!(sequence_diff(3, 5), -2);
    assert_eq!(sequence_diff(0, 65535), 1);
}

#[test]
fn test_channel_send_receive() {
    let config = ChannelConfig::default();
    let mut channel = Channel::new(0, config);
    
    let data = b"test message";
    channel.send(data, false).unwrap();
    
    // Simulate receiving
    channel.on_packet_received(data.to_vec());
    
    let received = channel.receive().unwrap();
    assert_eq!(received, data);
}

#[test]
fn test_channel_buffer_full() {
    let config = ChannelConfig {
        message_buffer_size: 2,
        block_on_full: true,
        ..Default::default()
    };
    
    let mut channel = Channel::new(0, config);
    
    assert!(channel.send(b"msg1", false).is_ok());
    assert!(channel.send(b"msg2", false).is_ok());
    assert!(matches!(
        channel.send(b"msg3", false),
        Err(ChannelError::BufferFull)
    ));
}

#[test]
fn test_reliable_endpoint_sequences() {
    let mut endpoint = ReliableEndpoint::new(256);
    
    assert_eq!(endpoint.next_sequence(), 0);
    assert_eq!(endpoint.next_sequence(), 1);
    assert_eq!(endpoint.next_sequence(), 2);
    
    // Test that sequences continue incrementing
    for i in 3..10 {
        assert_eq!(endpoint.next_sequence(), i);
    }
}

#[test]
fn test_reliable_endpoint_tracking() {
    let mut endpoint = ReliableEndpoint::new(256);
    let now = Instant::now();
    
    // Test packet tracking
    endpoint.on_packet_sent(0, now, vec![1, 2, 3]);
    endpoint.on_packet_sent(1, now, vec![4, 5, 6]);
    
    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 2);
    
    // Test acknowledgment
    endpoint.process_acks(0, 0);
    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 1);
}

#[test]
fn test_sequence_buffer_operations() {
    let mut buffer: SequenceBuffer<u32> = SequenceBuffer::new(16);
    
    buffer.insert(0, 100);
    buffer.insert(1, 200);
    buffer.insert(2, 300);
    
    assert!(buffer.exists(0));
    assert!(buffer.exists(1));
    assert!(buffer.exists(2));
    assert!(!buffer.exists(3));
    
    assert_eq!(*buffer.get(0).unwrap(), 100);
    assert_eq!(*buffer.get(1).unwrap(), 200);
    assert_eq!(*buffer.get(2).unwrap(), 300);
}

#[test]
fn test_connection_states() {
    let config = NetworkConfig::default();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
    
    let mut conn = Connection::new(config, local, remote);
    
    assert!(!conn.is_connected());
    assert_eq!(conn.local_addr(), local);
    assert_eq!(conn.remote_addr(), remote);
    
    // Should be able to connect once
    assert!(conn.connect().is_ok());
    
    // Should fail on second connect
    assert!(matches!(
        conn.connect(),
        Err(ConnectionError::AlreadyConnected)
    ));
}

#[test]
fn test_config_defaults() {
    let config = NetworkConfig::default();
    
    assert_eq!(config.protocol_id, 0x12345678);
    assert_eq!(config.max_clients, 64);
    assert_eq!(config.mtu, 1200);
    assert_eq!(config.max_channels, 8);
    
    let channel_config = config.default_channel_config;
    assert_eq!(channel_config.reliability, Reliability::Reliable);
    assert_eq!(channel_config.ordering, Ordering::Ordered);
}