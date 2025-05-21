use std::io::{Read, Write};
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use log::{debug};

// Bit I/O Module
pub mod bit_io {
    use std::io;
    use log::{debug, trace};

    pub trait BitWrite {
        fn write_bit(&mut self, bit: bool) -> io::Result<()>;
        fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()>;
        fn flush(&mut self) -> io::Result<()>;
        fn bit_pos(&self) -> usize;
    }

    pub trait BitRead {
        fn read_bit(&mut self) -> io::Result<bool>;
        fn read_bits(&mut self, bits: usize) -> io::Result<u64>;
        fn bit_pos(&self) -> usize;
    }

    pub struct BitBuffer {
        buffer: Vec<u8>,
        bit_pos: usize,
        read_pos: usize,
    }

    impl BitBuffer {
        pub fn new() -> Self {
            debug!("Creating new BitBuffer");
            BitBuffer {
                buffer: Vec::new(),
                bit_pos: 0,
                read_pos: 0,
            }
        }

        pub fn into_bytes(self) -> Vec<u8> {
            debug!("Converting BitBuffer to bytes: len={}", self.buffer.len());
            self.buffer
        }

        pub fn from_bytes(bytes: Vec<u8>) -> Self {
            debug!("Creating BitBuffer from bytes: len={}", bytes.len());
            BitBuffer {
                buffer: bytes,
                bit_pos: 0,
                read_pos: 0,
            }
        }
    }

    impl BitWrite for BitBuffer {
        fn write_bit(&mut self, bit: bool) -> io::Result<()> {
            let byte_pos = self.bit_pos / 8;
            let bit_offset = self.bit_pos % 8;
            trace!("Writing bit: {} at byte_pos: {}, bit_offset: {}, bit_pos: {}", bit, byte_pos, bit_offset, self.bit_pos);

            if byte_pos >= self.buffer.len() {
                trace!("Extending buffer to byte_pos: {}", byte_pos);
                self.buffer.push(0);
            }

            if bit {
                self.buffer[byte_pos] |= 1 << (7 - bit_offset);
            } else {
                self.buffer[byte_pos] &= !(1 << (7 - bit_offset));
            }

            self.bit_pos += 1;
            trace!("Buffer after write: {:?}", self.buffer);
            Ok(())
        }

        fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()> {
            if bits > 64 {
                debug!("Error: Attempted to write {} bits, exceeds 64", bits);
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Bits exceed 64"));
            }
            debug!("Writing {} bits: {} at bit_pos: {}", bits, value, self.bit_pos);
            for i in (0..bits).rev() {
                let bit = ((value >> i) & 1) != 0;
                self.write_bit(bit)?;
            }
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            debug!("Flushing BitBuffer at bit_pos: {}", self.bit_pos);
            while self.bit_pos % 8 != 0 {
                self.write_bit(false)?;
            }
            trace!("Buffer after flush: {:?}", self.buffer);
            Ok(())
        }

        fn bit_pos(&self) -> usize {
            self.bit_pos
        }
    }

    impl BitRead for BitBuffer {
        fn read_bit(&mut self) -> io::Result<bool> {
            let byte_pos = self.read_pos / 8;
            let bit_offset = self.read_pos % 8;
            trace!("Reading bit at byte_pos: {}, bit_offset: {}, read_pos: {}", byte_pos, bit_offset, self.read_pos);

            if byte_pos >= self.buffer.len() {
                debug!("Error: Buffer underflow at read_pos: {}", self.read_pos);
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer underflow"));
            }

            let bit = (self.buffer[byte_pos] & (1 << (7 - bit_offset))) != 0;
            self.read_pos += 1;
            trace!("Read bit: {}", bit);
            Ok(bit)
        }

        fn read_bits(&mut self, bits: usize) -> io::Result<u64> {
            if bits > 64 {
                debug!("Error: Attempted to read {} bits, exceeds 64", bits);
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Bits exceed 64"));
            }
            debug!("Reading {} bits at read_pos: {}", bits, self.read_pos);
            let mut value = 0;
            for _ in 0..bits {
                value = (value << 1) | (self.read_bit()? as u64);
            }
            trace!("Read bits value: {}", value);
            Ok(value)
        }

        fn bit_pos(&self) -> usize {
            self.read_pos
        }
    }
}

// Serialization Traits
pub trait BitSerialize {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()>;
}

pub trait BitDeserialize: Sized {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self>;
}

pub trait ByteAlignedSerialize {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()>;
}

pub trait ByteAlignedDeserialize: Sized {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(
        reader: &mut R,
    ) -> std::io::Result<Self>;
}

// Primitive Implementations for u8 and i8 (no endianness)
macro_rules! impl_primitive_single_byte {
    ($($t:ty, $bits:expr, $write:ident, $read:ident),*) => {
        $(
            impl BitSerialize for $t {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                    debug!("Serializing {}: {}", stringify!($t), *self);
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    debug!("Deserializing {}", stringify!($t));
                    let value = reader.read_bits($bits)?;
                    debug!("Deserialized {}: {}", stringify!($t), value);
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    debug!("Byte-aligned serializing {}: {}", stringify!($t), *self);
                    writer.$write(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    debug!("Byte-aligned deserializing {}", stringify!($t));
                    let value = reader.$read()?;
                    debug!("Deserialized {}: {}", stringify!($t), value);
                    Ok(value)
                }
            }
        )*
    };
}

// Primitive Implementations for multi-byte types (with LittleEndian)
macro_rules! impl_primitive_multi_byte {
    ($($t:ty, $bits:expr, $write:ident, $read:ident),*) => {
        $(
            impl BitSerialize for $t {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                    debug!("Serializing {}: {}", stringify!($t), *self);
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    debug!("Deserializing {}", stringify!($t));
                    let value = reader.read_bits($bits)?;
                    debug!("Deserialized {}: {}", stringify!($t), value);
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    debug!("Byte-aligned serializing {}: {}", stringify!($t), *self);
                    writer.$write::<LittleEndian>(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    debug!("Byte-aligned deserializing {}", stringify!($t));
                    let value = reader.$read::<LittleEndian>()?;
                    debug!("Deserialized {}: {}", stringify!($t), value);
                    Ok(value)
                }
            }
        )*
    };
}

impl_primitive_single_byte!(
    u8, 8, write_u8, read_u8,
    i8, 8, write_i8, read_i8
);

impl_primitive_multi_byte!(
    u16, 16, write_u16, read_u16,
    i16, 16, write_i16, read_i16,
    u32, 32, write_u32, read_u32,
    i32, 32, write_i32, read_i32,
    u64, 64, write_u64, read_u64,
    i64, 64, write_i64, read_i64,
    f32, 32, write_f32, read_f32,
    f64, 64, write_f64, read_f64
);

impl BitSerialize for bool {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        debug!("Serializing bool: {}", *self);
        writer.write_bit(*self)?;
        Ok(())
    }
}

impl BitDeserialize for bool {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        debug!("Deserializing bool");
        let value = reader.read_bit()?;
        debug!("Deserialized bool: {}", value);
        Ok(value)
    }
}

impl ByteAlignedSerialize for bool {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        debug!("Byte-aligned serializing bool: {}", *self);
        writer.write_u8(if *self { 1 } else { 0 })?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for bool {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        debug!("Byte-aligned deserializing bool");
        let value = reader.read_u8()?;
        debug!("Deserialized bool: {}", value != 0);
        Ok(value != 0)
    }
}

// Collection Implementations
impl<T: BitSerialize> BitSerialize for Vec<T> {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        debug!("Serializing Vec<T> with len: {}, max_len: {}, len_bits: {}", self.len(), max_len, len_bits);
        if self.len() > max_len {
            debug!("Error: Vector length {} exceeds max_len {}", self.len(), max_len);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Vector length exceeds max_len"));
        }
        writer.write_bits(self.len() as u64, len_bits)?;
        for (i, item) in self.iter().enumerate() {
            debug!("Serializing Vec<T> item {} at bit_pos: {}", i, writer.bit_pos());
            item.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Vec<T> {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        debug!("Deserializing Vec<T> with len_bits: {}", len_bits);
        let len = reader.read_bits(len_bits)? as usize;
        debug!("Deserialized Vec<T> length: {}", len);
        if len > max_len {
            debug!("Error: Vector length {} exceeds max_len {}", len, max_len);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Vector length exceeds max_len"));
        }
        let mut vec = Vec::with_capacity(len);
        for i in 0..len {
            debug!("Deserializing Vec<T> item {} at read_pos: {}", i, reader.bit_pos());
            vec.push(T::bit_deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl<T: ByteAlignedSerialize> ByteAlignedSerialize for Vec<T> {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        debug!("Byte-aligned serializing Vec<T> with len: {}", self.len());
        writer.write_u32::<LittleEndian>(self.len() as u32)?;
        for (i, item) in self.iter().enumerate() {
            debug!("Byte-aligned serializing Vec<T> item {}", i);
            item.byte_aligned_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize> ByteAlignedDeserialize for Vec<T> {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        debug!("Byte-aligned deserializing Vec<T>");
        let len = reader.read_u32::<LittleEndian>()? as usize;
        debug!("Deserialized Vec<T> length: {}", len);
        let mut vec = Vec::with_capacity(len);
        for i in 0..len {
            debug!("Byte-aligned deserializing Vec<T> item {}", i);
            vec.push(T::byte_aligned_deserialize(reader)?);
        }
        Ok(vec)
    }
}