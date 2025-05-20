use std::collections::VecDeque;
use log::trace;
use super::serialize::Serialize;

// Constants for deterministic lockstep
const LOCKSTEP_DELAY_MS: u32 = 100; // Delay to ensure input synchronization
const MAX_INPUTS: usize = 10; // Maximum stored inputs to prevent memory growth

// Stores a player input with its sequence and timestamp
#[derive(Debug, Clone)]
pub struct InputEntry<T: Serialize> {
    pub data: T,
    pub sequence: u16,
    pub timestamp: u32,
}

// Manages deterministic lockstep input synchronization
#[derive(Debug)]
pub struct Lockstep<T: Serialize> {
    inputs: VecDeque<InputEntry<T>>,
    max_inputs: usize,
    last_processed: u16,
}

impl<T: Serialize + Clone> Lockstep<T> {
    pub fn new(max_inputs: usize) -> Self {
        Lockstep {
            inputs: VecDeque::new(),
            max_inputs: max_inputs.max(1),
            last_processed: 0,
        }
    }

    // Adds a new input to the queue
    pub fn add_input(&mut self, data: T, sequence: u16, timestamp: u32) {
        while self.inputs.len() >= self.max_inputs {
            self.inputs.pop_front();
        }
        self.inputs.push_back(InputEntry { data, sequence, timestamp });
        trace!("Added input with sequence {}", sequence);
    }

    // Processes inputs in lockstep order, respecting delay
    pub fn process_inputs(&mut self, current_time: u32) -> Option<T> {
        let process_time = current_time.saturating_sub(LOCKSTEP_DELAY_MS);
        if let Some(input) = self.inputs.front() {
            if input.timestamp <= process_time && input.sequence == self.last_processed.wrapping_add(1) {
                let input = self.inputs.pop_front().unwrap();
                self.last_processed = input.sequence;
                trace!("Processed input sequence {}", input.sequence);
                return Some(input.data);
            }
        }
        None
    }
}