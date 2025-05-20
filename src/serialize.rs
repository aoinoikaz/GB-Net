use std::io;

// Trait for serializable types
pub trait Serialize {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter>;
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> where Self: Sized;
}

// Functional bit-level writer
#[derive(Debug, Clone, PartialEq)]
pub struct BitWriter {
    buffer: Vec<u8>,
    bit_offset: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        BitWriter {
            buffer: Vec::with_capacity(128), // Pre-allocate
            bit_offset: 0,
        }
    }

    pub fn write_bits(self, value: u64, num_bits: u8) -> io::Result<Self> {
        if num_bits == 0 || num_bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid number of bits"));
        }

        let mut writer = self;
        let mut bits_remaining = num_bits as usize;
        let value = value & ((1u64 << num_bits) - 1);

        while bits_remaining > 0 {
            let byte_index = writer.bit_offset / 8;
            let bit_index = writer.bit_offset % 8;
            let bits_to_write = (8 - bit_index).min(bits_remaining);

            if byte_index >= writer.buffer.len() {
                writer.buffer.push(0);
            }

            let shift = (8 - bit_index - bits_to_write) as u64;
            let mask = ((1u64 << bits_to_write) - 1) << shift;
            let bits = (value >> (bits_remaining - bits_to_write)) & ((1u64 << bits_to_write) - 1);
            writer.buffer[byte_index] = (writer.buffer[byte_index] & !(mask as u8)) | ((bits << shift) as u8);

            writer.bit_offset += bits_to_write;
            bits_remaining -= bits_to_write;
        }

        Ok(writer)
    }

    pub fn write_bit(self, value: bool) -> io::Result<Self> {
        self.write_bits(value as u64, 1)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

// Functional bit-level reader
#[derive(Debug, Clone, PartialEq)]
pub struct BitReader {
    buffer: Vec<u8>,
    bit_offset: usize,
}

impl BitReader {
    pub fn new(buffer: Vec<u8>) -> Self {
        BitReader {
            buffer,
            bit_offset: 0,
        }
    }

    pub fn read_bits(self, num_bits: u8) -> io::Result<(u64, Self)> {
        if num_bits == 0 || num_bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid number of bits"));
        }
        if self.bit_offset + num_bits as usize > self.buffer.len() * 8 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer underflow"));
        }

        let mut result = 0u64;
        let mut bits_remaining = num_bits as usize;
        let mut reader = self;

        while bits_remaining > 0 {
            let byte_index = reader.bit_offset / 8;
            let bit_index = reader.bit_offset % 8;
            let bits_to_read = (8 - bit_index).min(bits_remaining);

            let shift = (8 - bit_index - bits_to_read) as u64;
            let mask = ((1u64 << bits_to_read) - 1) << shift;
            let bits = ((reader.buffer[byte_index] as u64 & mask) >> shift) as u64;

            result = (result << bits_to_read) | bits;
            reader.bit_offset += bits_to_read;
            bits_remaining -= bits_to_read;
        }

        Ok((result, reader))
    }

    pub fn read_bit(self) -> io::Result<(bool, Self)> {
        self.read_bits(1).map(|(v, r)| (v != 0, r))
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let byte_index = (self.bit_offset + 7) / 8;
        self.buffer[byte_index..].to_vec()
    }
}