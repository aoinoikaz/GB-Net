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
    health: u8,                       // 4 bits
    ammo: u8,                         // 4 bits
    #[bits = 6]                      // 6 bits (override)
    energy: u8,
    is_active: bool,                  // 1 bit
    #[bits = 16]                     // 16 bits (override)
    x_pos: u16,
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
    #[max_len = 8]
    players: Vec<PlayerState>,        // 3 bits for length (ceil(log2(8)) = 3)
    status: PlayerStatus,             // 1 bit + payload
}

#[cfg(test)]
mod tests {
    use crate::serialize::{BitSerialize, BitDeserialize, bit_io::{BitBuffer, BitWrite}};
    use std::io::ErrorKind;
    use std::env;
    use log::debug;

    fn init_logger() {
        env::set_var("RUST_LOG", "debug,gbnet::serialize::bit_io=trace");
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_primitive_serialization() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize)]
        struct PrimitivePacket {
            a: u8,           // 4 bits (default)
            #[bits = 6]
            b: u8,           // 6 bits
            c: bool,         // 1 bit
        }
        let packet = PrimitivePacket { a: 15, b: 50, c: true };
        debug!("Starting test_primitive_serialization with packet: a={}, b={}, c={}", packet.a, packet.b, packet.c);
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        debug!("Serialized data: {:?}", bit_data);
        assert!(bit_data.len() <= 2, "Expected ~11 bits (2 bytes), got {} bytes", bit_data.len());
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = PrimitivePacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.a, 15, "Expected a=15, got {}", deserialized.a);
        assert_eq!(deserialized.b, 50, "Expected b=50, got {}", deserialized.b);
        assert_eq!(deserialized.c, true, "Expected c=true, got {}", deserialized.c);
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
        let valid = VecPacket { data: vec![1, 2] };
        debug!("Serializing valid VecPacket: {:?}", valid.data);
        let mut bit_buffer = BitBuffer::new();
        valid.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        debug!("Serialized data: {:?}", bit_data);
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = VecPacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.data, vec![1, 2], "Expected data=[1, 2], got {:?}", deserialized.data);

        // Invalid case
        let invalid = VecPacket { data: vec![1, 2, 3] };
        debug!("Attempting to serialize invalid VecPacket: {:?}", invalid.data);
        let mut bit_buffer = BitBuffer::new();
        let result = invalid.bit_serialize(&mut bit_buffer);
        assert!(result.is_err(), "Expected error for vector length > max_len");
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData, "Expected InvalidData error");
        Ok(())
    }

    #[test]
    fn test_byte_alignment() -> std::io::Result<()> {
        init_logger();
        #[derive(NetworkSerialize)]
        struct AlignPacket {
            a: bool,         // 1 bit
            #[byte_align]
            b: u8,           // 4 bits, after padding
        }
        let packet = AlignPacket { a: true, b: 10 };
        debug!("Starting test_byte_alignment with packet: a={}, b={}", packet.a, packet.b);
        let mut bit_buffer = BitBuffer::new();
        packet.bit_serialize(&mut bit_buffer)?;
        bit_buffer.flush()?;
        let bit_data = bit_buffer.into_bytes();
        debug!("Serialized data: {:?}", bit_data);
        assert_eq!(bit_data.len(), 2, "Expected 12 bits (1 + 7 padding + 4) = 2 bytes, got {} bytes", bit_data.len());
        let mut bit_buffer = BitBuffer::from_bytes(bit_data);
        let deserialized = AlignPacket::bit_deserialize(&mut bit_buffer)?;
        assert_eq!(deserialized.a, true, "Expected a=true, got {}", deserialized.a);
        assert_eq!(deserialized.b, 10, "Expected b=10, got {}", deserialized.b);
        Ok(())
    }
}