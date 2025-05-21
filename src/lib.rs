use std::io;

pub use gbnet_derive::{Serialize, Deserialize};
pub mod bit_io;
pub mod instant;
pub mod packet;
pub mod serialize;

/// Trait for serializing a type into a `BitWriter`.
pub trait Serialize {
    fn serialize(&self, writer: bit_io::BitWriter) -> io::Result<bit_io::BitWriter>;
}

/// Trait for deserializing a type from a `BitReader`.
pub trait Deserialize: Sized {
    fn deserialize(reader: bit_io::BitReader) -> io::Result<(Self, bit_io::BitReader)>;
}