pub mod serialize;

#[macro_use]
extern crate gbnet_macros;

use serialize::bit_io::BitBuffer;
#[cfg(test)]
use serialize::bit_io::{BitRead, BitWrite};
use serialize::{BitDeserialize, BitSerialize};

#[derive(NetworkSerialize)]
#[default_bits(u16 = 10, bool = 1)]
#[default_max_len = 16]
pub struct NetworkMessage {
    #[bits = 10]
    message_id: u16,                  // 10 bits
    #[bits = 8]
    priority: u8,                     // 8 bits
    is_urgent: bool,                  // 1 bit
    #[max_len = 4]
    players: Vec<PlayerInfo>,         // 3 bits for length (ceil(log2(5))), + PlayerInfo bits
    message_type: MessageType,        // 2 bits (4 variants) + payload
    #[byte_align]
    game_state: GameState,            // Aligned to byte boundary, + GameState bits
}

#[derive(NetworkSerialize)]
pub enum MessageType {
    StatusUpdate,                     // 0 (2 bits)
    Command { #[bits = 8] code: u8 }, // 1 (2 bits) + 8 bits
    Alert { #[bits = 4] level: u8 },  // 2 (2 bits) + 4 bits
    Sync,                    // 3 (2 bits)
}

#[derive(NetworkSerialize, Default, Debug)]
pub struct GameState {
    #[bits = 10]
    round: u16,                       // 10 bits
    #[bits = 8]
    score: u8,                        // 8 bits
    is_paused: bool,                  // 1 bit
}

#[derive(NetworkSerialize, Default, Debug)]
pub struct PlayerInfo {
    #[bits = 6]
    health: u8,                       
    #[bits = 4]
    energy: u8,                       
    is_active: bool,                  
    // NEW: Add this optional field
    nickname: Option<u8>,             // 1 bit discriminant + conditional 8 bits
}

fn init_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_message_serialization() -> std::io::Result<()> {
        init_logger();
        let packet = NetworkMessage {
            message_id: 500,
            priority: 3,
            is_urgent: true,
            players: vec![
                PlayerInfo {
                    health: 50,
                    energy: 10,
                    is_active: true,
                    nickname: Some(42)
                },
                PlayerInfo {
                    health: 30,
                    energy: 5,
                    is_active: false,
                    nickname: None
                },
            ],
            message_type: MessageType::Command { code: 42 },
            game_state: GameState {
                round: 100,
                score: 255,
                is_paused: false,
            },
        };
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        let bit_length = 10 + 8 + 1 + 3 + (2 * (6 + 4 + 1)) + 2 + 8 + 2 + (10 + 8 + 1); // 75 bits
        assert_eq!(bit_buffer.unpadded_length(), bit_length, "Bit length mismatch");
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = NetworkMessage::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.message_id, packet.message_id);
        assert_eq!(deserialized.priority, packet.priority);
        assert_eq!(deserialized.is_urgent, packet.is_urgent);
        assert_eq!(deserialized.players.len(), packet.players.len());
        assert_eq!(deserialized.players[0].health, packet.players[0].health);
        assert_eq!(deserialized.players[0].energy, packet.players[0].energy);
        assert_eq!(deserialized.players[0].is_active, packet.players[0].is_active);
        assert_eq!(deserialized.players[1].health, packet.players[1].health);
        assert_eq!(deserialized.players[1].energy, packet.players[1].energy);
        assert_eq!(deserialized.players[1].is_active, packet.players[1].is_active);
        match (deserialized.message_type, packet.message_type) {
            (MessageType::Command { code: c1 }, MessageType::Command { code: c2 }) => {
                assert_eq!(c1, c2);
            }
            _ => panic!("MessageType mismatch"),
        }
        assert_eq!(deserialized.game_state.round, packet.game_state.round);
        assert_eq!(deserialized.game_state.score, packet.game_state.score);
        assert_eq!(deserialized.game_state.is_paused, packet.game_state.is_paused);
        Ok(())
    }
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn benchmark_optimization() -> std::io::Result<()> {
        // ADD: Single test run for debugging BEFORE the benchmark
        let packet = NetworkMessage {
            message_id: 500,
            priority: 3,
            is_urgent: true,
            players: vec![
                PlayerInfo {
                    health: 50,
                    energy: 10,
                    is_active: true,
                    nickname: Some(42)
                },
                PlayerInfo {
                    health: 30,
                    energy: 5,
                    is_active: false,
                    nickname: None
                },
            ],
            message_type: MessageType::Command { code: 42 },
            game_state: GameState {
                round: 100,
                score: 255,
                is_paused: false,
            },
        };

        let mut debug_buffer = BitBuffer::new();
        packet.bit_serialize(&mut debug_buffer)?;
        //let expected_bits = 10 + 8 + 1 + 3 + (2 * (6 + 4 + 1)) + 2 + 8 + 2 + (10 + 8 + 1); // 75 bits
        
        //println!("Serialized bit length: {}", debug_buffer.unpadded_length());
        //println!("Expected bit length: {}", expected_bits);
        
        //let debug_bytes = debug_buffer.into_bytes(false)?;
        //println!("Serialized bytes: {:?} (length: {})", debug_bytes, debug_bytes.len());
        
        // Now run the actual benchmark
        let start = Instant::now();
        
        // Run your existing test 1000 times
        for _ in 0..1000 {
            let mut buffer = BitBuffer::new();
            packet.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = NetworkMessage::bit_deserialize(&mut buffer)?;
        }
        
        println!("1000 serialization cycles took: {:?}", start.elapsed());
        Ok(())
    }
}