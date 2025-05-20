use log::{debug, trace};
use std::io;

// Trait for serializing and deserializing game data (e.g., game state, inputs)
pub trait Serialize {
    fn serialize(&self, writer: &mut BitWriter) -> io::Result<()>;
    fn deserialize(reader: &mut BitReader) -> io::Result<Self> where Self: Sized;
}

// Default implementation for Vec<u8> as a fallback for raw or legacy data
impl Serialize for Vec<u8> {
    fn serialize(&self, writer: &mut BitWriter) -> io::Result<()> {
        writer.write_bits(self.len() as u64, 16)?;
        for &byte in self {
            writer.write_bits(byte as u64, 8)?;
        }
        Ok(())
    }

    fn deserialize(reader: &mut BitReader) -> io::Result<Self> {
        let len = reader.read_bits(16)? as usize;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(reader.read_bits(8)? as u8);
        }
        Ok(data)
    }
}

// Bit-level writer for efficient packet serialization
#[derive(Debug)]
pub struct BitWriter {
    buffer: Vec<u8>,
    current_byte: u8,
    bit_position: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        BitWriter {
            buffer: Vec::new(),
            current_byte: 0,
            bit_position: 0,
        }
    }

    // Writes a specified number of bits from a value to the buffer
    pub fn write_bits(&mut self, value: u64, bits: u8) -> io::Result<()> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot write more than 64 bits"));
        }
        let mut remaining_bits = bits;
        let mut current_value = value;

        while remaining_bits > 0 {
            let bits_to_write = (8 - self.bit_position).min(remaining_bits);
            let shift = remaining_bits.saturating_sub(bits_to_write);
            let bits_value = ((current_value >> shift) & ((1 << bits_to_write) - 1)) as u8;

            self.current_byte |= bits_value << (8 - self.bit_position - bits_to_write);
            self.bit_position += bits_to_write;
            remaining_bits -= bits_to_write;
            current_value &= (1 << remaining_bits) - 1;

            if self.bit_position >= 8 {
                self.buffer.push(self.current_byte);
                trace!("Flushed byte to buffer: {:02x}", self.current_byte);
                self.current_byte = 0;
                self.bit_position = 0;
            }
        }
        Ok(())
    }

    // Flushes any remaining bits to the buffer
    pub fn flush(&mut self) -> io::Result<()> {
        if self.bit_position > 0 {
            self.buffer.push(self.current_byte);
            trace!("Flushed final byte: {:02x}", self.current_byte);
            self.current_byte = 0;
            self.bit_position = 0;
        }
        Ok(())
    }

    // Returns the serialized buffer
    pub fn into_bytes(self) -> Vec<u8> {
        trace!("Returning buffer: {:02x?}", self.buffer);
        self.buffer
    }
}

// Bit-level reader for deserializing packets
#[derive(Debug)]
pub struct BitReader {
    buffer: Vec<u8>,
    position: usize,
    bit_position: u8,
}

impl BitReader {
    pub fn new(buffer: Vec<u8>) -> Self {
        trace!("Initialized BitReader with buffer: {:02x?}", buffer);
        BitReader {
            buffer,
            position: 0,
            bit_position: 0,
        }
    }

    // Reads a specified number of bits from the buffer
    pub fn read_bits(&mut self, bits: u8) -> io::Result<u64> {
        if bits > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot read more than 64 bits",
            ));
        }
        if bits == 0 {
            return Ok(0);
        }

        let total_bits_available = (self.buffer.len().saturating_sub(self.position)) * 8
            - self.bit_position as usize;
        if bits as usize > total_bits_available {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Not enough bits available",
            ));
        }

        let mut result: u64 = 0;
        let mut remaining_bits = bits;

        while remaining_bits > 0 {
            let bits_to_read = (8 - self.bit_position).min(remaining_bits);
            let byte = self.buffer.get(self.position).copied().unwrap_or(0);
            let shift = 8 - self.bit_position - bits_to_read;
            let mask = ((1 << bits_to_read) - 1) << shift;
            let value = ((byte & mask) >> shift) as u64;

            result = (result << bits_to_read) | value;
            self.bit_position += bits_to_read;
            remaining_bits -= bits_to_read;

            if self.bit_position >= 8 {
                self.position += 1;
                self.bit_position = 0;
            }
            debug!(
                "Processed {} bits: value {}, position {}, bit_position {}",
                bits_to_read, value, self.position, self.bit_position
            );
        }

        Ok(result)
    }

    // Returns the remaining buffer
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}