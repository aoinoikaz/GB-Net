# GBNet

**GBNet** is a Rust library for multiplayer game networking, designed to provide efficient, reliable, and flexible networking for real-time games. It combines advanced bit-packed serialization with a robust networking stack to optimize bandwidth and ensure smooth gameplay. While the core serialization features are production-ready, the Reliable UDP (RUDP) components are currently a **work in progress (WIP)**.

## Features

- **Advanced Bit-Packed Serialization**: Highly optimized serialization with bit-level control, supporting custom bit sizes, byte alignment, and skipping fields. (Fully implemented)
- **Connections**: Manage client-server and peer-to-peer connections. (WIP)
- **Reliable Messages**: Ensure message delivery over UDP. (WIP)
- **Large Data Transfer**: Handle large payloads with fragmentation. (WIP)
- **Packet Fragmentation**: Split and reassemble packets efficiently. (WIP)
- **Packet Delivery**: Guarantee delivery order and reliability. (WIP)
- **State Synchronization**: Sync game state across clients. (WIP)
- **Snapshot Compression**: Compress game state snapshots. (WIP)
- **Snapshot Interpolation**: Smooth client-side interpolation. (WIP)
- **Deterministic Lockstep**: Support for lockstep-based games. (WIP)
- **Congestion Avoidance**: Prevent network congestion. (WIP)
- **Fixed Timestep**: Consistent game updates. (WIP)

## Why GBNet?

GBNet stands out for its **bit-packed serialization**, which minimizes bandwidth usage by allowing fine-grained control over data encoding. The `NetworkSerialize` macro enables developers to define custom bit sizes for fields, align data to byte boundaries, and skip non-essential fields, making it ideal for low-latency, high-performance games. While the RUDP stack is still in development, the serialization system is robust and ready for use in game projects.

## Installation

Add GBNet to your project by including it in your `Cargo.toml`:

```toml
[dependencies]
gbnet = { git = "https://github.com/yourusername/gbnet.git" }
gbnet_macros = { git = "https://github.com/yourusername/gbnet.git" }