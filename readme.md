# GBNet

GBNet is a high-performance Rust library for multiplayer game networking, featuring an advanced bit-packed serialization system and reliable UDP networking stack. Designed for real-time games, GBNet minimizes bandwidth usage while providing robust networking primitives for building multiplayer experiences.

## Features

### 🎯 Advanced Serialization System
- **Bit-Packed Encoding**: Serialize data with bit-level precision to minimize bandwidth
- **Flexible Field Control**: Custom bit sizes via `#[bits = N]` attributes
- **Smart Defaults**: Set project-wide defaults with `#[default_bits(type = N)]`
- **Selective Serialization**: Skip fields with `#[no_serialize]`
- **Byte Alignment**: Force byte boundaries with `#[byte_align]`
- **Vector Optimization**: Cap vector lengths with `#[max_len = N]` for efficient encoding

### 🌐 Robust Networking Stack
- **Reliable UDP**: Message delivery guarantees over UDP
- **Connection Management**: Secure handshake protocol with challenge-response authentication
- **Channel System**: Multiple logical channels with configurable reliability
- **Packet Fragmentation**: Automatic splitting and reassembly of large messages
- **Congestion Control**: Built-in flow control and congestion avoidance
- **Sequence Management**: Proper handling of out-of-order packets

### 🚀 Performance Features
- **Zero-Copy Design**: Minimal allocations in hot paths
- **Optimized Bit Operations**: Fast bit reading/writing with byte-aligned fast paths
- **Configurable Buffers**: Tune memory usage for your specific needs
- **Statistics Tracking**: Built-in performance metrics and diagnostics

## Installation

Add GBNet to your `Cargo.toml`:

```toml
[dependencies]
gbnet = { git = "https://github.com/gondolabros/gbnet.git" }
gbnet_macros = { git = "https://github.com/gondolabros/gbnet.git" }
```

## Quick Start

### Basic Serialization

```rust
use gbnet::{NetworkSerialize, BitSerialize, BitDeserialize, BitBuffer};

#[derive(NetworkSerialize, Debug, PartialEq)]
struct PlayerUpdate {
    #[bits = 10]
    x: u16,      // 0-1023 range
    #[bits = 10] 
    y: u16,      // 0-1023 range
    #[bits = 7]
    health: u8,  // 0-127 range
    moving: bool,// 1 bit
}

fn main() -> std::io::Result<()> {
    let update = PlayerUpdate {
        x: 512,
        y: 768,
        health: 100,
        moving: true,
    };
    
    // Serialize
    let mut buffer = BitBuffer::new();
    update.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(true)?;
    
    // Deserialize
    let mut buffer = BitBuffer::from_bytes(bytes);
    let decoded = PlayerUpdate::bit_deserialize(&mut buffer)?;
    
    assert_eq!(update, decoded);
    Ok(())
}
```

### Network Communication

```rust
use gbnet::{UdpSocket, Connection, NetworkConfig};
use std::net::SocketAddr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a UDP socket
    let mut socket = UdpSocket::bind("127.0.0.1:0")?;
    
    // Configure networking
    let config = NetworkConfig::default();
    
    // Create a connection
    let remote_addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let local_addr = socket.local_addr()?;
    let mut connection = Connection::new(config, local_addr, remote_addr);
    
    // Connect to server
    connection.connect()?;
    
    // Send data on a channel
    connection.send(0, b"Hello, server!", true)?;
    
    // Update connection (handles retries, timeouts, etc.)
    connection.update(&mut socket)?;
    
    // Receive data
    if let Some(data) = connection.receive(0) {
        println!("Received: {:?}", data);
    }
    
    Ok(())
}
```

## Serialization Attributes

### Struct/Enum Attributes
- `#[default_bits(u8 = 4, u16 = 10)]` - Set default bit sizes for types
- `#[default_max_len = 100]` - Default max length for vectors
- `#[bits = 4]` - For enums, bits used for variant discriminant

### Field Attributes
- `#[bits = N]` - Use N bits for this field (must fit the value range)
- `#[byte_align]` - Align to byte boundary before this field
- `#[no_serialize]` - Skip field during serialization (uses Default on deserialization)
- `#[max_len = N]` - Maximum length for Vec fields

## Examples

### Efficient Game State

```rust
#[derive(NetworkSerialize)]
#[default_bits(u8 = 4, u16 = 12)]
struct GameState {
    tick: u32,                          // 32 bits
    #[max_len = 32]
    players: Vec<PlayerData>,           // 5 bits length + data
    #[bits = 3]
    game_mode: u8,                      // 3 bits (0-7)
    #[byte_align]
    checksum: u16,                      // 16 bits, byte-aligned
}

#[derive(NetworkSerialize)]
struct PlayerData {
    id: u8,                             // 4 bits (from default_bits)
    #[bits = 10]
    x: u16,                             // 10 bits
    #[bits = 10]
    y: u16,                             // 10 bits
    state: PlayerState,                 // Variable (enum)
}

#[derive(NetworkSerialize)]
#[bits = 2]  // 4 variants = 2 bits
enum PlayerState {
    Idle,
    Walking { #[bits = 8] speed: u8 },
    Running { #[bits = 8] speed: u8 },
    Dead,
}
```

### Reliable Messaging

```rust
use gbnet::{Channel, ChannelConfig, Reliability, Ordering};

// Configure a reliable, ordered channel for chat messages
let chat_config = ChannelConfig {
    reliability: Reliability::Reliable,
    ordering: Ordering::Ordered,
    max_message_size: 1024,
    message_buffer_size: 100,
    block_on_full: true,
};

let mut chat_channel = Channel::new(0, chat_config);

// Send a chat message
chat_channel.send(b"Hello, world!", true)?;

// Configure an unreliable channel for position updates  
let position_config = ChannelConfig {
    reliability: Reliability::Unreliable,
    ordering: Ordering::Unordered,
    ..Default::default()
};

let mut position_channel = Channel::new(1, position_config);
```

## Architecture

GBNet is organized into several key modules:

- **`serialize`**: Bit-packed and byte-aligned serialization traits and implementations
- **`packet`**: Core packet structures and protocol definitions
- **`connection`**: Connection state management and handshake protocol
- **`reliability`**: Reliable delivery, acknowledgments, and retransmission
- **`channel`**: Multiple logical channels with different delivery guarantees
- **`socket`**: Platform-agnostic UDP socket wrapper

## Performance Tips

1. **Use appropriate bit sizes**: Don't use more bits than necessary
2. **Group small fields**: Multiple bools can share a byte efficiently  
3. **Consider alignment**: Use `#[byte_align]` for fields that benefit from it
4. **Set reasonable max lengths**: Smaller max_len values use fewer bits
5. **Profile your packets**: Use the built-in statistics to optimize

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues for bugs and feature requests.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
