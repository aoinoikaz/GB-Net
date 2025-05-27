use gbnet::{
    UdpSocket, Packet, PacketHeader, PacketType,
    Connection, NetworkConfig,
    BitSerialize, BitDeserialize, BitBuffer,
};
use gbnet_macros::NetworkSerialize;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::thread;
use std::time::Duration;

#[derive(NetworkSerialize, Debug, PartialEq)]
struct GamePacket {
    #[bits = 16]
    player_id: u16,
    #[bits = 10]
    x: u16,
    #[bits = 10]
    y: u16,
    #[bits = 8]
    health: u8,
}

#[test]
fn test_full_packet_flow() -> std::io::Result<()> {
    // Create game packet
    let game_data = GamePacket {
        player_id: 12345,
        x: 512,
        y: 768,
        health: 100,
    };
    
    // Serialize game data
    let mut buffer = BitBuffer::new();
    game_data.bit_serialize(&mut buffer)?;
    let payload = buffer.into_bytes(true)?;
    
    // Create network packet
    let header = PacketHeader {
        protocol_id: 0x12345678,
        sequence: 1,
        ack: 0,
        ack_bits: 0,
    };
    
    let packet = Packet::new(header, PacketType::Payload { channel: 0, is_fragment: false })
        .with_payload(payload);
    
    // Serialize network packet
    let packet_data = packet.serialize()?;
    
    // Create sockets
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    
    let mut server = UdpSocket::bind(server_addr)?;
    let mut client = UdpSocket::bind(client_addr)?;
    
    let actual_server_addr = server.local_addr()?;
    
    // Send packet
    client.send_to(&packet_data, actual_server_addr)?;
    
    // Receive packet
    thread::sleep(Duration::from_millis(10));
    let (received_data, _from) = server.recv_from()?;
    
    // Deserialize network packet
    let received_packet = Packet::deserialize(received_data)?;
    
    // Deserialize game data
    let mut payload_buffer = BitBuffer::from_bytes(received_packet.payload);
    let received_game_data = GamePacket::bit_deserialize(&mut payload_buffer)?;
    
    // Verify
    assert_eq!(received_game_data, game_data);
    assert_eq!(received_packet.header.sequence, 1);
    
    Ok(())
}

#[test]
fn test_connection_handshake_simulation() {
    let config = NetworkConfig::default();
    
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    
    let mut server_socket = UdpSocket::bind(server_addr).unwrap();
    let mut client_socket = UdpSocket::bind(client_addr).unwrap();
    
    let actual_server_addr = server_socket.local_addr().unwrap();
    let actual_client_addr = client_socket.local_addr().unwrap();
    
    // Create connections
    let mut client_conn = Connection::new(config.clone(), actual_server_addr, actual_client_addr);
    let mut server_conn = Connection::new(config, actual_server_addr, actual_client_addr);
    
    // Client initiates connection
    client_conn.connect().unwrap();
    
    // In a real scenario, this would involve packet exchange
    // For now, we just verify the state transitions work
    assert!(!client_conn.is_connected());
    assert!(!server_conn.is_connected());
}

#[test]
fn test_multi_channel_messages() -> std::io::Result<()> {
    use gbnet::{Channel, ChannelConfig, Reliability};
    
    // Create channels with different configs
    let reliable_config = ChannelConfig {
        reliability: Reliability::Reliable,
        ..Default::default()
    };
    
    let unreliable_config = ChannelConfig {
        reliability: Reliability::Unreliable,
        ..Default::default()
    };
    
    let mut reliable_channel = Channel::new(0, reliable_config);
    let mut unreliable_channel = Channel::new(1, unreliable_config);
    
    // Send messages
    reliable_channel.send(b"important data", true)?;
    unreliable_channel.send(b"position update", false)?;
    
    // Simulate receiving
    reliable_channel.on_packet_received(b"important data".to_vec());
    unreliable_channel.on_packet_received(b"position update".to_vec());
    
    // Verify
    assert_eq!(reliable_channel.receive().unwrap(), b"important data");
    assert_eq!(unreliable_channel.receive().unwrap(), b"position update");
    
    Ok(())
}