// src/tests/mod.rs - Centralized unit tests for gbnet

#[cfg(test)]
pub mod serialize_tests;

#[cfg(test)]
pub mod network_tests;

#[cfg(test)]
pub mod test_utils {
    use env_logger;
    
    pub fn init_logger() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .try_init();
    }
}