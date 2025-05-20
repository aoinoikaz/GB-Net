use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use log::trace;

// Constants for congestion control
const MIN_SEND_INTERVAL_MS: f32 = 10.0; // Minimum time between packets (ms)
const MAX_SEND_INTERVAL_MS: f32 = 100.0; // Maximum time between packets (ms)
const RTT_THRESHOLD_MS: f32 = 200.0; // RTT above which we increase send interval
const PACKET_LOSS_THRESHOLD: f32 = 0.1; // Packet loss above which we increase send interval
const ADJUSTMENT_FACTOR: f32 = 1.2; // Factor to adjust send interval
const SMOOTHING_FACTOR: f32 = 0.1; // Smoothing factor for send interval updates

// Manages congestion control based on RTT and packet loss
#[derive(Debug)]
pub struct CongestionControl {
    send_intervals: HashMap<SocketAddr, f32>, // Send interval per connection (ms)
    last_send_times: HashMap<SocketAddr, Instant>, // Last send time per connection
}

impl CongestionControl {
    pub fn new() -> Self {
        CongestionControl {
            send_intervals: HashMap::new(),
            last_send_times: HashMap::new(),
        }
    }

    // Checks if a packet can be sent to the address
    pub fn can_send(&self, addr: SocketAddr, now: Instant) -> bool {
        let interval = self.send_intervals.get(&addr).copied().unwrap_or(MIN_SEND_INTERVAL_MS);
        if let Some(last_send) = self.last_send_times.get(&addr) {
            let elapsed_ms = now.duration_since(*last_send).as_secs_f32() * 1000.0;
            elapsed_ms >= interval
        } else {
            true // No previous send, allow immediately
        }
    }

    // Updates state on packet send
    pub fn on_packet_sent(&mut self, addr: SocketAddr, now: Instant) {
        self.last_send_times.insert(addr, now);
        trace!("Packet sent to {}, last send time updated", addr);
    }

    // Updates state on packet receive
    pub fn on_packet_received(&mut self, addr: SocketAddr, _now: Instant) {
        // Placeholder for receive tracking (e.g., for packet loss calculation)
        trace!("Packet received from {}", addr);
    }

    // Updates send interval based on RTT and packet loss
    pub fn update(&mut self, addr: SocketAddr, rtt: f32, packet_loss: f32) {
        let current_interval = self.send_intervals.entry(addr).or_insert(MIN_SEND_INTERVAL_MS);
        let mut new_interval = *current_interval;

        // Increase interval if RTT or packet loss is high
        if rtt > RTT_THRESHOLD_MS || packet_loss > PACKET_LOSS_THRESHOLD {
            new_interval *= ADJUSTMENT_FACTOR;
        } else {
            // Gradually decrease interval if conditions are good
            new_interval /= ADJUSTMENT_FACTOR;
        }

        // Clamp interval to reasonable bounds
        new_interval = new_interval.clamp(MIN_SEND_INTERVAL_MS, MAX_SEND_INTERVAL_MS);

        // Smooth the interval update
        *current_interval = *current_interval * (1.0 - SMOOTHING_FACTOR) + new_interval * SMOOTHING_FACTOR;
        trace!("Updated send interval for {}: {}ms (RTT: {}ms, loss: {})", addr, *current_interval, rtt, packet_loss);
    }
}