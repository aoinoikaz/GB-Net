use std::time::{Duration, Instant};
use log::trace;

// Constants for fixed timestep simulation
const FIXED_DT: f32 = 1.0 / 60.0; // 60 Hz simulation rate

// Manages fixed timestep simulation for consistent game updates
#[derive(Debug)]
pub struct FixedTimestep {
    dt: f32, // Fixed timestep in seconds
    accumulator: f32, // Accumulated time for partial steps
    last_update: Instant, // Last update time
}

impl FixedTimestep {
    pub fn new(dt: Duration) -> Self {
        FixedTimestep {
            dt: dt.as_secs_f32(),
            accumulator: 0.0,
            last_update: Instant::now(),
        }
    }

    // Updates the timestep, returning true if a simulation step is needed
    pub fn update(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        self.accumulator += elapsed;

        if self.accumulator >= self.dt {
            self.accumulator -= self.dt;
            trace!("Timestep triggered: dt={}s, accumulator={}s", self.dt, self.accumulator);
            true
        } else {
            false
        }
    }

    // Returns the fixed timestep
    pub fn get_dt(&self) -> f32 {
        self.dt
    }
}