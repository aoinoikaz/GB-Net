use crate::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use log::info;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelType {
    ReliableOrdered,
    ReliableUnordered,
    UnreliableSequenced,
    Unreliable,
    Snapshot,
}

pub type ChannelId = u8;

#[derive(Debug, Clone, Copy)]
pub struct ChannelConfig {
    pub retransmit_timeout: Duration,
    pub max_packet_size: usize,
    pub priority: u8,
    pub bandwidth_limit: Option<u32>,
    pub mtu: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        ChannelConfig {
            retransmit_timeout: Duration::from_millis(200),
            max_packet_size: 1200,
            priority: 0,
            bandwidth_limit: None,
            mtu: 1400,
        }
    }
}

#[cfg(feature = "metrics")]
#[derive(Debug)]
pub struct ChannelMetrics {
    packets_sent: u64,
    packets_received: u64,
    packets_lost: u64,
    packets_retransmitted: u64,
    bytes_sent: u64,
    bytes_received: u64,
    total_latency: Duration,
    latency_samples: u64,
}

#[cfg(feature = "metrics")]
impl ChannelMetrics {
    pub fn new() -> Self {
        ChannelMetrics {
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            packets_retransmitted: 0,
            bytes_sent: 0,
            bytes_received: 0,
            total_latency: Duration::from_secs(0),
            latency_samples: 0,
        }
    }

    pub fn record_sent(&mut self, bytes: usize) {
        self.packets_sent += 1;
        self.bytes_sent += bytes as u64;
    }

    pub fn record_received(&mut self, bytes: usize, latency: Duration) {
        self.packets_received += 1;
        self.bytes_received += bytes as u64;
        self.total_latency += latency;
        self.latency_samples += 1;
    }

    pub fn record_lost(&mut self) {
        self.packets_lost += 1;
    }

    pub fn record_retransmitted(&mut self) {
        self.packets_retransmitted += 1;
    }

    pub fn average_latency(&self) -> Option<Duration> {
        if self.latency_samples > 0 {
            Some(self.total_latency / self.latency_samples as u32)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Channel {
    id: ChannelId,
    channel_type: ChannelType,
    config: ChannelConfig,
    #[cfg(feature = "metrics")]
    metrics: ChannelMetrics,
    bandwidth_usage: AtomicU64,
    last_bandwidth_reset: Instant,
}

impl Channel {
    pub fn new(id: ChannelId, channel_type: ChannelType, config: ChannelConfig) -> Self {
        #[cfg(debug_assertions)]
        info!("Creating channel {}: {:?}", id, channel_type);
        Channel {
            id,
            channel_type,
            config,
            #[cfg(feature = "metrics")]
            metrics: ChannelMetrics::new(),
            bandwidth_usage: AtomicU64::new(0),
            last_bandwidth_reset: Instant::now(),
        }
    }

    pub fn id(&self) -> ChannelId {
        self.id
    }

    pub fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    pub fn config(&self) -> &ChannelConfig {
        &self.config
    }

    #[cfg(feature = "metrics")]
    pub fn metrics(&self) -> &ChannelMetrics {
        &self.metrics
    }

    pub fn check_bandwidth(&self, packet_size: usize, now: Instant) -> Result<(), Error> {
        if now.duration_since(self.last_bandwidth_reset) >= Duration::from_secs(1) {
            self.bandwidth_usage.store(0, Ordering::Relaxed);
            // Note: last_bandwidth_reset is not updated atomically, but it's safe as it's only written here
            // and read elsewhere, with no concurrent writes. We'll use Tokio::Mutex in peer.rs if needed.
            unsafe {
                let last_bandwidth_reset = &mut *(std::ptr::addr_of!(self.last_bandwidth_reset) as *mut Instant);
                *last_bandwidth_reset = now;
            }
        }
        if let Some(limit) = self.config.bandwidth_limit {
            let current = self.bandwidth_usage.load(Ordering::Relaxed);
            if current + packet_size as u64 > limit as u64 {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Bandwidth limit exceeded",
                )));
            }
            self.bandwidth_usage.fetch_add(packet_size as u64, Ordering::Relaxed);
        }
        Ok(())
    }

    #[cfg(feature = "metrics")]
    pub fn record_sent(&mut self, bytes: usize) {
        self.metrics.record_sent(bytes);
    }

    #[cfg(feature = "metrics")]
    pub fn record_received(&mut self, bytes: usize, latency: Duration) {
        self.metrics.record_received(bytes, latency);
    }

    #[cfg(feature = "metrics")]
    pub fn record_lost(&mut self) {
        self.metrics.record_lost();
    }

    #[cfg(feature = "metrics")]
    pub fn record_retransmitted(&mut self) {
        self.metrics.record_retransmitted();
    }
}