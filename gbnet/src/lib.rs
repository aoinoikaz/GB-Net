pub mod serialize;

#[macro_use]
extern crate gbnet_macros;

use serialize::bit_io::BitBuffer;
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

#[derive(NetworkSerialize, Default, Debug, PartialEq)]
pub struct GameState {
    #[bits = 10]
    round: u16,                       // 10 bits
    #[bits = 8]
    score: u8,                        // 8 bits
    is_paused: bool,                  // 1 bit
}

#[derive(NetworkSerialize, Default, Debug, PartialEq)]
pub struct PlayerInfo {
    #[bits = 6]
    health: u8,                       
    #[bits = 4]
    energy: u8,                       
    is_active: bool,                  
    nickname: Option<u8>,             // 1 bit discriminant + conditional 8 bits
}

// Test struct with String and array fields for macro testing
#[derive(NetworkSerialize, Debug, PartialEq)]
#[default_max_len = 32]
pub struct ExtendedMessage {
    #[max_len = 16]
    player_name: String,              // 16-bit length + string bytes
    coordinates: [f32; 3],            // Fixed array, no length prefix
    tags: Vec<String>,                // Dynamic array of strings
    metadata: (u8, bool, u16),        // Tuple serialization
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn init_logger() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug) // Reduced to Debug level
            .try_init();
    }

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
        
        // Calculate expected bits including Option<u8> fields
        // Player 1: health(6) + energy(4) + active(1) + Some(1+8) = 20 bits
        // Player 2: health(6) + energy(4) + active(1) + None(1) = 12 bits
        let expected_bits = 10 + 8 + 1 + 3 + 20 + 12 + 2 + 8 + 2 + (10 + 8 + 1); // 85 bits
        println!("Expected bits: {}, Actual bits: {}", expected_bits, bit_buffer.unpadded_length());
        
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
        assert_eq!(deserialized.players[0].nickname, packet.players[0].nickname);
        assert_eq!(deserialized.players[1].health, packet.players[1].health);
        assert_eq!(deserialized.players[1].energy, packet.players[1].energy);
        assert_eq!(deserialized.players[1].is_active, packet.players[1].is_active);
        assert_eq!(deserialized.players[1].nickname, packet.players[1].nickname);
        
        match (deserialized.message_type, packet.message_type) {
            (MessageType::Command { code: c1 }, MessageType::Command { code: c2 }) => {
                assert_eq!(c1, c2);
            }
            _ => panic!("MessageType mismatch"),
        }
        assert_eq!(deserialized.game_state, packet.game_state);
        Ok(())
    }

    #[test]
    fn test_string_serialization() -> std::io::Result<()> {
        init_logger();
        let test_string = "Hello, Network! 🚀".to_string();
        let mut bit_buffer = BitBuffer::new();
        test_string.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = String::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_string);
        println!("String serialization test passed: '{}'", deserialized);
        Ok(())
    }

    #[test]
    fn test_array_serialization() -> std::io::Result<()> {
        init_logger();
        let test_array: [u8; 4] = [1, 2, 3, 4];
        let mut bit_buffer = BitBuffer::new();
        test_array.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <[u8; 4]>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_array);
        println!("Array serialization test passed: {:?}", deserialized);
        Ok(())
    }

    #[test]
    fn test_tuple_serialization() -> std::io::Result<()> {
        init_logger();
        let test_tuple = (42u8, true, 1337u16);
        let mut bit_buffer = BitBuffer::new();
        test_tuple.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <(u8, bool, u16)>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_tuple);
        println!("Tuple serialization test passed: {:?}", deserialized);
        Ok(())
    }

    #[test]
    fn test_float_serialization() -> std::io::Result<()> {
        init_logger();
        // Test individual f32 values first
        let test_float = 10.5f32;
        let mut bit_buffer = BitBuffer::new();
        test_float.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = f32::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, test_float);
        println!("Float serialization test passed: {}", deserialized);
        Ok(())
    }

    #[test]
    fn test_extended_message_with_new_types() -> std::io::Result<()> {
        init_logger();
        let message = ExtendedMessage {
            player_name: "Alice".to_string(),
            coordinates: [10.5, 20.3, 30.7],
            tags: vec!["VIP".to_string(), "Pro".to_string()],
            metadata: (255u8, true, 65535u16),
        };

        let mut bit_buffer = BitBuffer::new();
        message.bit_serialize(&mut bit_buffer)?;
        
        println!("ExtendedMessage serialized to {} bits", bit_buffer.unpadded_length());
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = ExtendedMessage::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized.player_name, message.player_name);
        assert_eq!(deserialized.coordinates, message.coordinates); // This should work now with f32 bit serialization fix
        assert_eq!(deserialized.tags, message.tags);
        assert_eq!(deserialized.metadata, message.metadata);
        
        println!("ExtendedMessage test passed with all new types!");
        Ok(())
    }

    #[test]
    fn test_complex_nested_structures() -> std::io::Result<()> {
        init_logger();
        
        // Test deeply nested structures
        let nested_data: Vec<Vec<(String, [u8; 2])>> = vec![
            vec![
                ("first".to_string(), [1, 2]),
                ("second".to_string(), [3, 4]),
            ],
            vec![
                ("third".to_string(), [5, 6]),
            ],
        ];

        let mut bit_buffer = BitBuffer::new();
        nested_data.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<Vec<(String, [u8; 2])>>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, nested_data);
        println!("Complex nested structure test passed!");
        Ok(())
    }

    #[test]
    fn test_option_variants() -> std::io::Result<()> {
        init_logger();
        
        // Test different Option combinations
        let options: Vec<Option<String>> = vec![
            None,
            Some("test".to_string()),
            None,
            Some("another".to_string()),
        ];

        let mut bit_buffer = BitBuffer::new();
        options.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<Option<String>>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, options);
        println!("Option variants test passed!");
        Ok(())
    }

    #[test]
    fn test_empty_collections() -> std::io::Result<()> {
        init_logger();
        
        // Test empty vector
        let empty_vec: Vec<String> = vec![];
        let mut bit_buffer = BitBuffer::new();
        empty_vec.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = Vec::<String>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, empty_vec);
        
        // Test empty string
        let empty_string = String::new();
        let mut bit_buffer = BitBuffer::new();
        empty_string.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = String::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, empty_string);
        println!("Empty collections test passed!");
        Ok(())
    }

    #[test]
    fn test_large_arrays() -> std::io::Result<()> {
        init_logger();
        
        // Test larger fixed array
        let large_array: [u16; 32] = [0; 32];
        let mut bit_buffer = BitBuffer::new();
        large_array.bit_serialize(&mut bit_buffer)?;
        
        let bit_data = bit_buffer.into_bytes(false)?;
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = <[u16; 32]>::bit_deserialize(&mut bit_buffer)?;
        
        assert_eq!(deserialized, large_array);
        println!("Large array test passed!");
        Ok(())
    }
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn benchmark_optimization() -> std::io::Result<()> {
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

        // Single test run for debugging
        let mut debug_buffer = BitBuffer::new();
        packet.bit_serialize(&mut debug_buffer)?;
        
        println!("Serialized bit length: {}", debug_buffer.unpadded_length());
        
        let debug_bytes = debug_buffer.into_bytes(false)?;
        println!("Serialized bytes: {:?} (length: {})", debug_bytes, debug_bytes.len());
        
        // Benchmark
        let start = Instant::now();
        
        // Run test 1000 times
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

    #[test]
    fn benchmark_string_vs_primitives() -> std::io::Result<()> {
        let test_string = "Hello, Network Protocol!".to_string();
        let test_number = 42u32;
        
        let iterations = 10000;
        
        // String benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            let mut buffer = BitBuffer::new();
            test_string.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = String::bit_deserialize(&mut buffer)?;
        }
        let string_duration = start.elapsed();
        
        // Number benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            let mut buffer = BitBuffer::new();
            test_number.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = u32::bit_deserialize(&mut buffer)?;
        }
        let number_duration = start.elapsed();
        
        println!("String ({} chars) serialization: {:?}", test_string.len(), string_duration);
        println!("u32 serialization: {:?}", number_duration);
        println!("String is {:.2}x slower", string_duration.as_nanos() as f64 / number_duration.as_nanos() as f64);
        
        Ok(())
    }

    #[test]
    fn benchmark_new_types() -> std::io::Result<()> {
        let extended_msg = ExtendedMessage {
            player_name: "BenchmarkPlayer".to_string(),
            coordinates: [1.0, 2.0, 3.0],
            tags: vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()],
            metadata: (100u8, false, 5000u16),
        };

        let iterations = 5000;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let mut buffer = BitBuffer::new();
            extended_msg.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = ExtendedMessage::bit_deserialize(&mut buffer)?;
        }
        
        let duration = start.elapsed();
        println!("ExtendedMessage ({} iterations) took: {:?}", iterations, duration);
        println!("Average per operation: {:?}", duration / iterations);
        
        Ok(())
    }

    #[test]
    fn benchmark_array_vs_vec() -> std::io::Result<()> {
        let fixed_array: [u32; 16] = [42; 16];
        let dynamic_vec: Vec<u32> = vec![42; 16];
        
        let iterations = 10000;
        
        // Array benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            let mut buffer = BitBuffer::new();
            fixed_array.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = <[u32; 16]>::bit_deserialize(&mut buffer)?;
        }
        let array_duration = start.elapsed();
        
        // Vec benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            let mut buffer = BitBuffer::new();
            dynamic_vec.bit_serialize(&mut buffer)?;
            let bytes = buffer.into_bytes(false)?;
            let mut buffer = BitBuffer::from_bytes(bytes);
            let _deserialized = Vec::<u32>::bit_deserialize(&mut buffer)?;
        }
        let vec_duration = start.elapsed();
        
        println!("Array [u32; 16] serialization: {:?}", array_duration);
        println!("Vec<u32> (16 items) serialization: {:?}", vec_duration);
        println!("Array is {:.2}x faster", vec_duration.as_nanos() as f64 / array_duration.as_nanos() as f64);
        
        Ok(())
    }
}