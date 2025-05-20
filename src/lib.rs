use log::{info, trace, warn};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use rand::RngCore;
use tokio::net::UdpSocket;
use thiserror::Error;

mod channel;
mod connection;
mod packet;
mod reliability;
mod serialize;
mod interpolation;
mod lockstep;
mod physics;
mod timestep;
mod congestion;
mod netsim;

use channel::{Channel, ChannelId, ChannelType};
use connection::Connection;
use packet::{Packet, PacketHeader, PacketType};
use reliability::Reliability;
use serialize::{BitReader, BitWriter, Serialize};
use interpolation::Interpolator;
use lockstep::Lockstep;
use physics::PhysicsState;
use timestep::FixedTimestep;
use netsim::NetworkSimulator;

// Custom error types for the library
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Timeout error")]
    Timeout,
    #[error("Invalid channel ID: {0}")]
    InvalidChannel(ChannelId),
}

// Initializes logging for the library
pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .init();
    info!("GBNet library initialized");
}

// Client-side networking implementation
pub struct UdpClient<T: Serialize + Clone + std::fmt::Debug> {
    socket: UdpSocket,
    connections: HashMap<SocketAddr, (Connection, Reliability<T>)>,
    channels: HashMap<ChannelId, Channel<T>>,
    interpolator: Interpolator<T>,
    lockstep: Lockstep<T>,
    physics: PhysicsState<T>,
    timestep: FixedTimestep,
    net_sim: NetworkSimulator,
    connection_id: u32,
    next_sequence: u16,
}

impl<T: Serialize + Clone + std::fmt::Debug> UdpClient<T> {
    pub async fn new(local_addr: &str, initial_state: T) -> Result<Self, Error> {
        trace!("Creating UdpClient on {}", local_addr);
        let socket = UdpSocket::bind(local_addr).await?;
        info!("Client bound to {}", local_addr);
        let mut channels = HashMap::new();
        channels.insert(0, Channel::new(0, ChannelType::Reliable));
        channels.insert(1, Channel::new(1, ChannelType::Unreliable));
        channels.insert(2, Channel::new(2, ChannelType::Snapshot));
        Ok(UdpClient {
            socket,
            connections: HashMap::new(),
            channels,
            interpolator: Interpolator::new(10),
            lockstep: Lockstep::new(10),
            physics: PhysicsState::new(initial_state),
            timestep: FixedTimestep::new(Duration::from_secs_f32(1.0 / 60.0)),
            net_sim: NetworkSimulator::new(),
            connection_id: rand::thread_rng().next_u32(),
            next_sequence: 0,
        })
    }

    pub async fn connect(&mut self, addr: SocketAddr) -> Result<(), Error> {
        let sequence = self.next_sequence();
        let packet = Packet::new_connect_request(sequence, self.connection_id);
        self.send_packet(addr, packet).await?;
        let (response, _) = self.receive_packet().await?;
        if let PacketType::ConnectAccept = response.packet_type {
            self.connections.insert(addr, (Connection::new(addr), Reliability::new()));
            let (connection, _reliability) = self.connections.get_mut(&addr).unwrap();
            connection.on_receive(response.header.sequence, response.header.ack, response.header.ack_bits, Instant::now());
            Ok(())
        } else {
            Err(Error::Io(io::Error::new(io::ErrorKind::Other, "Connection failed")))
        }
    }

    pub async fn send(&mut self, addr: SocketAddr, channel_id: ChannelId, packet: Packet<T>) -> Result<(), Error> {
        let (connection, reliability) = self.connections.entry(addr).or_insert((Connection::new(addr), Reliability::new()));
        let packet = packet.with_connection_id(connection.connection_id);
        trace!("Preparing to send packet to {} on channel {}: sequence {}", addr, channel_id, packet.header.sequence);

        let channel = self.channels.get(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} for send to {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let (packet, new_reliability) = channel.prepare_packet(packet, addr, reliability.clone());
        let buf = {
            let writer = BitWriter::new();
            packet.serialize(writer)?.into_bytes()
        };
        trace!("Sending buffer to {}: {:02x?}", addr, buf);
        self.net_sim.send(&mut self.socket, addr, &buf).await?;
        info!("Sent packet to {} on channel {}: sequence {}", addr, channel_id, packet.header.sequence);

        let now = Instant::now();
        connection.on_send(packet.header.sequence, now);
        *reliability = channel.on_packet_sent(packet.clone(), now, addr, new_reliability);

        if let PacketType::Snapshot { data, timestamp } = packet.packet_type {
            self.interpolator.add_state(data, timestamp);
        } else if let PacketType::Input(data) = packet.packet_type {
            self.lockstep.add_input(data, packet.header.sequence, now.elapsed().as_millis() as u32);
        }

        Ok(())
    }

    pub async fn send_keep_alive(&mut self, addr: SocketAddr, channel_id: ChannelId) -> Result<(), Error> {
        trace!("Checking keep-alive for {} on channel {}", addr, channel_id);
        let now = Instant::now();
        let sequence = self.next_sequence(); // Get sequence before mutable borrow
        if let Some((connection, _reliability)) = self.connections.get_mut(&addr) {
            if connection.should_send_keep_alive(now) {
                let packet = Packet::new_keep_alive(sequence, channel_id, connection.connection_id);
                trace!("Sending keep-alive packet: sequence {}", packet.header.sequence);
                self.send(addr, channel_id, packet).await?;
            }
        }
        Ok(())
    }

    pub async fn receive(&mut self, now: Instant) -> Result<(Packet<T>, SocketAddr, ChannelId), Error> {
        let (packet, addr) = self.receive_packet().await?;
        let channel_id = packet.header.channel_id;

        let channel = self.channels.get(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} from {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let entry = self.connections.entry(addr).or_insert((Connection::new(addr), Reliability::new()));
        entry.0.on_receive(
            packet.header.sequence,
            packet.header.ack,
            packet.header.ack_bits,
            now,
        );

        let (delivered_packet, new_reliability) = if packet.header.ack != 0 {
            trace!("Acknowledging packet with ack {}", packet.header.ack);
            let reliability = channel.on_packet_acked(packet.header.ack, addr, entry.1.clone());
            channel.on_packet_received(packet, addr, reliability)
        } else {
            channel.on_packet_received(packet, addr, entry.1.clone())
        };
        entry.1 = new_reliability;

        if let Some(delivered_packet) = delivered_packet {
            if let PacketType::Snapshot { data, timestamp } = &delivered_packet.packet_type {
                self.interpolator.add_state(data.clone(), *timestamp);
            } else if let PacketType::Input(data) = &delivered_packet.packet_type {
                self.lockstep.add_input(data.clone(), delivered_packet.header.sequence, now.elapsed().as_millis() as u32);
            }
            trace!("Delivered packet: sequence {}", delivered_packet.header.sequence);
            Ok((delivered_packet, addr, channel_id))
        } else {
            trace!("No packet delivered, retrying receive");
            Box::pin(self.receive(now)).await
        }
    }

    pub fn process_lockstep(&mut self, current_time: u32) -> Option<T> {
        self.lockstep.process_inputs(current_time)
    }

    pub fn step_physics(&mut self, state: T, dt: f32) -> T {
        self.physics.step(state, dt)
    }

    pub fn update_timestep(&mut self, now: Instant) -> bool {
        self.timestep.update(now)
    }

    async fn send_packet(&mut self, addr: SocketAddr, packet: Packet<T>) -> Result<(), Error> {
        let buf = {
            let writer = BitWriter::new();
            packet.serialize(writer)?.into_bytes()
        };
        self.net_sim.send(&mut self.socket, addr, &buf).await?;
        Ok(())
    }

    async fn receive_packet(&mut self) -> Result<(Packet<T>, SocketAddr), Error> {
        trace!("Waiting to receive packet");
        let (buf, addr) = self.net_sim.receive(&mut self.socket).await?;
        trace!("Received {} bytes from {}: {:02x?}", buf.len(), addr, &buf[..]);
        let reader = BitReader::new(buf);
        let (packet, _) = Packet::deserialize(reader).map_err(|e| {
            warn!("Failed to deserialize packet from {}: {:?}", addr, e);
            Error::Io(e)
        })?;
        Ok((packet, addr))
    }

    pub async fn check_retransmissions(&mut self, now: Instant) -> Result<(), Error> {
        trace!("Checking retransmissions");
        let mut retransmit_tasks = Vec::new();
        for (channel_id, channel) in self.channels.iter() {
            for addr in self.connections.keys().copied().collect::<Vec<_>>() {
                if let Some((_, reliability)) = self.connections.get_mut(&addr) {
                    let (retransmit_packets, new_reliability) = channel.check_retransmissions(now, reliability.clone());
                    *reliability = new_reliability;
                    for packet in retransmit_packets {
                        let addr = packet.addr;
                        let buf = {
                            let writer = BitWriter::new();
                            packet.packet.serialize(writer)?.into_bytes()
                        };
                        retransmit_tasks.push((addr, buf, *channel_id, packet.sequence));
                    }
                }
            }
        }

        for (addr, buf, channel_id, sequence) in retransmit_tasks {
            trace!("Retransmitting buffer to {}: {:02x?}", addr, buf);
            self.net_sim.send(&mut self.socket, addr, &buf).await?;
            info!("Retransmitted packet to {} on channel {}: sequence {}", addr, channel_id, sequence);
        }
        Ok(())
    }

    pub fn cleanup_connections(&mut self, now: Instant) {
        trace!("Cleaning up connections");
        self.connections.retain(|addr, (connection, _)| {
            if connection.is_timed_out(now) {
                warn!("Connection to {} timed out", addr);
                connection.disconnect();
                false
            } else {
                true
            }
        });
    }

    fn next_sequence(&mut self) -> u16 {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        seq
    }
}

// Server-side networking implementation
pub struct UdpServer<T: Serialize + Clone + std::fmt::Debug> {
    socket: UdpSocket,
    connections: HashMap<SocketAddr, (Connection, Reliability<T>)>,
    channels: HashMap<ChannelId, Channel<T>>,
    interpolator: Interpolator<T>,
    lockstep: Lockstep<T>,
    physics: PhysicsState<T>,
    timestep: FixedTimestep,
    net_sim: NetworkSimulator,
    next_sequence: u16,
}

impl<T: Serialize + Clone + std::fmt::Debug> UdpServer<T> {
    pub async fn new(addr: &str, initial_state: T) -> Result<Self, Error> {
        trace!("Creating UdpServer on {}", addr);
        let socket = UdpSocket::bind(addr).await?;
        info!("Server bound to {}", addr);
        let mut channels = HashMap::new();
        channels.insert(0, Channel::new(0, ChannelType::Reliable));
        channels.insert(1, Channel::new(1, ChannelType::Unreliable));
        channels.insert(2, Channel::new(2, ChannelType::Snapshot));
        Ok(UdpServer {
            socket,
            connections: HashMap::new(),
            channels,
            interpolator: Interpolator::new(10),
            lockstep: Lockstep::new(10),
            physics: PhysicsState::new(initial_state),
            timestep: FixedTimestep::new(Duration::from_secs_f32(1.0 / 60.0)),
            net_sim: NetworkSimulator::new(),
            next_sequence: 0,
        })
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        trace!("Starting UdpServer run loop");
        loop {
            let now = Instant::now();
            trace!("Attempting to receive packet");
            match self.receive_packet().await {
                Ok((packet, addr)) => {
                    let channel_id = packet.header.channel_id;
                    trace!("Received packet from {} on channel {}: sequence {}", addr, channel_id, packet.header.sequence);
                    let channel = self.channels.get(&channel_id).ok_or_else(|| {
                        warn!("Invalid channel ID {} from {}", channel_id, addr);
                        Error::InvalidChannel(channel_id)
                    })?;
                    let (connection, reliability) = self.connections.entry(addr).or_insert((Connection::new(addr), Reliability::new()));

                    match packet.packet_type {
                        PacketType::ConnectRequest => {
                            let sequence = packet.header.sequence.wrapping_add(1);
                            let response = Packet::new_connect_accept(sequence, connection.connection_id);
                            self.send_packet(addr, response).await?;
                            {
                                let (connection, _) = self.connections.entry(addr).or_insert((Connection::new(addr), Reliability::new()));
                                connection.on_receive(packet.header.sequence, packet.header.ack, packet.header.ack_bits, now);
                            }
                            continue;
                        }
                        PacketType::Disconnect => {
                            connection.disconnect();
                            self.connections.remove(&addr);
                            continue;
                        }
                        _ => {}
                    }

                    connection.on_receive(packet.header.sequence, packet.header.ack, packet.header.ack_bits, now);
                    let (delivered_packet, new_reliability) = if packet.header.ack != 0 {
                        trace!("Acknowledging packet with ack {}", packet.header.ack);
                        let reliability = channel.on_packet_acked(packet.header.ack, addr, reliability.clone());
                        channel.on_packet_received(packet, addr, reliability)
                    } else {
                        channel.on_packet_received(packet, addr, reliability.clone())
                    };
                    *reliability = new_reliability;

                    if let Some(delivered_packet) = delivered_packet {
                        let response_packet = Packet {
                            header: PacketHeader {
                                sequence: delivered_packet.header.sequence,
                                ack: delivered_packet.header.sequence,
                                ack_bits: 0,
                                channel_id,
                                connection_id: connection.connection_id as u16,
                            },
                            packet_type: delivered_packet.packet_type.clone(),
                        };
                        self.send(addr, channel_id, response_packet).await?;
                        if let PacketType::Snapshot { data, timestamp } = &delivered_packet.packet_type {
                            self.interpolator.add_state(data.clone(), *timestamp);
                        } else if let PacketType::Input(data) = &delivered_packet.packet_type {
                            self.lockstep.add_input(data.clone(), delivered_packet.header.sequence, now.elapsed().as_millis() as u32);
                        }
                    }
                    self.send_keep_alive(addr, channel_id).await?;
                    self.check_retransmissions(now).await?;
                    self.cleanup_connections(now);
                }
                Err(e) => {
                    warn!("Receive failed: {:?}", e);
                    trace!("Sleeping 100ms before retrying receive");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    pub async fn send(&mut self, addr: SocketAddr, channel_id: ChannelId, packet: Packet<T>) -> Result<(), Error> {
        let (connection, reliability) = self.connections.entry(addr).or_insert((Connection::new(addr), Reliability::new()));
        let packet = packet.with_connection_id(connection.connection_id);
        trace!("Preparing to send packet to {} on channel {}: sequence {}", addr, channel_id, packet.header.sequence);

        let channel = self.channels.get(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} for send to {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let (packet, new_reliability) = channel.prepare_packet(packet, addr, reliability.clone());
        let buf = {
            let writer = BitWriter::new();
            packet.serialize(writer)?.into_bytes()
        };
        trace!("Sending buffer to {}: {:02x?}", addr, buf);
        self.net_sim.send(&mut self.socket, addr, &buf).await?;
        info!("Sent packet to {} on channel {}: sequence {}", addr, channel_id, packet.header.sequence);

        let now = Instant::now();
        connection.on_send(packet.header.sequence, now);
        *reliability = channel.on_packet_sent(packet.clone(), now, addr, new_reliability);

        if let PacketType::Snapshot { data, timestamp } = packet.packet_type {
            self.interpolator.add_state(data, timestamp);
        } else if let PacketType::Input(data) = packet.packet_type {
            self.lockstep.add_input(data, packet.header.sequence, now.elapsed().as_millis() as u32);
        }

        Ok(())
    }

    async fn send_keep_alive(&mut self, addr: SocketAddr, channel_id: ChannelId) -> Result<(), Error> {
        trace!("Checking keep-alive for {} on channel {}", addr, channel_id);
        let now = Instant::now();
        let sequence = self.next_sequence();
        if let Some((connection, _reliability)) = self.connections.get_mut(&addr) {
            if connection.should_send_keep_alive(now) {
                let packet = Packet::new_keep_alive(sequence, channel_id, connection.connection_id);
                trace!("Sending keep-alive packet: sequence {}", packet.header.sequence);
                self.send(addr, channel_id, packet).await?;
            }
        }
        Ok(())
    }

    async fn receive_packet(&mut self) -> Result<(Packet<T>, SocketAddr), Error> {
        trace!("Waiting to receive packet");
        let (buf, addr) = self.net_sim.receive(&mut self.socket).await?;
        trace!("Received {} bytes from {}: {:02x?}", buf.len(), addr, &buf[..]);
        let reader = BitReader::new(buf);
        let (packet, _) = Packet::deserialize(reader).map_err(|e| {
            warn!("Failed to deserialize packet from {}: {:?}", addr, e);
            Error::Io(e)
        })?;
        Ok((packet, addr))
    }

    async fn send_packet(&mut self, addr: SocketAddr, packet: Packet<T>) -> Result<(), Error> {
        let buf = {
            let writer = BitWriter::new();
            packet.serialize(writer)?.into_bytes()
        };
        self.net_sim.send(&mut self.socket, addr, &buf).await?;
        Ok(())
    }

    async fn check_retransmissions(&mut self, now: Instant) -> Result<(), Error> {
        trace!("Checking retransmissions");
        let mut retransmit_tasks = Vec::new();
        for (channel_id, channel) in self.channels.iter() {
            for addr in self.connections.keys().copied().collect::<Vec<_>>() {
                if let Some((_, reliability)) = self.connections.get_mut(&addr) {
                    let (retransmit_packets, new_reliability) = channel.check_retransmissions(now, reliability.clone());
                    *reliability = new_reliability;
                    for packet in retransmit_packets {
                        let addr = packet.addr;
                        let buf = {
                            let writer = BitWriter::new();
                            packet.packet.serialize(writer)?.into_bytes()
                        };
                        retransmit_tasks.push((addr, buf, *channel_id, packet.sequence));
                    }
                }
            }
        }

        for (addr, buf, channel_id, sequence) in retransmit_tasks {
            trace!("Retransmitting buffer to {}: {:02x?}", addr, buf);
            self.net_sim.send(&mut self.socket, addr, &buf).await?;
            info!("Retransmitted packet to {} on channel {}: sequence {}", addr, channel_id, sequence);
        }
        Ok(())
    }

    pub fn cleanup_connections(&mut self, now: Instant) {
        trace!("Cleaning up connections");
        self.connections.retain(|addr, (connection, _)| {
            if connection.is_timed_out(now) {
                warn!("Connection to {} timed out", addr);
                connection.disconnect();
                false
            } else {
                true
            }
        });
    }

    fn next_sequence(&mut self) -> u16 {
        let seq = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::warn;
    use connection::ConnectionState;
    use tokio::time::{sleep, Duration};

    #[derive(Debug, Clone, PartialEq)]
    struct TestData {
        value: u32,
    }

    impl Serialize for TestData {
        fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
            writer.write_bits(self.value as u64, 32)
        }

        fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
            let (value, reader) = reader.read_bits(32)?;
            Ok((TestData { value: value as u32 }, reader))
        }
    }

    #[tokio::test]
    async fn test_client_server_reliability() {
        init();

        let mut server = UdpServer::new("127.0.0.1:8080", TestData { value: 0 }).await.unwrap();
        let server_addr = server.socket.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            if let Err(e) = server.run().await {
                warn!("Server error: {:?}", e);
            }
        });

        let mut client = UdpClient::new("127.0.0.1:8081", TestData { value: 0 }).await.unwrap(); // Fixed IP typo
        client.connect(server_addr).await.unwrap();

        let now = Instant::now();

        // Test Reliable channel (ordered)
        let mut sent_sequences = Vec::new();
        for i in 1..=5 {
            let data = TestData { value: i as u32 };
            let packet = Packet::new_data(i, 0, data, true, client.connection_id);
            client.send(server_addr, 0, packet).await.unwrap();
            sent_sequences.push(i);
            sleep(Duration::from_millis(10)).await;
        }

        let mut received_sequences = Vec::new();
        for expected_seq in &sent_sequences {
            let expected_data = TestData { value: *expected_seq as u32 };
            match client.receive(now).await {
                Ok((received_packet, _, channel_id)) => {
                    if let PacketType::Data { data, ordered } = received_packet.packet_type {
                        let sequence = received_packet.header.sequence;
                        assert_eq!(channel_id, 0, "Expected channel 0");
                        assert_eq!(sequence, *expected_seq, "Received wrong sequence");
                        assert_eq!(data.value, expected_data.value, "Data mismatch");
                        assert!(ordered, "Expected ordered delivery");
                        received_sequences.push(sequence);
                    } else {
                        warn!("Received non-data packet: {:?}", received_packet);
                        server_handle.abort();
                        panic!("Expected data packet");
                    }
                }
                Err(e) => {
                    warn!("Receive failed: {:?}", e);
                    server_handle.abort();
                    panic!("Receive failed: {:?}", e);
                }
            }
        }
        assert_eq!(received_sequences, sent_sequences, "Out-of-order delivery");

        // Test Reliable channel (unordered)
        let mut unordered_data = Vec::new();
        for i in 6..=8 {
            let data = TestData { value: i as u32 };
            let packet = Packet::new_data(i, 0, data.clone(), false, client.connection_id);
            client.send(server_addr, 0, packet).await.unwrap();
            unordered_data.push((i, data));
            sleep(Duration::from_millis(10)).await;
        }

        let mut received_unordered = Vec::new();
        for _ in 0..3 {
            match client.receive(now).await {
                Ok((received_packet, _, channel_id)) => {
                    if let PacketType::Data { data, ordered } = received_packet.packet_type {
                        let sequence = received_packet.header.sequence;
                        assert_eq!(channel_id, 0, "Expected channel 0");
                        assert!(!ordered, "Expected unordered delivery");
                        received_unordered.push((sequence, data));
                    }
                }
                Err(e) => {
                    warn!("Receive failed: {:?}", e);
                    server_handle.abort();
                    panic!("Receive failed: {:?}", e);
                }
            }
        }

        for (seq, data) in unordered_data {
            assert!(received_unordered.iter().any(|(s, d)| *s == seq && d.value == data.value), 
                    "Unordered packet sequence {} missing", seq);
        }

        // Test Unreliable channel (sequenced)
        for i in 10..=12 {
            let packet = Packet::new_data(i, 1, TestData { value: i as u32 }, false, client.connection_id);
            client.send(server_addr, 1, packet).await.unwrap();
            sleep(Duration::from_millis(10)).await;
        }

        match client.receive(now).await {
            Ok((received_packet, _, channel_id)) => {
                if let PacketType::Data { data, ordered } = received_packet.packet_type {
                    assert_eq!(channel_id, 1, "Expected channel 1");
                    assert_eq!(received_packet.header.sequence, 12, "Expected latest sequence");
                    assert_eq!(data.value, 12, "Data mismatch");
                    assert!(!ordered, "Expected unordered delivery");
                }
            }
            Err(e) => {
                warn!("Receive failed: {:?}", e);
                server_handle.abort();
                panic!("Receive failed: {:?}", e);
            }
        }

        // Test Snapshot channel
        let timestamp = 1000;
        let state_data = TestData { value: 100 };
        let packet = Packet::new_snapshot(13, 2, state_data.clone(), timestamp, 1, client.connection_id);
        client.send(server_addr, 2, packet).await.unwrap();
        sleep(Duration::from_millis(10)).await;

        match client.receive(now).await {
            Ok((received_packet, _, channel_id)) => {
                if let PacketType::Snapshot { data, timestamp: recv_timestamp } = received_packet.packet_type {
                    assert_eq!(channel_id, 2, "Expected channel 2");
                    assert_eq!(data.value, state_data.value, "Snapshot data mismatch");
                    assert_eq!(recv_timestamp, timestamp, "Timestamp mismatch");
                } else {
                    warn!("Received wrong packet type: {:?}", received_packet);
                    server_handle.abort();
                    panic!("Expected snapshot packet");
                }
            }
            Err(e) => {
                warn!("Receive failed: {:?}", e);
                server_handle.abort();
                panic!("Expected snapshot packet");
            }
        }

        // Test Input for lockstep
        let input_data = TestData { value: 200 };
        let packet = Packet::new_input(14, 0, input_data.clone(), client.connection_id);
        client.send(server_addr, 0, packet).await.unwrap();
        sleep(Duration::from_millis(110)).await;

        let current_time = Instant::now().elapsed().as_millis() as u32;
        if let Some(received_input) = client.process_lockstep(current_time) {
            assert_eq!(received_input.value, input_data.value, "Lockstep input data mismatch");
        } else {
            warn!("No lockstep input processed");
            server_handle.abort();
            panic!("Expected lockstep input");
        }

        client.cleanup_connections(now);
        assert!(client.connections.contains_key(&server_addr), "Connection dropped unexpectedly");
        assert_eq!(client.connections[&server_addr].0.state, ConnectionState::Connected, "Expected connected state");

        server_handle.abort();
    }
}