use std::io::{self, Read, Write};
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use log::debug;

pub mod bit_io {
    use std::io;
    use log::{debug, trace};

    pub trait BitWrite {
        fn write_bit(&mut self, bit: bool) -> io::Result<()>;
        fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()>;
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
        unpadded_length: usize, // Tracks bits before padding
    }

    impl BitBuffer {
        pub fn new() -> Self {
            BitBuffer {
                buffer: Vec::new(),
                bit_pos: 0,
                read_pos: 0,
                unpadded_length: 0,
            }
        }

        pub fn unpadded_length(&self) -> usize {
            self.unpadded_length
        }

        pub fn into_bytes(mut self, pad_to_byte: bool) -> io::Result<Vec<u8>> {
            self.flush(pad_to_byte)?;
            Ok(self.buffer)
        }

        pub fn from_bytes(bytes: Vec<u8>) -> Self {
            BitBuffer {
                buffer: bytes,
                bit_pos: 0,
                read_pos: 0,
                unpadded_length: 0,
            }
        }

        pub fn to_bit_string(&self, bit_length: usize) -> String {
            let mut bit_string = String::new();
            let mut bits_written = 0;
            for (i, &byte) in self.buffer.iter().enumerate() {
                for j in (0..8).rev() {
                    if bits_written < bit_length {
                        let bit = (byte >> j) & 1;
                        bit_string.push_str(&bit.to_string());
                        bits_written += 1;
                    } else {
                        break;
                    }
                }
                if bits_written < bit_length && i < self.buffer.len() - 1 {
                    bit_string.push(' ');
                }
            }
            bit_string.trim().to_string()
        }

        fn flush(&mut self, pad_to_byte: bool) -> io::Result<()> {
            if pad_to_byte {
                while self.bit_pos % 8 != 0 {
                    self.write_bit(false)?;
                }
            }
            Ok(())
        }

        // OPTIMIZATION: Fast path for byte-aligned writes
        fn write_bytes_fast(&mut self, value: u64, bytes: usize) -> io::Result<()> {
            // Ensure we have enough space
            self.buffer.reserve(bytes);
            
            // Write bytes from most significant to least significant
            for i in 0..bytes {
                let byte = ((value >> (8 * (bytes - 1 - i))) & 0xFF) as u8;
                self.buffer.push(byte);
                trace!("Wrote byte {}: {}", i, byte);
            }
            
            self.bit_pos += bytes * 8;
            self.unpadded_length += bytes * 8;
            
            Ok(())
        }

        // OPTIMIZATION: Write multiple bits per operation
        fn write_bits_optimized(&mut self, value: u64, bits: usize) -> io::Result<()> {
            let mut remaining_bits = bits;
            let mut val = value;
            
            while remaining_bits > 0 {
                let byte_pos = self.bit_pos / 8;
                let bit_offset = self.bit_pos % 8;
                let bits_available_in_byte = 8 - bit_offset;
                let bits_to_write = remaining_bits.min(bits_available_in_byte);
                
                // Ensure buffer has space
                while byte_pos >= self.buffer.len() {
                    self.buffer.push(0);
                }
                
                // Extract the bits we want to write (from the most significant bits of remaining)
                let shift = if remaining_bits >= bits_to_write { remaining_bits - bits_to_write } else { 0 };
                let bits_to_write_val = if shift < 64 {
                    (val >> shift) & ((1u64 << bits_to_write) - 1)
                } else {
                    0
                };
                
                // Write these bits to the current byte
                let byte_shift = bits_available_in_byte - bits_to_write;
                self.buffer[byte_pos] |= (bits_to_write_val as u8) << byte_shift;
                
                trace!(
                    "Wrote {} bits (value {}) to byte {} at offset {}", 
                    bits_to_write, bits_to_write_val, byte_pos, bit_offset
                );
                
                // Update counters
                self.bit_pos += bits_to_write;
                remaining_bits -= bits_to_write;
                
                // Mask off the bits we just wrote
                val &= if remaining_bits > 0 && remaining_bits < 64 {
                    (1u64 << remaining_bits) - 1
                } else if remaining_bits == 0 {
                    0
                } else {
                    u64::MAX
                };
            }
            
            self.unpadded_length += bits;
            Ok(())
        }

        // OPTIMIZATION: Fast path for byte-aligned reads
        fn read_bytes_fast(&mut self, bytes: usize) -> io::Result<u64> {
            let start_byte = self.read_pos / 8;
            let end_byte = start_byte + bytes;
            
            if end_byte > self.buffer.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Not enough bytes to read"
                ));
            }
            
            let mut value = 0u64;
            for i in 0..bytes {
                let byte = self.buffer[start_byte + i];
                value |= (byte as u64) << (8 * (bytes - 1 - i));
                trace!("Read byte {}: {}", i, byte);
            }
            
            self.read_pos += bytes * 8;
            Ok(value)
        }

        // OPTIMIZATION: Read multiple bits per operation
        fn read_bits_optimized(&mut self, bits: usize) -> io::Result<u64> {
            let mut remaining_bits = bits;
            let mut value = 0u64;
            
            while remaining_bits > 0 {
                let byte_pos = self.read_pos / 8;
                let bit_offset = self.read_pos % 8;
                let bits_available_in_byte = 8 - bit_offset;
                let bits_to_read = remaining_bits.min(bits_available_in_byte);
                
                if byte_pos >= self.buffer.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Buffer underflow during optimized read"
                    ));
                }
                
                // Extract the bits we want from the current byte
                let byte_shift = bits_available_in_byte - bits_to_read;
                let mask = if bits_to_read >= 8 { 0xFF } else { (1u8 << bits_to_read) - 1 };
                let bits_value = (self.buffer[byte_pos] >> byte_shift) & mask;
                
                // Add these bits to our result (they go in the most significant position of remaining bits)
                let result_shift = remaining_bits - bits_to_read;
                if result_shift < 64 {  // Prevent shift overflow
                    value |= (bits_value as u64) << result_shift;
                }
                
                self.read_pos += bits_to_read;
                remaining_bits -= bits_to_read;
            }
            
            Ok(value)
        }
    }

    impl BitWrite for BitBuffer {
        fn write_bit(&mut self, bit: bool) -> io::Result<()> {
            let byte_pos = self.bit_pos / 8;
            let bit_offset = self.bit_pos % 8;

            if byte_pos >= self.buffer.len() {
                self.buffer.push(0);
            }

            if bit {
                self.buffer[byte_pos] |= 1 << (7 - bit_offset);
            } else {
                self.buffer[byte_pos] &= !(1 << (7 - bit_offset));
            }

            self.bit_pos += 1;
            self.unpadded_length += 1;
            Ok(())
        }

        fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()> {
            if bits > 64 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Bits exceed 64"));
            }
            if bits == 0 {
                return Ok(());
            }

            let val = value & ((1u64 << bits) - 1); // Mask to ensure only `bits` are used

            // FAST PATH: Check if we can write whole bytes efficiently
            if self.bit_pos % 8 == 0 && bits % 8 == 0 {
                return self.write_bytes_fast(val, bits / 8);
            }

            // OPTIMIZED PATH: Write multiple bits per operation when possible
            self.write_bits_optimized(val, bits)
        }

        fn bit_pos(&self) -> usize {
            self.bit_pos
        }
    }

    impl BitRead for BitBuffer {
        fn read_bit(&mut self) -> io::Result<bool> {
            let byte_pos = self.read_pos / 8;
            let bit_offset = self.read_pos % 8;

            if byte_pos >= self.buffer.len() {
                debug!("Error: Buffer underflow at read_pos: {}", self.read_pos);
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Buffer underflow",
                ));
            }

            let bit = (self.buffer[byte_pos] & (1 << (7 - bit_offset))) != 0;
            self.read_pos += 1;
            Ok(bit)
        }

        fn read_bits(&mut self, bits: usize) -> io::Result<u64> {
            if bits > 64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Bits exceed 64",
                ));
            }
            if bits == 0 {
                return Ok(0);
            }

            // FAST PATH: Check if we can read whole bytes efficiently
            if self.read_pos % 8 == 0 && bits % 8 == 0 {
                return self.read_bytes_fast(bits / 8);
            }

            // OPTIMIZED PATH: Read multiple bits per operation when possible
            self.read_bits_optimized(bits)
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
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.read_bits($bits)?;
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.$write(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.$read()?;
                    Ok(value)
                }
            }
        )*
    };
}

// Primitive Implementations for multi-byte integer types (with LittleEndian)
macro_rules! impl_primitive_multi_byte {
    ($($t:ty, $bits:expr, $write:ident, $read:ident),*) => {
        $(
            impl BitSerialize for $t {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.read_bits($bits)?;
                    Ok(value as $t)
                }
            }
            impl ByteAlignedSerialize for $t {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.$write::<LittleEndian>(*self)?;
                    Ok(())
                }
            }
            impl ByteAlignedDeserialize for $t {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                    let value = reader.$read::<LittleEndian>()?;
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
    i64, 64, write_i64, read_i64
);

// FIXED: Float implementations using to_bits/from_bits for proper IEEE 754 serialization
impl BitSerialize for f32 {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bits(self.to_bits() as u64, 32)?;
        Ok(())
    }
}

impl BitDeserialize for f32 {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let bits = reader.read_bits(32)? as u32;
        let value = f32::from_bits(bits);
        Ok(value)
    }
}

impl ByteAlignedSerialize for f32 {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_f32::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for f32 {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let value = reader.read_f32::<LittleEndian>()?;
        Ok(value)
    }
}

impl BitSerialize for f64 {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bits(self.to_bits(), 64)?;
        Ok(())
    }
}

impl BitDeserialize for f64 {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let bits = reader.read_bits(64)?;
        let value = f64::from_bits(bits);
        Ok(value)
    }
}

impl ByteAlignedSerialize for f64 {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_f64::<LittleEndian>(*self)?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for f64 {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let value = reader.read_f64::<LittleEndian>()?;
        Ok(value)
    }
}

impl BitSerialize for bool {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_bit(*self)?;
        Ok(())
    }
}

impl BitDeserialize for bool {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        let value = reader.read_bit()?;
        Ok(value)
    }
}

impl ByteAlignedSerialize for bool {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> io::Result<()> {
        writer.write_u8(if *self { 1 } else { 0 })?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for bool {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(
        reader: &mut R,
    ) -> io::Result<Self> {
        let value = reader.read_u8()?;
        Ok(value != 0)
    }
}

// String implementations
impl BitSerialize for String {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits for length
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        
        if self.len() > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("String length {} exceeds max_len {}", self.len(), max_len),
            ));
        }
        
        writer.write_bits(self.len() as u64, len_bits)?;
        for byte in self.as_bytes() {
            writer.write_bits(*byte as u64, 8)?;
        }
        Ok(())
    }
}

impl BitDeserialize for String {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits for length
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        let len = reader.read_bits(len_bits)? as usize;
        
        if len > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("String length {} exceeds max_len {}", len, max_len),
            ));
        }
        
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(reader.read_bits(8)? as u8);
        }
        
        String::from_utf8(bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })
    }
}

impl ByteAlignedSerialize for String {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LittleEndian>(self.len() as u32)?;
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl ByteAlignedDeserialize for String {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<LittleEndian>()? as usize;
        let mut bytes = vec![0u8; len];
        reader.read_exact(&mut bytes)?;
        
        String::from_utf8(bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })
    }
}

// Fixed-size array implementations - FIXED unused variable warnings
macro_rules! impl_array {
    ($($n:expr),*) => {
        $(
            impl<T: BitSerialize> BitSerialize for [T; $n] {
                fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
                    for item in self.iter() {
                        item.bit_serialize(writer)?;
                    }
                    Ok(())
                }
            }

            impl<T: BitDeserialize + Default + Copy> BitDeserialize for [T; $n] {
                fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
                    let mut array = [T::default(); $n];
                    for i in 0..$n {
                        array[i] = T::bit_deserialize(reader)?;
                    }
                    Ok(array)
                }
            }

            impl<T: ByteAlignedSerialize> ByteAlignedSerialize for [T; $n] {
                fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
                    for item in self.iter() {
                        item.byte_aligned_serialize(writer)?;
                    }
                    Ok(())
                }
            }

            impl<T: ByteAlignedDeserialize + Default + Copy> ByteAlignedDeserialize for [T; $n] {
                fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
                    let mut array = [T::default(); $n];
                    for i in 0..$n {
                        array[i] = T::byte_aligned_deserialize(reader)?;
                    }
                    Ok(array)
                }
            }
        )*
    };
}

impl_array!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 20, 24, 32, 48, 64, 96, 128, 256, 512, 1024);

// Tuple implementations
impl<T: BitSerialize, U: BitSerialize> BitSerialize for (T, U) {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize> BitDeserialize for (T, U) {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((T::bit_deserialize(reader)?, U::bit_deserialize(reader)?))
    }
}

impl<T: ByteAlignedSerialize, U: ByteAlignedSerialize> ByteAlignedSerialize for (T, U) {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize, U: ByteAlignedDeserialize> ByteAlignedDeserialize for (T, U) {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((T::byte_aligned_deserialize(reader)?, U::byte_aligned_deserialize(reader)?))
    }
}

impl<T: BitSerialize, U: BitSerialize, V: BitSerialize> BitSerialize for (T, U, V) {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        self.2.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize, V: BitDeserialize> BitDeserialize for (T, U, V) {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((T::bit_deserialize(reader)?, U::bit_deserialize(reader)?, V::bit_deserialize(reader)?))
    }
}

impl<T: ByteAlignedSerialize, U: ByteAlignedSerialize, V: ByteAlignedSerialize> ByteAlignedSerialize for (T, U, V) {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        self.2.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize, U: ByteAlignedDeserialize, V: ByteAlignedDeserialize> ByteAlignedDeserialize for (T, U, V) {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((T::byte_aligned_deserialize(reader)?, U::byte_aligned_deserialize(reader)?, V::byte_aligned_deserialize(reader)?))
    }
}

// 4-tuple
impl<T: BitSerialize, U: BitSerialize, V: BitSerialize, W: BitSerialize> BitSerialize for (T, U, V, W) {
    fn bit_serialize<Wr: bit_io::BitWrite>(&self, writer: &mut Wr) -> io::Result<()> {
        self.0.bit_serialize(writer)?;
        self.1.bit_serialize(writer)?;
        self.2.bit_serialize(writer)?;
        self.3.bit_serialize(writer)?;
        Ok(())
    }
}

impl<T: BitDeserialize, U: BitDeserialize, V: BitDeserialize, W: BitDeserialize> BitDeserialize for (T, U, V, W) {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        Ok((T::bit_deserialize(reader)?, U::bit_deserialize(reader)?, V::bit_deserialize(reader)?, W::bit_deserialize(reader)?))
    }
}

impl<T: ByteAlignedSerialize, U: ByteAlignedSerialize, V: ByteAlignedSerialize, W: ByteAlignedSerialize> ByteAlignedSerialize for (T, U, V, W) {
    fn byte_aligned_serialize<Wr: Write + WriteBytesExt>(&self, writer: &mut Wr) -> io::Result<()> {
        self.0.byte_aligned_serialize(writer)?;
        self.1.byte_aligned_serialize(writer)?;
        self.2.byte_aligned_serialize(writer)?;
        self.3.byte_aligned_serialize(writer)?;
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize, U: ByteAlignedDeserialize, V: ByteAlignedDeserialize, W: ByteAlignedDeserialize> ByteAlignedDeserialize for (T, U, V, W) {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> io::Result<Self> {
        Ok((T::byte_aligned_deserialize(reader)?, U::byte_aligned_deserialize(reader)?, V::byte_aligned_deserialize(reader)?, W::byte_aligned_deserialize(reader)?))
    }
}

impl<T: BitSerialize> BitSerialize for Vec<T> {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        if self.len() > max_len {
            debug!(
                "Error: Vector length {} exceeds max_len {}",
                self.len(),
                max_len
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Vector length {} exceeds max_len {}", self.len(), max_len),
            ));
        }
        writer.write_bits(self.len() as u64, len_bits)?;
        for item in self.iter() {
            item.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Vec<T> {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> io::Result<Self> {
        const DEFAULT_MAX_LEN: usize = 65535; // 16 bits
        let max_len = DEFAULT_MAX_LEN;
        let len_bits = (max_len as f64).log2().ceil() as usize;
        let len = reader.read_bits(len_bits)? as usize;
        if len > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Vector length {} exceeds max_len {}", len, max_len),
            ));
        }
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::bit_deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl<T: ByteAlignedSerialize> ByteAlignedSerialize for Vec<T> {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> io::Result<()> {
        writer.write_u32::<LittleEndian>(self.len() as u32)?;
        for item in self.iter() {
            item.byte_aligned_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize> ByteAlignedDeserialize for Vec<T> {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(
        reader: &mut R,
    ) -> io::Result<Self> {
        let len = reader.read_u32::<LittleEndian>()? as usize;
        debug!("Deserialized Vec<T> length: {}", len);
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::byte_aligned_deserialize(reader)?);
        }
        Ok(vec)
    }
}

// Option<T> implementations
impl<T: BitSerialize> BitSerialize for Option<T> {
    fn bit_serialize<W: bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Some(value) => {
                writer.write_bit(true)?;  // 1 bit for Some
                value.bit_serialize(writer)?;
            }
            None => {
                writer.write_bit(false)?; // 1 bit for None
            }
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Option<T> {
    fn bit_deserialize<R: bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
        let has_value = reader.read_bit()?;
        if has_value {
            Ok(Some(T::bit_deserialize(reader)?))
        } else {
            Ok(None)
        }
    }
}

impl<T: ByteAlignedSerialize> ByteAlignedSerialize for Option<T> {
    fn byte_aligned_serialize<W: Write + WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Some(value) => {
                writer.write_u8(1)?;
                value.byte_aligned_serialize(writer)?;
            }
            None => {
                writer.write_u8(0)?;
            }
        }
        Ok(())
    }
}

impl<T: ByteAlignedDeserialize> ByteAlignedDeserialize for Option<T> {
    fn byte_aligned_deserialize<R: Read + ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
        let has_value = reader.read_u8()? != 0;
        if has_value {
            Ok(Some(T::byte_aligned_deserialize(reader)?))
        } else {
            Ok(None)
        }
    }
}