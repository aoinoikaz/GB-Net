pub mod serialize;

#[macro_use]
extern crate gbnet_macros;

/// `NetworkSerialize` derives bit- and byte-aligned serialization/deserialization.
/// - `#[bits = N]` specifies the bit count for a field in bit-serialization.
/// - In byte-aligned mode, `#[bits]` is ignored, and fields are written as full bytes.
/// - Use `#[no_serialize]` to skip fields, `#[max_len]` for Vec lengths, and `#[byte_align]` for byte alignment in bit-serialization.
/// - `#[default_max_len = N]` sets a default max length for Vec<T> fields without `#[max_len]`.
#[derive(NetworkSerialize, Default)]
#[default_bits(u8 = 4, u16 = 10, bool = 1)]
#[default_max_len = 16]
pub struct GamePacket {
    sequence: u16,                    // 10 bits
    #[bits = 8]                      // 8 bits (override)
    packet_type: u8,
    health: u8,                       // 8 bits (macro default)
    ammo: u8,                         // 8 bits (macro default)
    #[bits = 6]                      // 6 bits (override)
    energy: u8,
    is_active: bool,                  // 1 bit
    #[bits = 16]                     // 16 bits (override)
    x_pos: u16,
    #[max_len = 8]                   // 4 bits for length (ceil(log2(9)))
    players: Vec<PlayerState>,
    status: PlayerStatus,             // 1 bit + payload
}

#[derive(NetworkSerialize, Debug)]
pub struct PlayerState {
    #[bits = 4]
    health: u8,                       // 4 bits (explicit)
}

#[derive(NetworkSerialize, Debug)]
pub enum PlayerStatus {
    Idle,                             // 0 (1 bit)
    Running { #[bits = 4] speed: u8 }, // 1 (1 bit) + 4 bits
}

impl Default for PlayerStatus {
    fn default() -> Self {
        PlayerStatus::Idle
    }
}

#[derive(NetworkSerialize, Default)]
#[default_bits(u8 = 4, u16 = 10, bool = 1)]
#[default_max_len = 16]
pub struct LargeGamePacket {
    sequence: u16,                    // 10 bits
    #[bits = 8]
    packet_type: u8,                  // 8 bits
    health: u8,                       // 8 bits (macro default)
    ammo: u8,                         // 8 bits (macro default)
    energy: u8,                       // 8 bits (macro default)
    shield: u8,                       // 8 bits (macro default)
    #[bits = 6]
    special_counter: u8,              // 6 bits
    x_pos: u16,                       // 10 bits
    y_pos: u16,                       // 10 bits
    is_alive: bool,                   // 1 bit
    #[byte_align]
    flag: bool,                       // 1 bit (padded to byte boundary)
    #[max_len = 8]
    players: Vec<PlayerState>,        // 4 bits for length (ceil(log2(9)))
    status: PlayerStatus,             // 1 bit + payload
}

#[cfg(test)]
mod tests {
    use crate::{PlayerState, PlayerStatus};
    use crate::serialize::{
        BitSerialize, BitDeserialize, bit_io::{BitBuffer, BitWrite, generate_bit_pattern},
    };
    use std::io::ErrorKind;
    use std::env;
    use log::debug;

    fn init_logger() {
        env::set_var("RUST_LOG", "debug,gbnet::serialize::bit_io=trace");
        let _ = env_logger::builder().is_test(true).try_init();
    }

    fn print_bit_buffer(buffer: &[u8], bit_length: usize, field_desc: &str) -> String {
        debug!(
            "Serialized buffer for {} ({} bits): {:?}",
            field_desc, bit_length, buffer
        );
        let mut bit_string = String::new();
        let mut bits_written = 0;

        // Calculate the flushed length (padded to next byte boundary)
        let flushed_length = (bit_length + 7) / 8 * 8;

        for (i, &byte) in buffer.iter().enumerate() {
            for j in (0..8).rev() {
                if bits_written < flushed_length {
                    let bit = (byte >> j) & 1;
                    bit_string.push_str(&bit.to_string());
                    bits_written += 1;
                } else {
                    break;
                }
            }
            if bits_written < flushed_length && i < buffer.len() - 1 {
                bit_string.push(' ');
            }
        }

        // Pad with zeros if buffer is too short
        while bits_written < flushed_length {
            bit_string.push('0');
            bits_written += 1;
            if bits_written % 8 == 0 && bits_written < flushed_length {
                bit_string.push(' ');
            }
        }

        debug!(
            "Bit-level breakdown for {}: {}",
            field_desc,
            bit_string.trim()
        );
        bit_string.trim().to_string()
    }

    #[test]
    fn test_primitive_serialization() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize)]
        struct PrimitivePacket {
            a: u8,           // 8 bits (macro default)
            #[bits = 6]
            b: u8,           // 6 bits
            c: bool,         // 1 bit
        }
        let packet = PrimitivePacket { a: 15, b: 50, c: true };
        debug!(
            "Starting test_primitive_serialization with packet: a={}, b={}, c={}",
            packet.a, packet.b, packet.c
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let bit_length = 8 + 6 + 1; // a (8) + b (6) + c (1) = 15 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "PrimitivePacket");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&packet)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = PrimitivePacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: a={}, b={}, c={}",
            deserialized.a, deserialized.b, deserialized.c
        );
        assert_eq!(deserialized.a, 15, "Expected a=15, got {}", deserialized.a);
        assert_eq!(deserialized.b, 50, "Expected b=50, got {}", deserialized.b);
        assert_eq!(
            deserialized.c, true,
            "Expected c=true, got {}",
            deserialized.c
        );
        Ok(())
    }

    #[test]
    fn test_vector_max_len() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize)]
        #[default_max_len = 4]
        struct VecPacket {
            #[max_len = 2]
            data: Vec<u8>,
        }
        debug!("Starting test_vector_max_len");
        // Valid case
        let valid = VecPacket {
            data: vec![1, 2],
        };
        debug!("Serializing valid VecPacket: {:?}", valid.data);
        let mut bit_buffer = BitBuffer::new();
        valid.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let len_bits = ((2 + 1) as f64).log2().ceil() as usize; // ceil(log2(3)) = 2
        let bit_length = len_bits + 2 * 8; // len (2) + 2*u8 (16) = 18 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "VecPacket (valid)");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&valid)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = VecPacket::bit_deserialize(&mut bit_buffer)?;
        debug!("Deserialized packet: data={:?}", deserialized.data);
        assert_eq!(
            deserialized.data,
            vec![1, 2],
            "Expected data=[1, 2], got {:?}",
            deserialized.data
        );

        // Invalid case
        let invalid = VecPacket {
            data: vec![1, 2, 3],
        };
        debug!("Attempting to serialize invalid VecPacket: {:?}", invalid.data);
        let mut bit_buffer = BitBuffer::new();
        let result = invalid.bit_serialize(&mut bit_buffer);
        assert!(
            result.is_err(),
            "Expected error for vector length > max_len"
        );
        assert_eq!(
            result.unwrap_err().kind(),
            ErrorKind::InvalidData,
            "Expected InvalidData error"
        );
        Ok(())
    }

    #[test]
    fn test_byte_alignment() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize)]
        struct AlignPacket {
            a: bool,         // 1 bit
            #[byte_align]
            b: u8,           // 8 bits (macro default), after padding
        }
        let packet = AlignPacket { a: true, b: 10 };
        debug!(
            "Starting test_byte_alignment with packet: a={}, b={}",
            packet.a, packet.b
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let bit_length = 1 + 7 + 8; // a (1) + padding (7) + b (8) = 16 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "AlignPacket");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&packet)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = AlignPacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: a={}, b={}",
            deserialized.a, deserialized.b
        );
        assert_eq!(
            deserialized.a, true,
            "Expected a=true, got {}",
            deserialized.a
        );
        assert_eq!(deserialized.b, 10, "Expected b=10, got {}", deserialized.b);
        Ok(())
    }

    #[test]
    fn test_complex_nested_structure() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize, Default)]
        #[default_bits(u8 = 4, u16 = 10, bool = 1)]
        #[default_max_len = 16]
        struct ComplexPacket {
            id: u16,                      // 10 bits
            #[bits = 6]
            flags: u8,                    // 6 bits
            #[max_len = 4]
            players: Vec<PlayerState>,    // 3 bits for length (ceil(log2(5)))
            status: PlayerStatus,         // 1 bit + payload
            #[byte_align]
            settings: GameSettings,       // Byte-aligned nested struct
        }

        #[derive(NetworkSerialize, Default, Debug)]
        struct GameSettings {
            difficulty: u8,               // 8 bits (macro default)
            is_multiplayer: bool,         // 1 bit
            #[max_len = 2]
            modifiers: Vec<u8>,           // 2 bits for length (ceil(log2(3)))
        }

        let packet = ComplexPacket {
            id: 1023,
            flags: 63,
            players: vec![
                PlayerState { health: 15 },
                PlayerState { health: 10 },
            ],
            status: PlayerStatus::Running { speed: 7 },
            settings: GameSettings {
                difficulty: 3,
                is_multiplayer: true,
                modifiers: vec![1, 2],
            },
        };
        debug!(
            "Starting test_complex_nested_structure with packet: id={}, flags={}, players={:?}, status={:?}, settings={:?}",
            packet.id, packet.flags, packet.players, packet.status, packet.settings
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();

        // Generate expected bit pattern
        let (expected_bit_pattern, bit_length) = generate_bit_pattern(&packet)?;
        let bit_string = print_bit_buffer(&bit_data, bit_length, "ComplexPacket");

        // Compare actual and expected bit patterns
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            bit_length,
            (bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = ComplexPacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: id={}, flags={}, players={:?}, status={:?}, settings={:?}",
            deserialized.id, deserialized.flags, deserialized.players, deserialized.status, deserialized.settings
        );
        assert_eq!(
            deserialized.id, 1023,
            "Expected id=1023, got {}",
            deserialized.id
        );
        assert_eq!(
            deserialized.flags, 63,
            "Expected flags=63, got {}",
            deserialized.flags
        );
        assert_eq!(
            deserialized.players.len(),
            2,
            "Expected 2 players, got {}",
            deserialized.players.len()
        );
        assert_eq!(
            deserialized.players[0].health,
            15,
            "Expected player[0].health=15, got {}",
            deserialized.players[0].health
        );
        assert_eq!(
            deserialized.players[1].health,
            10,
            "Expected player[1].health=10, got {}",
            deserialized.players[1].health
        );
        match deserialized.status {
            PlayerStatus::Running { speed } => assert_eq!(
                speed, 7,
                "Expected status.speed=7, got {}",
                speed
            ),
            _ => panic!("Expected Running status, got {:?}", deserialized.status),
        }
        assert_eq!(
            deserialized.settings.difficulty,
            3,
            "Expected settings.difficulty=3, got {}",
            deserialized.settings.difficulty
        );
        assert_eq!(
            deserialized.settings.is_multiplayer,
            true,
            "Expected settings.is_multiplayer=true, got {}",
            deserialized.settings.is_multiplayer
        );
        assert_eq!(
            deserialized.settings.modifiers,
            vec![1, 2],
            "Expected settings.modifiers=[1, 2], got {:?}",
            deserialized.settings.modifiers
        );
        Ok(())
    }

    #[test]
    fn test_another_complex_nested_structure() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize, Default)]
        #[default_bits(u8 = 4, u16 = 10, bool = 1)]
        #[default_max_len = 16]
        struct GameWorldPacket {
            world_id: u16,                    // 10 bits
            #[max_len = 4]
            entities: Vec<Entity>,            // 3 bits for length (ceil(log2(5)))
            state: WorldState,                // 1 bit + payload
            #[byte_align]
            settings: EnvironmentSettings,    // Byte-aligned nested struct
        }

        #[derive(NetworkSerialize, Default, Debug)]
        struct Entity {
            entity_id: u8,                    // 8 bits (macro default)
            health: u8,                       // 8 bits (macro default)
            is_active: bool,                  // 1 bit
        }

        #[derive(NetworkSerialize, Default, Debug)]
        enum WorldState {
            #[default]
            Paused,
            Active { #[bits = 4] time: u8 },  // 1 bit (variant) + 4 bits (time)
        }

        #[derive(NetworkSerialize, Default, Debug)]
        struct EnvironmentSettings {
            temperature: u8,                  // 8 bits (macro default)
            #[max_len = 2]
            effects: Vec<u8>,                 // 2 bits for length (ceil(log2(3)))
        }

        let packet = GameWorldPacket {
            world_id: 512,
            entities: vec![
                Entity {
                    entity_id: 1,
                    health: 100,
                    is_active: true,
                },
                Entity {
                    entity_id: 2,
                    health: 50,
                    is_active: false,
                },
            ],
            state: WorldState::Active { time: 10 },
            settings: EnvironmentSettings {
                temperature: 25,
                effects: vec![3, 4],
            },
        };
        debug!(
            "Starting test_another_complex_nested_structure with packet: world_id={}, entities={:?}, state={:?}, settings={:?}",
            packet.world_id, packet.entities, packet.state, packet.settings
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();

        // Generate expected bit pattern
        let (expected_bit_pattern, bit_length) = generate_bit_pattern(&packet)?;
        let bit_string = print_bit_buffer(&bit_data, bit_length, "GameWorldPacket");

        // Compare actual and expected bit patterns
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            bit_length,
            (bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = GameWorldPacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: world_id={}, entities={:?}, state={:?}, settings={:?}",
            deserialized.world_id, deserialized.entities, deserialized.state, deserialized.settings
        );
        assert_eq!(
            deserialized.world_id, 512,
            "Expected world_id=512, got {}",
            deserialized.world_id
        );
        assert_eq!(
            deserialized.entities.len(),
            2,
            "Expected 2 entities, got {}",
            deserialized.entities.len()
        );
        assert_eq!(
            deserialized.entities[0].entity_id,
            1,
            "Expected entities[0].entity_id=1, got {}",
            deserialized.entities[0].entity_id
        );
        assert_eq!(
            deserialized.entities[0].health,
            100,
            "Expected entities[0].health=100, got {}",
            deserialized.entities[0].health
        );
        assert_eq!(
            deserialized.entities[0].is_active,
            true,
            "Expected entities[0].is_active=true, got {}",
            deserialized.entities[0].is_active
        );
        assert_eq!(
            deserialized.entities[1].entity_id,
            2,
            "Expected entities[1].entity_id=2, got {}",
            deserialized.entities[1].entity_id
        );
        assert_eq!(
            deserialized.entities[1].health,
            50,
            "Expected entities[1].health=50, got {}",
            deserialized.entities[1].health
        );
        assert_eq!(
            deserialized.entities[1].is_active,
            false,
            "Expected entities[1].is_active=false, got {}",
            deserialized.entities[1].is_active
        );
        match deserialized.state {
            WorldState::Active { time } => assert_eq!(
                time, 10,
                "Expected state.time=10, got {}",
                time
            ),
            _ => panic!("Expected Active state, got {:?}", deserialized.state),
        }
        assert_eq!(
            deserialized.settings.temperature,
            25,
            "Expected settings.temperature=25, got {}",
            deserialized.settings.temperature
        );
        assert_eq!(
            deserialized.settings.effects,
            vec![3, 4],
            "Expected settings.effects=[3, 4], got {:?}",
            deserialized.settings.effects
        );
        Ok(())
    }

    #[test]
    fn test_no_serialize() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize, Debug, PartialEq)]
        struct NoSerializePacket {
            a: u8,                   // 8 bits (macro default)
            #[no_serialize]
            b: u16,                  // Skipped
            c: bool,                 // 1 bit
            #[no_serialize]
            d: String,               // Skipped
        }
        let packet = NoSerializePacket {
            a: 42,
            b: 9999, // Should be ignored
            c: true,
            d: "test".to_string(), // Should be ignored
        };
        debug!(
            "Starting test_no_serialize with packet: a={}, b={}, c={}, d={}",
            packet.a, packet.b, packet.c, packet.d
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let bit_length = 8 + 1; // a (8) + c (1) = 9 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "NoSerializePacket");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&packet)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = NoSerializePacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: a={}, b={}, c={}, d={}",
            deserialized.a, deserialized.b, deserialized.c, deserialized.d
        );
        assert_eq!(
            deserialized.a, 42,
            "Expected a=42, got {}",
            deserialized.a
        );
        assert_eq!(
            deserialized.c, true,
            "Expected c=true, got {}",
            deserialized.c
        );
        // Non-serialized fields should retain default values
        assert_eq!(
            deserialized.b, 0,
            "Expected b=0 (default), got {}",
            deserialized.b
        );
        assert_eq!(
            deserialized.d, "",
            "Expected d='' (default), got {}",
            deserialized.d
        );
        Ok(())
    }

    #[test]
    fn test_default_bit_sizes() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize, Debug, PartialEq)]
        #[default_bits(u8 = 4, u16 = 10, bool = 1)]
        struct DefaultBitPacket {
            a: u8,      // 4 bits (default)
            b: u16,     // 10 bits (default)
            c: bool,    // 1 bit (default)
        }
        let packet = DefaultBitPacket {
            a: 15,
            b: 1023,
            c: false,
        };
        debug!(
            "Starting test_default_bit_sizes with packet: a={}, b={}, c={}",
            packet.a, packet.b, packet.c
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let bit_length = 4 + 10 + 1; // a (4) + b (10) + c (1) = 15 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "DefaultBitPacket");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&packet)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = DefaultBitPacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: a={}, b={}, c={}",
            deserialized.a, deserialized.b, deserialized.c
        );
        assert_eq!(
            deserialized.a, 15,
            "Expected a=15, got {}",
            deserialized.a
        );
        assert_eq!(
            deserialized.b, 1023,
            "Expected b=1023, got {}",
            deserialized.b
        );
        assert_eq!(
            deserialized.c, false,
            "Expected c=false, got {}",
            deserialized.c
        );
        Ok(())
    }

    #[test]
    fn test_empty_vector() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize, Debug, PartialEq)]
        #[default_max_len = 4]
        struct EmptyVecPacket {
            id: u8,                // 8 bits (macro default)
            #[max_len = 4]
            data: Vec<u8>,         // 3 bits for length (ceil(log2(5)))
        }
        let packet = EmptyVecPacket {
            id: 100,
            data: vec![],
        };
        debug!(
            "Starting test_empty_vector with packet: id={}, data={:?}",
            packet.id, packet.data
        );
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let len_bits = ((4 + 1) as f64).log2().ceil() as usize; // ceil(log2(5)) = 3
        let bit_length = 8 + len_bits; // id (8) + len (3) = 11 bits
        let bit_string = print_bit_buffer(&bit_data, bit_length, "EmptyVecPacket");

        // Generate expected bit pattern
        let (expected_bit_pattern, expected_bit_length) = generate_bit_pattern(&packet)?;
        assert_eq!(
            bit_string.replace(" ", ""),
            expected_bit_pattern,
            "Incorrect bit pattern"
        );
        assert!(
            bit_data.len() <= (expected_bit_length + 7) / 8,
            "Expected ~{} bits ({} bytes), got {} bytes",
            expected_bit_length,
            (expected_bit_length + 7) / 8,
            bit_data.len()
        );

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = EmptyVecPacket::bit_deserialize(&mut bit_buffer)?;
        debug!(
            "Deserialized packet: id={}, data={:?}",
            deserialized.id, deserialized.data
        );
        assert_eq!(
            deserialized.id, 100,
            "Expected id=100, got {}",
            deserialized.id
        );
        assert_eq!(
            deserialized.data,
            vec![],
            "Expected data=[], got {:?}",
            deserialized.data
        );
        Ok(())
    }
}