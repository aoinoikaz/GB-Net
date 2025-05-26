// connection.rs - Connection state management for reliable UDP
use std::net::SocketAddr;
use std::time::Instant;
use std::collections::VecDeque;
use rand::random;

use crate::{
    NetworkConfig, NetworkStats,
    packet::{Packet, PacketHeader, PacketType, disconnect_reason, sequence_greater_than},
    socket::{UdpSocket, SocketError},
    reliability::ReliableEndpoint,
    channel::{Channel, ChannelError},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    ChallengeResponse,
    Connected,
    Disconnecting,
}

#[derive(Debug)]
pub enum ConnectionError {
    NotConnected,
    AlreadyConnected,
    ConnectionDenied(u8),
    Timeout,
    ProtocolMismatch,
    InvalidPacket,
    SocketError(SocketError),
    ChannelError(ChannelError),
}

impl From<SocketError> for ConnectionError {
    fn from(err: SocketError) -> Self {
        ConnectionError::SocketError(err)
    }
}

impl From<ChannelError> for ConnectionError {
    fn from(err: ChannelError) -> Self {
        ConnectionError::ChannelError(err)
    }
}

pub struct Connection {
    config: NetworkConfig,
    state: ConnectionState,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    
    // Connection handshake
    client_salt: u64,
    server_salt: u64,
    
    // Timing
    last_packet_send_time: Instant,
    last_packet_recv_time: Instant,
    connection_start_time: Option<Instant>,
    connection_request_time: Option<Instant>,
    connection_retry_count: u32,
    
    // Reliability
    local_sequence: u16,
    remote_sequence: u16,
    ack_bits: u32,
    reliability: ReliableEndpoint,
    
    // Channels
    channels: Vec<Channel>,
    
    // Queues
    send_queue: VecDeque<Packet>,
    recv_queue: VecDeque<Packet>,
    
    // Stats
    stats: NetworkStats,
}

impl Connection {
    /// Creates a new connection with the given configuration and addresses.
    pub fn new(config: NetworkConfig, local_addr: SocketAddr, remote_addr: SocketAddr) -> Self {
        let mut channels = Vec::with_capacity(config.max_channels);
        let channel_config = config.default_channel_config;
        for i in 0..config.max_channels {
            channels.push(Channel::new(i as u8, channel_config));
        }
        
        let packet_buffer_size = config.packet_buffer_size;
        
        Self {
            config,
            state: ConnectionState::Disconnected,
            local_addr,
            remote_addr,
            client_salt: random(),
            server_salt: 0,
            last_packet_send_time: Instant::now(),
            last_packet_recv_time: Instant::now(),
            connection_start_time: None,
            connection_request_time: None,
            connection_retry_count: 0,
            local_sequence: 0,
            remote_sequence: 0,
            ack_bits: 0,
            reliability: ReliableEndpoint::new(packet_buffer_size),
            channels,
            send_queue: VecDeque::new(),
            recv_queue: VecDeque::new(),
            stats: NetworkStats::default(),
        }
    }
    
    /// Initiates a connection by sending a connection request.
    pub fn connect(&mut self) -> Result<(), ConnectionError> {
        if self.state != ConnectionState::Disconnected {
            return Err(ConnectionError::AlreadyConnected);
        }
        
        self.state = ConnectionState::Connecting;
        self.connection_request_time = Some(Instant::now());
        self.connection_retry_count = 0;
        
        // Send connection request
        self.send_connection_request()?;
        
        Ok(())
    }
    
    /// Disconnects the connection with a given reason.
    pub fn disconnect(&mut self, reason: u8) -> Result<(), ConnectionError> {
        if self.state == ConnectionState::Disconnected {
            return Ok(());
        }
        
        // Send disconnect packet
        let header = self.create_header();
        let packet = Packet::new(header, PacketType::Disconnect { reason });
        self.send_queue.push_back(packet);
        
        self.state = ConnectionState::Disconnecting;
        self.reset_connection();
        
        Ok(())
    }
    
    /// Updates the connection state, processes send/receive queues, and handles timeouts.
    pub fn update(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        let now = Instant::now();
        
        // Check for timeout
        if self.state != ConnectionState::Disconnected {
            let time_since_recv = now.duration_since(self.last_packet_recv_time);
            if time_since_recv > self.config.connection_timeout {
                self.disconnect(disconnect_reason::TIMEOUT)?;
                return Err(ConnectionError::Timeout);
            }
        }
        
        // Handle connection state
        match self.state {
            ConnectionState::Connecting => {
                if let Some(request_time) = self.connection_request_time {
                    if now.duration_since(request_time) > self.config.connection_request_timeout {
                        self.connection_retry_count += 1;
                        if self.connection_retry_count > self.config.connection_request_max_retries {
                            self.state = ConnectionState::Disconnected;
                            return Err(ConnectionError::Timeout);
                        }
                        self.send_connection_request()?;
                        self.connection_request_time = Some(now);
                    }
                }
            }
            ConnectionState::Connected => {
                // Send keepalive if needed
                let time_since_send = now.duration_since(self.last_packet_send_time);
                if time_since_send > self.config.keepalive_interval {
                    self.send_keepalive()?;
                }
                
                // Update reliability system
                self.reliability.update(now);
            }
            _ => {}
        }
        
        // Process send queue
        self.process_send_queue(socket)?;
        
        // Receive packets
        self.receive_packets(socket)?;
        
        Ok(())
    }
    
    /// Sends data on a specific channel.
    pub fn send(&mut self, channel_id: u8, data: &[u8], reliable: bool) -> Result<(), ConnectionError> {
        if self.state != ConnectionState::Connected {
            return Err(ConnectionError::NotConnected);
        }
        
        if channel_id as usize >= self.channels.len() {
            return Err(ConnectionError::InvalidPacket);
        }
        
        self.channels[channel_id as usize].send(data, reliable)?;
        Ok(())
    }
    
    /// Receives data from a specific channel.
    pub fn receive(&mut self, channel_id: u8) -> Option<Vec<u8>> {
        if channel_id as usize >= self.channels.len() {
            return None;
        }
        
        self.channels[channel_id as usize].receive()
    }
    
    /// Creates a packet header with current sequence and ack information.
    fn create_header(&self) -> PacketHeader {
        PacketHeader {
            protocol_id: self.config.protocol_id,
            sequence: self.local_sequence,
            ack: self.remote_sequence,
            ack_bits: self.ack_bits,
        }
    }
    
    /// Sends a connection request packet.
    fn send_connection_request(&mut self) -> Result<(), ConnectionError> {
        let header = PacketHeader {
            protocol_id: self.config.protocol_id,
            sequence: 0,
            ack: 0,
            ack_bits: 0,
        };
        
        let packet = Packet::new(header, PacketType::ConnectionRequest);
        self.send_queue.push_back(packet);
        Ok(())
    }
    
    /// Sends a keepalive packet.
    fn send_keepalive(&mut self) -> Result<(), ConnectionError> {
        let header = self.create_header();
        let packet = Packet::new(header, PacketType::KeepAlive);
        self.send_queue.push_back(packet);
        Ok(())
    }
    
    /// Processes the send queue, transmitting packets via the socket.
    fn process_send_queue(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        while let Some(packet) = self.send_queue.pop_front() {
            let data = packet.serialize().map_err(|_| ConnectionError::InvalidPacket)?;
            socket.send_to(&data, self.remote_addr)?;
            
            self.last_packet_send_time = Instant::now();
            self.stats.packets_sent += 1;
            self.stats.bytes_sent += data.len() as u64;
            
            // Track reliable packets
            if let PacketType::Payload { channel, .. } = packet.packet_type {
                if self.channels[channel as usize].is_reliable() {
                    self.reliability.on_packet_sent(packet.header.sequence, Instant::now());
                }
            }
        }
        Ok(())
    }
    
    /// Receives packets from the socket and processes them.
    fn receive_packets(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        loop {
            match socket.recv_from() {
                Ok((data, addr)) => {
                    if addr != self.remote_addr {
                        continue; // Ignore packets from other addresses
                    }
                    
                    let packet = Packet::deserialize(data)
                        .map_err(|_| ConnectionError::InvalidPacket)?;
                    
                    // Validate protocol ID
                    if packet.header.protocol_id != self.config.protocol_id {
                        return Err(ConnectionError::ProtocolMismatch);
                    }
                    
                    self.last_packet_recv_time = Instant::now();
                    self.stats.packets_received += 1;
                    self.stats.bytes_received += data.len() as u64;
                    
                    self.handle_packet(packet)?;
                }
                Err(SocketError::WouldBlock) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    
    /// Handles a received packet based on the current connection state.
    fn handle_packet(&mut self, packet: Packet) -> Result<(), ConnectionError> {
        match (&self.state, &packet.packet_type) {
            (ConnectionState::Connecting, PacketType::ConnectionChallenge { server_salt }) => {
                self.server_salt = *server_salt;
                self.state = ConnectionState::ChallengeResponse;
                
                // Send response
                let header = self.create_header();
                let response = Packet::new(
                    header,
                    PacketType::ConnectionResponse { client_salt: self.client_salt }
                );
                self.send_queue.push_back(response);
            }
            
            (ConnectionState::ChallengeResponse, PacketType::ConnectionAccept) => {
                self.state = ConnectionState::Connected;
                self.connection_start_time = Some(Instant::now());
                self.last_packet_recv_time = Instant::now();
                
                // Reset sequences
                self.local_sequence = 0;
                self.remote_sequence = 0;
            }
            
            (_, PacketType::ConnectionDeny { reason }) => {
                self.state = ConnectionState::Disconnected;
                return Err(ConnectionError::ConnectionDenied(*reason));
            }
            
            (ConnectionState::Connected, _) => {
                // Update reliability tracking
                self.reliability.on_packet_received(packet.header.sequence, Instant::now());
                
                // Update remote sequence and acks
                if sequence_greater_than(packet.header.sequence, self.remote_sequence) {
                    self.remote_sequence = packet.header.sequence;
                }
                
                // Process acks
                self.reliability.process_acks(packet.header.ack, packet.header.ack_bits);
                
                // Handle specific packet types
                match packet.packet_type {
                    PacketType::Payload { channel, .. } => {
                        if (channel as usize) < self.channels.len() {
                            self.channels[channel as usize].on_packet_received(packet.payload);
                        }
                    }
                    PacketType::Disconnect { reason: _ } => {
                        self.state = ConnectionState::Disconnected;
                        self.reset_connection();
                    }
                    _ => {}
                }
            }
            
            _ => {} // Ignore unexpected packets
        }
        
        Ok(())
    }
    
    /// Resets the connection state and clears queues.
    fn reset_connection(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.connection_start_time = None;
        self.connection_request_time = None;
        self.local_sequence = 0;
        self.remote_sequence = 0;
        self.ack_bits = 0;
        self.send_queue.clear();
        self.recv_queue.clear();
        
        for channel in &mut self.channels {
            channel.reset();
        }
    }
    
    /// Checks if the connection is in the Connected state.
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    
    /// Returns the connection statistics.
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }
}