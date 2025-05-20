use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use rand::{Rng, thread_rng};
use log::trace;

// Constants for network simulation
const PACKET_LOSS_RATE: f32 = 0.1; // 10% packet loss probability
const LATENCY_MIN_MS: u32 = 50; // Minimum added latency
const LATENCY_MAX_MS: u32 = 150; // Maximum added latency
const JITTER_MS: u32 = 20; // Jitter range (+/- 20ms)

// Simulates network conditions (loss, latency, jitter)
#[derive(Debug)]
pub struct NetworkSimulator {
    packet_loss_rate: f32,
    latency_min: u32,
    latency_max: u32,
    jitter: u32,
    pending_packets: Vec<(SocketAddr, Vec<u8>, Instant)>,
}

impl NetworkSimulator {
    pub fn new() -> Self {
        NetworkSimulator {
            packet_loss_rate: PACKET_LOSS_RATE,
            latency_min: LATENCY_MIN_MS,
            latency_max: LATENCY_MAX_MS,
            jitter: JITTER_MS,
            pending_packets: Vec::new(),
        }
    }

    // Simulates sending a packet with loss, latency, and jitter
    pub async fn send(&mut self, socket: &mut UdpSocket, addr: SocketAddr, buf: &[u8]) -> Result<(), std::io::Error> {
        if thread_rng().r#gen::<f32>() < self.packet_loss_rate {
            trace!("Dropped packet to {} due to simulated loss", addr);
            return Ok(());
        }

        let latency = thread_rng().r#gen_range(self.latency_min..=self.latency_max);
        let jitter = thread_rng().r#gen_range(0..=self.jitter * 2).saturating_sub(self.jitter);
        let delay_ms = (latency + jitter) as u64;
        let send_time = Instant::now() + Duration::from_millis(delay_ms);

        self.pending_packets.push((addr, buf.to_vec(), send_time));
        trace!("Queued packet to {} with delay {}ms", addr, delay_ms);
        Ok(())
    }

    // Simulates receiving a packet, applying queued delays
    pub async fn receive(&mut self, socket: &mut UdpSocket) -> Result<(Vec<u8>, SocketAddr), std::io::Error> {
        // Process pending sends
        let now = Instant::now();
        let mut i = 0;
        while i < self.pending_packets.len() {
            let (addr, buf, send_time) = &self.pending_packets[i];
            if now >= *send_time {
                socket.send_to(buf, *addr).await?;
                trace!("Sent delayed packet to {}", addr);
                self.pending_packets.swap_remove(i);
            } else {
                i += 1;
            }
        }

        // Receive new packet
        let mut buf = [0; 2048];
        let (len, addr) = socket.recv_from(&mut buf).await?;
        Ok((buf[..len].to_vec(), addr))
    }
}