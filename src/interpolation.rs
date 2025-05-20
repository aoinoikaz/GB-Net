use std::collections::VecDeque;
use log::trace;
use super::serialize::Serialize;

// Constants for snapshot interpolation
const INTERPOLATION_DELAY_MS: u32 = 100; // Fixed delay to handle network jitter
const MAX_STATES: usize = 10; // Maximum stored states to prevent memory growth

// Stores a snapshot state with its timestamp
#[derive(Debug, Clone)]
pub struct StateEntry<T: Serialize> {
    pub data: T,
    pub timestamp: u32,
}

// Manages client-side interpolation of game state snapshots
#[derive(Debug)]
pub struct Interpolator<T: Serialize> {
    states: VecDeque<StateEntry<T>>,
    max_states: usize,
}

impl<T: Serialize + Clone> Interpolator<T> {
    pub fn new(max_states: usize) -> Self {
        Interpolator {
            states: VecDeque::new(),
            max_states: max_states.max(2), // Ensure at least two states for interpolation
        }
    }

    // Adds a new snapshot state
    pub fn add_state(&mut self, data: T, timestamp: u32) {
        while self.states.len() >= self.max_states {
            self.states.pop_front();
        }
        self.states.push_back(StateEntry { data, timestamp });
        trace!("Added state with timestamp {}", timestamp);
    }

    // Interpolates between snapshots for smooth rendering
    pub fn interpolate(&self, current_time: u32) -> Option<T> {
        let render_time = current_time.saturating_sub(INTERPOLATION_DELAY_MS);
        let mut _before = None;
        let mut after = None;

        for (i, state) in self.states.iter().enumerate() {
            if state.timestamp <= render_time {
                _before = Some((i, state));
            } else if state.timestamp >= render_time && after.is_none() {
                after = Some((i, state));
                break;
            }
        }

        match (_before, after) {
            (Some((_, before_state)), Some((_, after_state))) => {
                let t = (render_time - before_state.timestamp) as f32 / 
                        (after_state.timestamp - before_state.timestamp) as f32;
                Some(self.interpolate_data(&before_state.data, &after_state.data, t.clamp(0.0, 1.0)))
            }
            (Some((_, state)), None) | (None, Some((_, state))) => Some(state.data.clone()),
            (None, None) => None,
        }
    }

    // Interpolates between two states (placeholder for custom interpolation)
    fn interpolate_data(&self, before: &T, after: &T, t: f32) -> T {
        // Note: This is a placeholder. Developers should implement custom interpolation
        // for their game state (e.g., linear interpolation for positions).
        // For generic T, we return a clone of the after state as a fallback.
        trace!("Interpolating states with t={}", t);
        after.clone()
    }
}