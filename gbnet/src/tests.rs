// tests.rs - All tests for gbnet

// These need to be inside the crate, not as standalone declarations
use crate::serialize::bit_io::BitBuffer;
use crate::serialize::{BitDeserialize, BitSerialize};
use gbnet_macros::NetworkSerialize;

// Test structures
#[derive(NetworkSerialize, Debug, PartialEq)]
#[default_bits(u16 = 10, bool = 1)]
#[default_max_len = 16]
pub struct NetworkMessage {
    #[bits = 10]
    message_id: u16,
    #[bits = 8]
    priority: u8,
    is_urgent: bool,
    #[max_len = 4]
    players: Vec<PlayerInfo>,
    message_type: MessageType,
    #[byte_align]
    game_state: GameState,
}

#[derive(NetworkSerialize, Debug, PartialEq)]
#[bits = 2]
pub enum MessageType {
    StatusUpdate,
    Command { #[bits = 8] code: u8 },
    Alert { #[bits = 4] level: u8 },
    Sync,
}

#[derive(NetworkSerialize, Default, Debug, PartialEq)]
pub struct GameState {
    #[bits = 10]
    round: u16,
    #[bits = 8]
    score: u8,
    is_paused: bool,
}

#[derive(NetworkSerialize, Default, Debug, PartialEq)]
pub struct PlayerInfo {
    #[bits = 6]
    health: u8,                       
    #[bits = 4]
    energy: u8,                       
    is_active: bool,                  
    nickname: Option<u8>,
}

#[derive(NetworkSerialize, Debug, PartialEq)]
#[default_max_len = 32]
pub struct ExtendedMessage {
    #[max_len = 16]
    player_name: String,
    coordinates: [f32; 3],
    tags: Vec<String>,
    metadata: (u8, bool, u16),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        socket::{UdpSocket, SocketError},
        packet::{Packet, PacketHeader, PacketType, sequence_greater_than, sequence_diff},
        connection::{Connection, ConnectionState, ConnectionError},
        reliability::{ReliableEndpoint, SequenceBuffer},
        channel::{Channel, ChannelError},
        config::{NetworkConfig, ChannelConfig, Reliability, Ordering},
    };
    use std::net::{SocketAddr, IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};
    use std::thread;

    #[allow(dead_code)]
    fn init_logger() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .try_init();
    }

    // ========== SERIALIZATION TESTS ==========

    #[test]
    fn test_network_message_serialization() -> std::io::Result<()> {
        init_logger();
        let packet = NetworkMessage {
            message_id: 500,
            priority: 3,
            is_urgent: true,
            players: vec![
                PlayerInfo {
                    health: 50,
                    energy: 10,
                    is_active: true,
                    nickname: Some(42)
                },
                PlayerInfo {
                    health: 30,
                    energy: 5,
                    is_active: false,
                    nickname: None
                },
            ],
            message_type: MessageType::Command { code: 42 },
            game_state: GameState {
                round: 100,
                score: 255,
                is_paused: false,
            },
        };
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = NetworkMessage::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized.message_id, packet.message_id);
        assert_eq!(deserialized.priority, packet.priority);
        assert_eq!(deserialized.is_urgent, packet.is_urgent);
        assert_eq!(deserialized.players.len(), packet.players.len());
        assert_eq!(deserialized.game_state, packet.game_state);
        Ok(())
    }

    #[test]
    fn test_string_serialization() -> std::io::Result<()> {
        init_logger();
        let test_string = "Hello, Network! 🚀".to_string();
        let mut bit_buffer = BitBuffer::new();
        test_string.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = String::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_string);
        Ok(())
    }

    #[test]
    fn test_array_serialization() -> std::io::Result<()> {
        init_logger();
        let test_array: [u8; 4] = [1, 2, 3, 4];
        let mut bit_buffer = BitBuffer::new();
        test_array.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <[u8; 4]>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_array);
        Ok(())
    }

    #[test]
    fn test_tuple_serialization() -> std::io::Result<()> {
        init_logger();
        let test_tuple = (42u8, true, 1337u16);
        let mut bit_buffer = BitBuffer::new();
        test_tuple.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <(u8, bool, u16)>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_tuple);
        Ok(())
    }

    #[test]
    fn test_float_serialization() -> std::io::Result<()> {
        init_logger();
        let test_float = 10.5f32;
        let mut bit_buffer = BitBuffer::new();
        test_float.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = f32::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_float);
        Ok(())
    }

    #[test]
    fn test_extended_message_with_new_types() -> std::io::Result<()> {
        init_logger();
        let message = ExtendedMessage {
            player_name: "Alice".to_string(),
            coordinates: [10.5, 20.3, 30.7],
            tags: vec!["VIP".to_string(), "Pro".to_string()],
            metadata: (255u8, true, 65535u16),
        };

        let mut bit_buffer = BitBuffer::new();
        message.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = ExtendedMessage::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, message);
        Ok(())
    }

    #[test]
    fn test_complex_nested_structures() -> std::io::Result<()> {
        init_logger();
        
        let nested_data: Vec<Vec<(String, [u8; 2])>> = vec![
            vec![
                ("first".to_string(), [1, 2]),
                ("second".to_string(), [3, 4]),
            ],
            vec![
                ("third".to_string(), [5, 6]),
            ],
        ];

        let mut bit_buffer = BitBuffer::new();
        nested_data.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<Vec<(String, [u8; 2])>>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, nested_data);
        Ok(())
    }

    #[test]
    fn test_option_variants() -> std::io::Result<()> {
        init_logger();
        
        let options: Vec<Option<String>> = vec![
            None,
            Some("test".to_string()),
            None,
            Some("another".to_string()),
        ];

        let mut bit_buffer = BitBuffer::new();
        options.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<Option<String>>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, options);
        Ok(())
    }

    #[test]
    fn test_empty_collections() -> std::io::Result<()> {
        init_logger();
        
        let empty_vec: Vec<String> = vec![];
        let mut bit_buffer = BitBuffer::new();
        empty_vec.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<String>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, empty_vec);
        
        let empty_string = String::new();
        let mut bit_buffer = BitBuffer::new();
        empty_string.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = String::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, empty_string);
        Ok(())
    }

    #[test]
    fn test_large_arrays() -> std::io::Result<()> {
        init_logger();
        
        let large_array: [u16; 32] = [0; 32];
        let mut bit_buffer = BitBuffer::new();
        large_array.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <[u16; 32]>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, large_array);
        Ok(())
    }

    // ========== NETWORK TESTS ==========

    #[test]
    fn test_socket_creation_and_binding() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let socket = UdpSocket::bind(addr);
        assert!(socket.is_ok());
        
        let socket = socket.unwrap();
        let local_addr = socket.local_addr();
        assert!(local_addr.is_ok());
    }

    #[test]
    fn test_socket_send_receive() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        
        let mut socket1 = UdpSocket::bind(addr1).unwrap();
        let mut socket2 = UdpSocket::bind(addr2).unwrap();
        
        let socket1_addr = socket1.local_addr().unwrap();
        let socket2_addr = socket2.local_addr().unwrap();
        
        let test_data = b"Hello, Network!";
        let sent = socket1.send_to(test_data, socket2_addr).unwrap();
        assert_eq!(sent, test_data.len());
        
        thread::sleep(Duration::from_millis(10));
        let (received_data, sender_addr) = socket2.recv_from().unwrap();
        assert_eq!(received_data, test_data);
        assert_eq!(sender_addr, socket1_addr);
    }

    #[test]
    fn test_packet_serialization_all_types() {
        let test_cases = vec![
            (PacketType::ConnectionRequest, "ConnectionRequest"),
            (PacketType::ConnectionChallenge { server_salt: 0x123456789ABCDEF0 }, "ConnectionChallenge"),
            (PacketType::ConnectionResponse { client_salt: 0xFEDCBA9876543210 }, "ConnectionResponse"),
            (PacketType::ConnectionAccept, "ConnectionAccept"),
            (PacketType::ConnectionDeny { reason: 5 }, "ConnectionDeny"),
            (PacketType::Disconnect { reason: 2 }, "Disconnect"),
            (PacketType::KeepAlive, "KeepAlive"),
            (PacketType::Payload { channel: 3, is_fragment: true }, "Payload"),
        ];
        
        for (packet_type, name) in test_cases {
            let header = PacketHeader {
                protocol_id: 0x12345678,
                sequence: 1000,
                ack: 999,
                ack_bits: 0xAAAAAAAA,
            };
            
            let packet = Packet::new(header.clone(), packet_type.clone())
                .with_payload(vec![1, 2, 3, 4, 5]);
            
            let serialized = packet.serialize().unwrap();
            let deserialized = Packet::deserialize(&serialized).unwrap();
            
            assert_eq!(packet.header, deserialized.header, "Header mismatch for {}", name);
            assert_eq!(packet.packet_type, deserialized.packet_type, "PacketType mismatch for {}", name);
            assert_eq!(packet.payload, deserialized.payload, "Payload mismatch for {}", name);
        }
    }

    #[test]
    fn test_sequence_number_math() {
        assert!(sequence_greater_than(1, 0));
        assert!(sequence_greater_than(100, 99));
        assert!(!sequence_greater_than(99, 100));
        assert!(sequence_greater_than(0, 65535));
        assert!(!sequence_greater_than(65535, 0));
        
        assert_eq!(sequence_diff(100, 99), 1);
        assert_eq!(sequence_diff(99, 100), -1);
        assert_eq!(sequence_diff(0, 65535), 1);
        assert_eq!(sequence_diff(65535, 0), -1);
    }

    #[test]
    fn test_channel_basic_operations() {
        let config = ChannelConfig {
            reliability: Reliability::Reliable,
            ordering: Ordering::Ordered,
            max_message_size: 1024,
            message_buffer_size: 10,
            block_on_full: true,
        };
        
        let mut channel = Channel::new(0, config);
        
        let data1 = b"Message 1";
        let data2 = b"Message 2";
        
        assert!(channel.send(data1, true).is_ok());
        assert!(channel.send(data2, false).is_ok());
        
        channel.on_packet_received(data1.to_vec());
        channel.on_packet_received(data2.to_vec());
        
        let received1 = channel.receive().unwrap();
        let received2 = channel.receive().unwrap();
        assert_eq!(received1, data1);
        assert_eq!(received2, data2);
        assert!(channel.receive().is_none());
    }

    #[test]
    fn test_channel_buffer_limits() {
        let config = ChannelConfig {
            message_buffer_size: 3,
            block_on_full: true,
            ..Default::default()
        };
        
        let mut channel = Channel::new(0, config);
        
        assert!(channel.send(b"msg1", false).is_ok());
        assert!(channel.send(b"msg2", false).is_ok());
        assert!(channel.send(b"msg3", false).is_ok());
        
        let result = channel.send(b"msg4", false);
        assert!(matches!(result, Err(ChannelError::BufferFull)));
    }

    #[test]
    fn test_channel_message_too_large() {
        let config = ChannelConfig {
            max_message_size: 10,
            ..Default::default()
        };
        
        let mut channel = Channel::new(0, config);
        
        let small_msg = b"small";
        let large_msg = b"this message is too large";
        
        assert!(channel.send(small_msg, false).is_ok());
        assert!(matches!(
            channel.send(large_msg, false),
            Err(ChannelError::MessageTooLarge)
        ));
    }

    #[test]
    fn test_reliable_endpoint_basic() {
        let mut endpoint = ReliableEndpoint::new(256);
        
        assert_eq!(endpoint.next_sequence(), 0);
        assert_eq!(endpoint.next_sequence(), 1);
        assert_eq!(endpoint.next_sequence(), 2);
        
        let now = Instant::now();
        endpoint.on_packet_sent(0, now, vec![1, 2, 3]);
        endpoint.on_packet_sent(1, now, vec![4, 5, 6]);
        
        let stats = endpoint.stats();
        assert_eq!(stats.packets_in_flight, 2);
        
        endpoint.process_acks(0, 0);
        let stats = endpoint.stats();
        assert_eq!(stats.packets_in_flight, 1);
    }

    #[test]
    fn test_reliable_endpoint_ack_bits() {
        let mut endpoint = ReliableEndpoint::new(256);
        let now = Instant::now();
        
        endpoint.on_packet_received(0, now);
        endpoint.on_packet_received(1, now);
        endpoint.on_packet_received(2, now);
        endpoint.on_packet_received(4, now);
        
        let (ack, ack_bits) = endpoint.get_ack_info();
        assert_eq!(ack, 4);
        assert_eq!(ack_bits & 0b1011, 0b1011);
    }

    #[test]
    fn test_reliable_endpoint_retransmission() {
        let mut endpoint = ReliableEndpoint::new(256);
        let now = Instant::now();
        
        endpoint.on_packet_sent(0, now, vec![1, 2, 3]);
        
        let retries = endpoint.update(now);
        assert_eq!(retries.len(), 0);
        
        let later = now + Duration::from_millis(150);
        let retries = endpoint.update(later);
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].0, 0);
        assert_eq!(retries[0].1, vec![1, 2, 3]);
    }

    #[test]
    fn test_sequence_buffer() {
        let mut buffer: SequenceBuffer<String> = SequenceBuffer::new(32);
        
        buffer.insert(0, "zero".to_string());
        buffer.insert(1, "one".to_string());
        buffer.insert(2, "two".to_string());
        
        assert!(buffer.exists(0));
        assert!(buffer.exists(1));
        assert!(buffer.exists(2));
        assert!(!buffer.exists(3));
        
        assert_eq!(buffer.get(0).unwrap(), "zero");
        assert_eq!(buffer.get(1).unwrap(), "one");
        assert_eq!(buffer.get(2).unwrap(), "two");
        assert!(buffer.get(3).is_none());
        
        buffer.insert(40, "forty".to_string());
        assert!(!buffer.exists(0));
        assert!(buffer.exists(40));
    }

    #[test]
    fn test_connection_state_machine() {
        let config = NetworkConfig::default();
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
        
        let mut connection = Connection::new(config, local_addr, remote_addr);
        
        assert_eq!(connection.is_connected(), false);
        assert_eq!(connection.local_addr(), local_addr);
        assert_eq!(connection.remote_addr(), remote_addr);
        
        let result = connection.connect();
        assert!(result.is_ok());
        
        let result = connection.connect();
        assert!(matches!(result, Err(ConnectionError::AlreadyConnected)));
    }

    #[test]
    fn test_connection_channels() {
        let config = NetworkConfig {
            max_channels: 4,
            ..Default::default()
        };
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
        
        let mut connection = Connection::new(config, local_addr, remote_addr);
        
        let result = connection.send(0, b"test", true);
        assert!(matches!(result, Err(ConnectionError::NotConnected)));
        
        connection.connect().unwrap();
        
        assert!(connection.receive(0).is_none());
        assert!(connection.receive(3).is_none());
        assert!(connection.receive(4).is_none());
    }

    #[test]
    fn test_network_config_defaults() {
        let config = NetworkConfig::default();
        
        assert_eq!(config.protocol_id, 0x12345678);
        assert_eq!(config.max_clients, 64);
        assert_eq!(config.mtu, 1200);
        assert_eq!(config.max_channels, 8);
        assert_eq!(config.send_rate, 60.0);
        
        let channel_config = config.default_channel_config;
        assert_eq!(channel_config.reliability, Reliability::Reliable);
        assert_eq!(channel_config.ordering, Ordering::Ordered);
        assert_eq!(channel_config.max_message_size, 1024 * 1024);
    }

    #[test]
    fn test_packet_header_sizes() {
        let header = PacketHeader {
            protocol_id: 0xFFFFFFFF,
            sequence: 0xFFFF,
            ack: 0xFFFF,
            ack_bits: 0xFFFFFFFF,
        };
        
        let packet = Packet::new(header, PacketType::KeepAlive);
        let serialized = packet.serialize().unwrap();
        
        assert!(serialized.len() >= 13);
    }

    #[test]
    fn test_integration_packet_roundtrip() {
        let header = PacketHeader {
            protocol_id: 0x12345678,
            sequence: 12345,
            ack: 12344,
            ack_bits: 0b11111111111111111111111111111110,
        };
        
        let packet_type = PacketType::Payload { 
            channel: 2, 
            is_fragment: false 
        };
        
        let payload = b"This is a test payload with some data!".to_vec();
        let packet = Packet::new(header.clone(), packet_type.clone())
            .with_payload(payload.clone());
        
        let serialized = packet.serialize().unwrap();
        let deserialized = Packet::deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.header.protocol_id, header.protocol_id);
        assert_eq!(deserialized.header.sequence, header.sequence);
        assert_eq!(deserialized.header.ack, header.ack);
        assert_eq!(deserialized.header.ack_bits, header.ack_bits);
        
        match deserialized.packet_type {
            PacketType::Payload { channel, is_fragment } => {
                assert_eq!(channel, 2);
                assert_eq!(is_fragment, false);
            }
            _ => panic!("Wrong packet type"),
        }
        
        assert_eq!(deserialized.payload, payload);
    }

    // ========== BENCHMARKS ==========

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn benchmark_serialization() -> std::io::Result<()> {
        let packet = NetworkMessage {
            message_id: 500,
            priority: 3,
            is_urgent: true,
            players: vec![
                PlayerInfo {
                    health: 50,
                    energy: 10,
                    is_active: true,
                    nickname: Some(42)
                },
            ],
            message_type: MessageType::Command { code: 42 },
            game_state: GameState {
                round: 100,
                score: 255,
                is_paused: false,
            },
        };

        let start = Instant::now();
        for _ in 0..1000 {
            let mut buffer = BitBuffer::new();
            packet.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = NetworkMessage::bit_deserialize(&mut buffer)?;
        }
        
        println!("1000 serialization cycles took: {:?}", start.elapsed());
        Ok(())
    }
}