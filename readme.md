# GBNet

**GBNet** is a Rust library for multiplayer game networking, designed for efficient, reliable, and high-performance real-time games. Its cornerstone is a powerful serialization system that supports both bit-packed and byte-aligned serialization/deserialization, optimized to minimize bandwidth while handling complex data structures. Paired with a robust networking stack, GBNet enables seamless multiplayer experiences with features like reliable messaging, state synchronization, and more.

## Features

- **Advanced Serialization**: Bit-packed and byte-aligned serialization/deserialization for structs, enums, and vectors, with fine-grained control over data encoding.
- **Bit-Level Control**: Custom bit sizes via `#[bits = N]` and defaults with `#[default_bits]`.
- **Byte Alignment**: Align fields to byte boundaries using `#[byte_align]`.
- **Field Skipping**: Exclude fields with `#[no_serialize]`.
- **Vector Length Limits**: Cap vectors with `#[max_len = N]` or `#[default_max_len]`.
- **Connections**: Client-server and peer-to-peer connection management.
- **Reliable Messages**: Guaranteed message delivery over UDP.
- **Large Data Transfer**: Fragmentation for large payloads.
- **Packet Fragmentation**: Efficient packet splitting and reassembly.
- **Packet Delivery**: Ordered and reliable packet delivery.
- **State Synchronization**: Game state syncing across clients.
- **Snapshot Compression**: Compressed game state snapshots.
- **Snapshot Interpolation**: Smooth client-side interpolation.
- **Deterministic Lockstep**: Lockstep support for strategy games.
- **Congestion Avoidance**: Network congestion prevention.
- **Fixed Timestep**: Consistent game updates.

## Installation

Add GBNet to your `Cargo.toml`:

```toml
[dependencies]
gbnet = { git = "https://github.com/yourusername/gbnet.git" }
gbnet_macros = { git = "https://github.com/yourusername/gbnet.git" }