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
#[allow(dead_code)] // Suppress unused local_id warning
pub struct GamePacket {
    sequence: u16,                    // 10 bits
    #[bits = 8]                      // 8 bits (override)
    packet_type: u8,
    health: u8,                       // 4 bits
    ammo: u8,                         // 4 bits
    #[bits = 6]                      // 6 bits (override)
    energy: u8,
    is_active: bool,                  // 1 bit
    #[bits = 16]                     // 16 bits (override)
    x_pos: u16,
    #[no_serialize]
    local_id: u32,
    #[max_len = 8]                   // 3 bits for length (ceil(log2(8)) = 3)
    players: Vec<PlayerState>,
    status: PlayerStatus,             // 1 bit + payload
}

#[derive(NetworkSerialize)]
pub struct PlayerState {
    #[bits = 4]
    health: u8,                       // 4 bits
}

#[derive(NetworkSerialize)]
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
#[allow(dead_code)] // Suppress unused local_id warning
pub struct LargeGamePacket {
    sequence: u16,                    // 10 bits
    #[bits = 8]
    packet_type: u8,                  // 8 bits
    health: u8,                       // 4 bits
    ammo: u8,                         // 4 bits
    energy: u8,                       // 4 bits
    shield: u8,                       // 4 bits
    #[bits = 6]
    special_counter: u8,              // 6 bits
    x_pos: u16,                       // 10 bits
    y_pos: u16,                       // 10 bits
    is_alive: bool,                   // 1 bit
    #[byte_align]
    flag: bool,                       // 1 bit (padded to byte boundary)
    #[no_serialize]
    local_id: u32,
    #[max_len = 8]
    players: Vec<PlayerState>,        // 3 bits for length (ceil(log2(8)) = 3)
    status: PlayerStatus,             // 1 bit + payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialize::{BitSerialize, BitDeserialize, ByteAlignedSerialize, ByteAlignedDeserialize, bit_io::{BitBuffer, BitWrite}};
    use std::io::ErrorKind;

    #[test]
    fn test_game_packet_serialization() -> std::io::Result<()> {
        let packet = GamePacket {
            sequence: 500,
            packet_type: 1,
            health: 10,
            ammo: 15,
            energy: 50,
            is_active: true,
            x_pos: 1000,
            local_id: 42,
            players: vec![PlayerState { health: 12 }],
            status: PlayerStatus::Running { speed: 5 },
        };

        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        assert!(bit_data.len() <= 8); // ~61 bits ≈ 8 bytes (10+8+4+4+6+1+16+3+4+1+4)

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = GamePacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.sequence, 500);
        assert_eq!(deserialized.packet_type, 1);
        assert_eq!(deserialized.health, 10);
        assert_eq!(deserialized.ammo, 15);
        assert_eq!(deserialized.energy, 50);
        assert_eq!(deserialized.is_active, true);
        assert_eq!(deserialized.x_pos, 1000);
        assert_eq!(deserialized.local_id, 0);
        assert_eq!(deserialized.players.len(), 1);
        assert_eq!(deserialized.players[0].health, 12);
        assert!(matches!(deserialized.status, PlayerStatus::Running { speed: 5 }));

        let mut byte_buffer = Vec::new();
        packet.byte_aligned_serialize(&mut byte_buffer)?;
        assert_eq!(byte_buffer.len(), 17); // 2+1+1+1+1+1+2+4+4+1+1

        let mut cursor = std::io::Cursor::new(byte_buffer);
        let deserialized = GamePacket::byte_aligned_deserialize(&mut cursor)?;
        assert_eq!(deserialized.sequence, 500);
        assert_eq!(deserialized.packet_type, 1);
        assert_eq!(deserialized.health, 10);

        Ok(())
    }

    #[test]
    fn test_large_game_packet_serialization() -> std::io::Result<()> {
        let packet = LargeGamePacket {
            sequence: 1000,
            packet_type: 1,
            health: 10,
            ammo: 15,
            energy: 12,
            shield: 8,
            special_counter: 50,
            x_pos: 500,
            y_pos: 600,
            is_alive: true,
            flag: true,
            local_id: 42,
            players: vec![PlayerState { health: 12 }],
            status: PlayerStatus::Idle,
        };

        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        assert!(bit_data.len() <= 10); // ~70 bits ≈ 9 bytes (10+8+4+4+4+4+6+10+10+1+1+padding+3+4+1)

        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = LargeGamePacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.sequence, 1000);
        assert_eq!(deserialized.packet_type, 1);
        assert_eq!(deserialized.health, 10);
        assert_eq!(deserialized.ammo, 15);
        assert_eq!(deserialized.energy, 12);
        assert_eq!(deserialized.shield, 8);
        assert_eq!(deserialized.special_counter, 50);
        assert_eq!(deserialized.x_pos, 500);
        assert_eq!(deserialized.y_pos, 600);
        assert_eq!(deserialized.is_alive, true);
        assert_eq!(deserialized.flag, true);
        assert_eq!(deserialized.local_id, 0);
        assert_eq!(deserialized.players.len(), 1);
        assert_eq!(deserialized.players[0].health, 12);
        assert!(matches!(deserialized.status, PlayerStatus::Idle));

        Ok(())
    }

    #[test]
    fn test_default_max_len() -> std::io::Result<()> {
        #[derive(NetworkSerialize)]
        #[default_max_len = 8]
        struct TestPacket {
            #[max_len = 4]
            vec1: Vec<u8>,
            vec2: Vec<u8>,
        }
        let packet = TestPacket {
            vec1: vec![1, 2],
            vec2: vec![3, 4, 5],
        };
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = TestPacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.vec1, vec![1, 2]);
        assert_eq!(deserialized.vec2, vec![3, 4, 5]);
        Ok(())
    }

    #[test]
    fn test_max_len_validation() -> std::io::Result<()> {
        #[derive(NetworkSerialize)]
        struct TestPacket {
            #[max_len = 4]
            data: Vec<u8>,
        }

        // Valid case: length 4 (within max_len)
        let valid_packet = TestPacket {
            data: vec![1, 2, 3, 4],
        };
        let mut bit_buffer = BitBuffer::new();
        valid_packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = TestPacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.data, vec![1, 2, 3, 4]);

        // Invalid case: length 5 (exceeds max_len)
        let invalid_packet = TestPacket {
            data: vec![1, 2, 3, 4, 5],
        };
        let mut bit_buffer = BitBuffer::new();
        let result = invalid_packet.bit_serialize(&mut bit_buffer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);

        Ok(())
    }
}