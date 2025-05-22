GBNet
GBNet is a Rust library for multiplayer game networking, designed for efficient, reliable, and high-performance real-time games. Its cornerstone is a powerful serialization system that supports both bit-packed and byte-aligned serialization/deserialization, optimized to minimize bandwidth while handling complex data structures. Paired with a robust networking stack, GBNet enables seamless multiplayer experiences with features like reliable messaging, state synchronization, and more.
Features

Advanced Serialization: Bit-packed and byte-aligned serialization/deserialization for structs, enums, and vectors, with fine-grained control over data encoding.
Bit-Level Control: Custom bit sizes via #[bits = N] and defaults with #[default_bits].
Byte Alignment: Align fields to byte boundaries using #[byte_align].
Field Skipping: Exclude fields with #[no_serialize].
Vector Length Limits: Cap vectors with #[max_len = N] or #[default_max_len].
Connections: Client-server and peer-to-peer connection management.
Reliable Messages: Guaranteed message delivery over UDP.
Large Data Transfer: Fragmentation for large payloads.
Packet Fragmentation: Efficient packet splitting and reassembly.
Packet Delivery: Ordered and reliable packet delivery.
State Synchronization: Game state syncing across clients.
Snapshot Compression: Compressed game state snapshots.
Snapshot Interpolation: Smooth client-side interpolation.
Deterministic Lockstep: Lockstep support for strategy games.
Congestion Avoidance: Network congestion prevention.
Fixed Timestep: Consistent game updates.

Installation
Add GBNet to your Cargo.toml:
[dependencies]
gbnet = { git = "https://github.com/yourusername/gbnet.git" }
gbnet_macros = { git = "https://github.com/yourusername/gbnet.git" }

Note: Replace yourusername with your GitHub username. Ensure the repository hosts gbnet and gbnet_macros.
Usage
GBNet’s serialization is powered by the NetworkSerialize macro, which derives bit- and byte-aligned serialization/deserialization for structs, enums, and vectors. It’s built for efficiency, allowing developers to optimize bandwidth with bit-level precision while supporting complex data like nested structs, enums with payloads, and length-capped vectors.
Serialization Capabilities

Bit-Packed Serialization: Encodes fields with custom bit counts using #[bits = N], e.g., a u8 as 6 bits to save bandwidth.
Byte-Aligned Serialization: Pads fields to byte boundaries with #[byte_align], ideal for byte-oriented protocols.
Annotations:
#[bits = N]: Sets bit count for a field (e.g., #[bits = 6] for a 6-bit u8).
#[byte_align]: Pads to a byte boundary before the annotated field.
#[no_serialize]: Skips a field, using its default value on deserialization.
#[max_len = N]: Caps vector lengths, encoding length in ceil(log2(N+1)) bits.
#[default_bits(type = N)]: Sets default bit sizes (e.g., u8 = 4, u16 = 10).
#[default_max_len = N]: Default max length for vectors without #[max_len].


Structs and Enums: Handles nested structs and enums with payloads (e.g., Running { speed: u8 }).
Vectors: Supports dynamic-length vectors with length caps.
Error Handling: Enforces bit size and vector length constraints, returning InvalidData errors for violations.
Testing: Comprehensive test suite validates primitives, vectors, enums, nested structs, alignment, and edge cases.

Examples
1. Bit-Packed Serialization with Custom Bits
Serialize a struct with custom bit sizes:
use gbnet::serialize::{BitSerialize, bit_io::BitBuffer};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize)]
struct PrimitivePacket {
    a: u8,           // 8 bits (macro default)
    #[bits = 6]
    b: u8,           // 6 bits
    c: bool,         // 1 bit
}

fn main() -> std::io::Result<()> {
    let packet = PrimitivePacket { a: 15, b: 50, c: true };
    let mut bit_buffer = BitBuffer::new();
    packet.bit_serialize(&mut bit_buffer)?;
    bit_buffer.flush()?;
    let bytes = bit_buffer.into_bytes();
    println!("Serialized bytes: {:?}", bytes); // [15, 202]
    Ok(())
}

Serializes a (8 bits: 00001111), b (6 bits: 110010), c (1 bit: 1), totaling 15 bits, padded to 16 bits.
2. Skipping Fields with #[no_serialize]
Exclude fields from serialization:
use gbnet::serialize::{BitSerialize, BitDeserialize, bit_io::BitBuffer};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize, Debug, PartialEq)]
struct NoSerializePacket {
    a: u8,                   // 8 bits
    #[no_serialize]
    b: u16,                  // Skipped
    c: bool,                 // 1 bit
    #[no_serialize]
    d: String,               // Skipped
}

fn main() -> std::io::Result<()> {
    let packet = NoSerializePacket {
        a: 42,
        b: 9999,
        c: true,
        d: "test".to_string(),
    };
    let mut bit_buffer = BitBuffer::new();
    packet.bit_serialize(&mut bit_buffer)?;
    bit_buffer.flush()?;
    let bytes = bit_buffer.into_bytes();
    
    let mut bit_buffer = BitBuffer::from_bytes(bytes);
    let deserialized = NoSerializePacket::bit_deserialize(&mut bit_buffer)?;
    assert_eq!(deserialized.a, 42);
    assert_eq!(deserialized.c, true);
    assert_eq!(deserialized.b, 0); // Default
    assert_eq!(deserialized.d, ""); // Default
    Ok(())
}

Serializes a (8 bits) and c (1 bit), totaling 9 bits, padded to 16 bits. Skipped fields use defaults.
3. Vector with Length Limits
Serialize a capped vector:
use gbnet::serialize::{BitSerialize, bit_io::BitBuffer};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize)]
#[default_max_len = 4]
struct VecPacket {
    #[max_len = 2]
    data: Vec<u8>,
}

fn main() -> std::io::Result<()> {
    let packet = VecPacket { data: vec![1, 2] };
    let mut bit_buffer = BitBuffer::new();
    packet.bit_serialize(&mut bit_buffer)?;
    bit_buffer.flush()?;
    let bytes = bit_buffer.into_bytes();
    println!("Serialized bytes: {:?}", bytes); // [128, 64, 128]
    Ok(())
}

Serializes length (2 bits: 10), then data=[1, 2] (8 bits each: 00000001, 00000010), totaling 18 bits, padded to 24 bits.
4. Byte Alignment
Align fields to byte boundaries:
use gbnet::serialize::{BitSerialize, bit_io::BitBuffer};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize)]
struct AlignPacket {
    a: bool,         // 1 bit
    #[byte_align]
    b: u8,           // 8 bits, after padding
}

fn main() -> std::io::Result<()> {
    let packet = AlignPacket { a: true, b: 10 };
    let mut bit_buffer = BitBuffer::new();
    packet.bit_serialize(&mut bit_buffer)?;
    bit_buffer.flush()?;
    let bytes = bit_buffer.into_bytes();
    println!("Serialized bytes: {:?}", bytes); // [128, 10]
    Ok(())
}

Serializes a (1 bit: 1), pads (7 bits: 0000000), then b (8 bits: 00001010), totaling 16 bits.
5. Complex Nested Structures
Serialize nested structs and enums:
use gbnet::serialize::{BitSerialize, bit_io::BitBuffer};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize, Debug)]
struct PlayerState {
    #[bits = 4]
    health: u8, // 4 bits
}

#[derive(NetworkSerialize, Debug)]
enum PlayerStatus {
    Idle,                     // 1 bit: 0
    Running { #[bits = 4] speed: u8 }, // 1 bit: 1 + 4 bits
}

#[derive(NetworkSerialize)]
#[default_bits(u8 = 4, u16 = 10, bool = 1)]
#[default_max_len = 16]
struct ComplexPacket {
    id: u16,                 // 10 bits
    #[bits = 6]
    flags: u8,               // 6 bits
    #[max_len = 4]
    players: Vec<PlayerState>, // 3 bits (length) + 4 bits per player
    status: PlayerStatus,    // 1 bit + payload
}

fn main() -> std::io::Result<()> {
    let packet = ComplexPacket {
        id: 1023,
        flags: 63,
        players: vec![PlayerState { health: 15 }, PlayerState { health: 10 }],
        status: PlayerStatus::Running { speed: 7 },
    };
    let mut bit_buffer = BitBuffer::new();
    packet.bit_serialize(&mut bit_buffer)?;
    bit_buffer.flush()?;
    let bytes = bit_buffer.into_bytes();
    println!("Serialized bytes: {:?}", bytes); // [255, 255, 95, 87, ...]
    Ok(())
}

Serializes id (10 bits), flags (6 bits), players length (3 bits), two PlayerState (4 bits each), and status (1 bit + 4 bits), padded to 64 bits.
