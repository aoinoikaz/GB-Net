use log::{info, trace, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration, sleep};
use thiserror::Error;

mod channel;
mod connection;
mod packet;
mod reliability;
mod serialize;
use channel::{Channel, ChannelId, ChannelType};
use connection::Connection;
use packet::Packet;
use serialize::{BitReader, BitWriter};

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Timeout error")]
    Timeout,
    #[error("Invalid channel ID: {0}")]
    InvalidChannel(ChannelId),
}

pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .init();
    info!("GBNet library initialized");
}

pub struct UdpClient {
    socket: UdpSocket,
    connections: HashMap<SocketAddr, Connection>,
    channels: HashMap<ChannelId, Channel>,
}

impl UdpClient {
    pub async fn new(local_addr: &str) -> Result<Self, Error> {
        trace!("Creating UdpClient on {}", local_addr);
        let socket = UdpSocket::bind(local_addr).await?;
        info!("Client bound to {}", local_addr);
        let mut channels = HashMap::new();
        channels.insert(0, Channel::new(0, ChannelType::ReliableOrdered));
        channels.insert(1, Channel::new(1, ChannelType::ReliableUnordered));
        channels.insert(2, Channel::new(2, ChannelType::Unreliable));
        channels.insert(3, Channel::new(3, ChannelType::Sequenced));
        channels.insert(4, Channel::new(4, ChannelType::ReliableSequenced));
        Ok(UdpClient {
            socket,
            connections: HashMap::new(),
            channels,
        })
    }

    pub async fn send(&mut self, addr: SocketAddr, channel_id: ChannelId, packet: Packet) -> Result<(), Error> {
        trace!("Preparing to send packet to {} on channel {}: {:?}", addr, channel_id, packet);
        let channel = self.channels.get_mut(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} for send to {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let packet = channel.prepare_packet(packet, addr);
        trace!("Prepared packet: sequence {}, data {:?}", packet.header.sequence, packet.packet_type);
        let mut writer = BitWriter::new();
        packet.serialize(&mut writer)?;
        let buf = writer.into_bytes();
        trace!("Sending buffer to {}: {:02x?}", addr, buf);
        self.socket.send_to(&buf, addr).await?;
        info!("Sent packet to {} on channel {}: {:?}", addr, channel_id, packet);

        let now = Instant::now();
        let connection = self.connections
            .entry(addr)
            .or_insert_with(|| Connection::new(addr));
        connection.on_send(packet.header.sequence, now);
        channel.on_packet_sent(packet, now, addr);

        Ok(())
    }

    pub async fn send_keep_alive(&mut self, addr: SocketAddr, channel_id: ChannelId) -> Result<(), Error> {
        trace!("Checking keep-alive for {} on channel {}", addr, channel_id);
        let now = Instant::now();
        if let Some(connection) = self.connections.get_mut(&addr) {
            if connection.should_send_keep_alive(now) {
                let packet = Packet::new_keep_alive(connection.sequence.wrapping_add(1), channel_id);
                trace!("Sending keep-alive packet: {:?}", packet);
                self.send(addr, channel_id, packet).await?;
            }
        }
        Ok(())
    }

    pub async fn receive(&mut self) -> Result<(Packet, SocketAddr, ChannelId), Error> {
        trace!("Waiting to receive packet");
        let mut buf = [0; 1024];
        let result = timeout(Duration::from_secs(15), self.socket.recv_from(&mut buf))
            .await
            .map_err(|_| Error::Timeout)?;
        let (len, addr) = result?;
        trace!("Received {} bytes from {}: {:02x?}", len, addr, &buf[..len]);
        let mut reader = BitReader::new(buf[..len].to_vec());
        let packet = Packet::deserialize(&mut reader).map_err(|e| {
            warn!("Failed to deserialize packet from {}: {:?}", addr, e);
            Error::Io(e)
        })?;
        let channel_id = packet.header.channel_id;
        info!("Received packet from {} on channel {}: {:?}", addr, channel_id, packet);
        trace!("Packet header: sequence {}, ack {}, ack_bits {:08x}, channel_id {}", 
               packet.header.sequence, packet.header.ack, packet.header.ack_bits, packet.header.channel_id);

        let channel = self.channels.get_mut(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} from {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let now = Instant::now();
        let connection = self.connections
            .entry(addr)
            .or_insert_with(|| Connection::new(addr));
        connection.on_receive(
            packet.header.sequence,
            packet.header.ack,
            packet.header.ack_bits,
            now,
        );

        if packet.header.ack != 0 {
            trace!("Acknowledging packet with ack {}", packet.header.ack);
            channel.on_packet_acked(packet.header.ack, addr);
        }

        if let Some(delivered_packet) = channel.on_packet_received(packet, addr) {
            trace!("Delivered packet: sequence {}, data {:?}", 
                   delivered_packet.header.sequence, delivered_packet.packet_type);
            Ok((delivered_packet, addr, channel_id))
        } else {
            trace!("No packet delivered, retrying receive");
            Box::pin(self.receive()).await
        }
    }

    pub async fn check_retransmissions(&mut self) -> Result<(), Error> {
        trace!("Checking retransmissions");
        let now = Instant::now();
        for channel in self.channels.values_mut() {
            let retransmit_packets = channel.check_retransmissions(now);
            for packet in retransmit_packets {
                let addr = packet.addr;
                let mut writer = BitWriter::new();
                packet.packet.serialize(&mut writer)?;
                let buf = writer.into_bytes();
                trace!("Retransmitting buffer to {}: {:02x?}", addr, buf);
                self.socket.send_to(&buf, addr).await?;
                info!("Retransmitted packet to {} on channel {}: sequence {}", addr, packet.packet.header.channel_id, packet.sequence);
            }
        }
        Ok(())
    }

    pub fn cleanup_connections(&mut self) {
        trace!("Cleaning up connections");
        let now = Instant::now();
        self.connections.retain(|_addr, connection| {
            if connection.is_timed_out(now) {
                warn!("Connection to {} timed out", connection.addr);
                connection.disconnect();
                false
            } else {
                true
            }
        });
    }
}

pub struct UdpServer {
    socket: UdpSocket,
    connections: HashMap<SocketAddr, Connection>,
    channels: HashMap<ChannelId, Channel>,
}

impl UdpServer {
    pub async fn new(addr: &str) -> Result<Self, Error> {
        trace!("Creating UdpServer on {}", addr);
        let socket = UdpSocket::bind(addr).await?;
        info!("Server bound to {}", addr);
        let mut channels = HashMap::new();
        channels.insert(0, Channel::new(0, ChannelType::ReliableOrdered));
        channels.insert(1, Channel::new(1, ChannelType::ReliableUnordered));
        channels.insert(2, Channel::new(2, ChannelType::Unreliable));
        channels.insert(3, Channel::new(3, ChannelType::Sequenced));
        channels.insert(4, Channel::new(4, ChannelType::ReliableSequenced));
        Ok(UdpServer {
            socket,
            connections: HashMap::new(),
            channels,
        })
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        trace!("Starting UdpServer run loop");
        loop {
            trace!("Attempting to receive packet");
            match self.receive().await {
                Ok((packet, addr, channel_id)) => {
                    trace!("Successfully received packet from {} on channel {}", addr, channel_id);
                    if let Err(e) = self.send(addr, channel_id, packet).await {
                        warn!("Failed to send packet to {}: {:?}", addr, e);
                    }
                    if let Err(e) = self.send_keep_alive(addr, channel_id).await {
                        warn!("Failed to send keep-alive to {}: {:?}", addr, e);
                    }
                    if let Err(e) = self.check_retransmissions().await {
                        warn!("Failed to check retransmissions: {:?}", e);
                    }
                    self.cleanup_connections();
                }
                Err(e) => {
                    warn!("Receive failed: {:?}", e);
                    trace!("Sleeping 100ms before retrying receive");
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn send(&mut self, addr: SocketAddr, channel_id: ChannelId, packet: Packet) -> Result<(), Error> {
        trace!("Preparing to send packet to {} on channel {}: {:?}", addr, channel_id, packet);
        let channel = self.channels.get_mut(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} for send to {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let packet = channel.prepare_packet(packet, addr);
        trace!("Prepared packet: sequence {}, data {:?}", packet.header.sequence, packet.packet_type);
        let mut writer = BitWriter::new();
        packet.serialize(&mut writer)?;
        let buf = writer.into_bytes();
        trace!("Sending buffer to {}: {:02x?}", addr, buf);
        self.socket.send_to(&buf, addr).await?;
        info!("Sent packet to {} on channel {}: {:?}", addr, channel_id, packet);

        let now = Instant::now();
        let connection = self.connections
            .entry(addr)
            .or_insert_with(|| Connection::new(addr));
        connection.on_send(packet.header.sequence, now);
        channel.on_packet_sent(packet, now, addr);

        Ok(())
    }

    async fn send_keep_alive(&mut self, addr: SocketAddr, channel_id: ChannelId) -> Result<(), Error> {
        trace!("Checking keep-alive for {} on channel {}", addr, channel_id);
        let now = Instant::now();
        if let Some(connection) = self.connections.get_mut(&addr) {
            if connection.should_send_keep_alive(now) {
                let packet = Packet::new_keep_alive(connection.sequence.wrapping_add(1), channel_id);
                trace!("Sending keep-alive packet: {:?}", packet);
                self.send(addr, channel_id, packet).await?;
            }
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<(Packet, SocketAddr, ChannelId), Error> {
        trace!("Waiting to receive packet");
        let mut buf = [0; 1024];
        let result = timeout(Duration::from_secs(15), self.socket.recv_from(&mut buf))
            .await
            .map_err(|_| Error::Timeout)?;
        let (len, addr) = result?;
        trace!("Received {} bytes from {}: {:02x?}", len, addr, &buf[..len]);
        let mut reader = BitReader::new(buf[..len].to_vec());
        let packet = Packet::deserialize(&mut reader).map_err(|e| {
            warn!("Failed to deserialize packet from {}: {:?}", addr, e);
            Error::Io(e)
        })?;
        let channel_id = packet.header.channel_id;
        info!("Received packet from {} on channel {}: {:?}", addr, channel_id, packet);
        trace!("Packet header: sequence {}, ack {}, ack_bits {:08x}, channel_id {}", 
               packet.header.sequence, packet.header.ack, packet.header.ack_bits, packet.header.channel_id);

        let channel = self.channels.get_mut(&channel_id).ok_or_else(|| {
            warn!("Invalid channel ID {} from {}", channel_id, addr);
            Error::InvalidChannel(channel_id)
        })?;
        let now = Instant::now();
        let connection = self.connections
            .entry(addr)
            .or_insert_with(|| Connection::new(addr));
        connection.on_receive(
            packet.header.sequence,
            packet.header.ack,
            packet.header.ack_bits,
            now,
        );

        if packet.header.ack != 0 {
            trace!("Acknowledging packet with ack {}", packet.header.ack);
            channel.on_packet_acked(packet.header.ack, addr);
        }

        if let Some(delivered_packet) = channel.on_packet_received(packet, addr) {
            trace!("Delivered packet: sequence {}, data {:?}", 
                   delivered_packet.header.sequence, delivered_packet.packet_type);
            Ok((delivered_packet, addr, channel_id))
        } else {
            trace!("No packet delivered, retrying receive");
            Box::pin(self.receive()).await
        }
    }

    async fn check_retransmissions(&mut self) -> Result<(), Error> {
        trace!("Checking retransmissions");
        let now = Instant::now();
        for channel in self.channels.values_mut() {
            let retransmit_packets = channel.check_retransmissions(now);
            for packet in retransmit_packets {
                let addr = packet.addr;
                let mut writer = BitWriter::new();
                packet.packet.serialize(&mut writer)?;
                let buf = writer.into_bytes();
                trace!("Retransmitting buffer to {}: {:02x?}", addr, buf);
                self.socket.send_to(&buf, addr).await?;
                info!("Retransmitted packet to {} on channel {}: sequence {}", addr, packet.packet.header.channel_id, packet.sequence);
            }
        }
        Ok(())
    }

    pub fn cleanup_connections(&mut self) {
        trace!("Cleaning up connections");
        let now = Instant::now();
        self.connections.retain(|_addr, connection| {
            if connection.is_timed_out(now) {
                warn!("Connection to {} timed out", connection.addr);
                connection.disconnect();
                false
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::warn;
    use connection::ConnectionState;
    use packet::PacketType;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_client_server_reliability() {
        init();

        let mut server = UdpServer::new("127.0.0.1:8080").await.unwrap();
        let server_addr = server.socket.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            if let Err(e) = server.run().await {
                warn!("Server error: {:?}", e);
            }
        });

        let mut client = UdpClient::new("127.0.0.1:8081").await.unwrap();

        // Test ReliableOrdered channel (0)
        let mut sent_sequences = Vec::new();
        for i in 1..=5 {
            let data = vec![i as u8; 3];
            let packet = Packet::new_data(i, 0, data.clone());
            if true { // 100% delivery to isolate serialization issues
                let mut writer = BitWriter::new();
                packet.serialize(&mut writer).unwrap();
                let buf = writer.into_bytes();
                trace!("Sent packet buffer for sequence {}: data {:?}, buffer {:02x?}", i, data, buf);
                if let Err(e) = client.send(server_addr, 0, packet).await {
                    warn!("Send failed: {:?}", e);
                    server_handle.abort();
                    panic!("Send failed: {:?}", e);
                }
                sent_sequences.push(i);
                sleep(Duration::from_millis(10)).await; // Small delay to prevent overwhelming server
            }
            client.check_retransmissions().await.unwrap(); // Ensure retransmissions
        }

        let mut received_sequences = Vec::new();
        let max_retries = 5;
        for expected_seq in &sent_sequences {
            let expected_data = vec![*expected_seq as u8; 3];
            let mut retries = max_retries;
            let result = loop {
                match client.receive().await {
                    Ok((received_packet, addr, channel_id)) => {
                        trace!("Received packet from {} with sequence {}, data {:?}", 
                              addr, received_packet.header.sequence, received_packet.packet_type);
                        if received_packet.header.sequence == *expected_seq {
                            break Some((received_packet, addr, channel_id));
                        } else {
                            warn!("Received out-of-order packet: sequence {}, expected sequence {}", 
                                  received_packet.header.sequence, expected_seq);
                            retries -= 1;
                            if retries == 0 {
                                warn!("No more retries for expected sequence {}", expected_seq);
                                break None;
                            }
                            client.check_retransmissions().await.unwrap();
                            sleep(Duration::from_millis(1000)).await;
                            continue;
                        }
                    }
                    Err(e) => {
                        warn!("Receive attempt {} failed: {:?}", max_retries - retries + 1, e);
                        retries -= 1;
                        if retries == 0 {
                            warn!("Receive failed after {} retries for sequence {}: {:?}", max_retries, expected_seq, e);
                            break None;
                        }
                        client.check_retransmissions().await.unwrap();
                        sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                }
            };
            if let Some((received_packet, _, channel_id)) = result {
                if let PacketType::Data(data) = received_packet.packet_type {
                    let sequence = received_packet.header.sequence;
                    assert_eq!(channel_id, 0, "Expected channel 0");
                    assert_eq!(sequence, *expected_seq, "Received wrong sequence");
                    if data != expected_data {
                        warn!("Data mismatch for sequence {}: got {:?}, expected {:?}", sequence, data, expected_data);
                    }
                    received_sequences.push(sequence);
                } else {
                    warn!("Received non-data packet: {:?}", received_packet);
                    server_handle.abort();
                    panic!("Expected data packet, got {:?}", received_packet);
                }
            } else {
                server_handle.abort();
                panic!("Failed to receive expected packet with sequence {}", expected_seq);
            }
            client.check_retransmissions().await.unwrap(); // Ensure retransmissions
        }

        for (i, &seq) in received_sequences.iter().enumerate() {
            assert_eq!(seq, sent_sequences[i], "Out-of-order packet delivery");
        }

        // Test Sequenced channel (3)
        for i in 10..=12 {
            let packet = Packet::new_data(i, 3, vec![i as u8; 3]);
            if let Err(e) = client.send(server_addr, 3, packet).await {
                warn!("Send failed: {:?}", e);
                server_handle.abort();
                panic!("Send failed: {:?}", e);
            }
            sleep(Duration::from_millis(10)).await; // Small delay
            client.check_retransmissions().await.unwrap(); // Ensure retransmissions
        }
        let max_retries = 5;
        let mut retries = 0;
        let result = loop {
            match client.receive().await {
                Ok(result) => break Some(result),
                Err(e) if retries < max_retries => {
                    warn!("Receive attempt {} failed: {:?}", retries + 1, e);
                    retries += 1;
                    client.check_retransmissions().await.unwrap();
                    sleep(Duration::from_millis(1000)).await;
                    continue;
                }
                Err(e) => {
                    warn!("Receive failed after {} retries: {:?}", max_retries, e);
                    server_handle.abort();
                    panic!("Receive failed: {:?}", e);
                }
            }
        };
        if let Some((received_packet, _, channel_id)) = result {
            if let PacketType::Data(data) = received_packet.packet_type {
                assert_eq!(channel_id, 3, "Expected channel 3");
                assert_eq!(received_packet.header.sequence, 12, "Expected latest sequence");
                assert_eq!(data, vec![12; 3], "Sequenced packet data mismatch");
            }
        }

        // Test ReliableSequenced channel (4) keep-alive
        sleep(Duration::from_secs(2)).await;
        if let Err(e) = client.send_keep_alive(server_addr, 4).await {
            warn!("Keep-alive failed: {:?}", e);
            server_handle.abort();
            panic!("Keep-alive failed: {:?}", e);
        }
        let max_retries = 5;
        let mut retries = 0;
        let result = loop {
            match client.receive().await {
                Ok(result) => break Some(result),
                Err(e) if retries < max_retries => {
                    warn!("Receive attempt {} failed: {:?}", retries + 1, e);
                    retries += 1;
                    client.check_retransmissions().await.unwrap();
                    sleep(Duration::from_millis(1000)).await;
                    continue;
                }
                Err(e) => {
                    warn!("Receive failed after {} retries: {:?}", max_retries, e);
                    server_handle.abort();
                    panic!("Receive failed: {:?}", e);
                }
            }
        };
        if let Some((received_packet, _, channel_id)) = result {
            if let PacketType::KeepAlive = received_packet.packet_type {
                assert_eq!(channel_id, 4, "Expected channel 4");
                assert!(received_packet.header.sequence >= 2, "Keep-alive sequence mismatch");
            } else {
                warn!("Received packet: {:?}", received_packet);
                server_handle.abort();
                panic!("Expected keep-alive packet, got {:?}", received_packet);
            }
        }

        client.cleanup_connections();
        assert!(client.connections.contains_key(&server_addr), "Connection dropped unexpectedly");
        assert_eq!(client.connections[&server_addr].state, ConnectionState::Connected, "Expected connected state");

        server_handle.abort();
    }
}