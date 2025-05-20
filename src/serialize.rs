use std::io;

pub trait Serialize {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter>;
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> where Self: Sized;
}

#[derive(Debug, Clone)]
pub struct BitWriter {
    buffer: Vec<u8>,
    bit_offset: usize,
}

#[derive(Debug, Clone)]
pub struct BitReader {
    buffer: Vec<u8>,
    bit_offset: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        BitWriter {
            buffer: Vec::new(),
            bit_offset: 0,
        }
    }

    pub fn write_bit(self, bit: bool) -> io::Result<Self> {
        let mut new_writer = self;
        let byte_index = new_writer.bit_offset / 8;
        let bit_index = new_writer.bit_offset % 8;

        if byte_index >= new_writer.buffer.len() {
            new_writer.buffer.push(0);
        }

        if bit {
            new_writer.buffer[byte_index] |= 1 << (7 - bit_index);
        }

        new_writer.bit_offset += 1;
        Ok(new_writer)
    }

    pub fn write_bits(self, value: u64, bits: usize) -> io::Result<Self> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot write more than 64 bits"));
        }

        let mut new_writer = self;
        let mut remaining_bits = bits;
        let mut current_value = value;

        while remaining_bits > 0 {
            let bits_to_write = std::cmp::min(8 - (new_writer.bit_offset % 8), remaining_bits);
            let shift = remaining_bits - bits_to_write;
            let mask = (1u64 << bits_to_write) - 1;
            let byte_value = ((current_value >> shift) & mask) as u8;

            let byte_index = new_writer.bit_offset / 8;
            let bit_index = new_writer.bit_offset % 8;

            if byte_index >= new_writer.buffer.len() {
                new_writer.buffer.push(0);
            }

            new_writer.buffer[byte_index] |= byte_value << (8 - bit_index - bits_to_write);
            new_writer.bit_offset += bits_to_write;
            remaining_bits -= bits_to_write;
        }

        Ok(new_writer)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

impl BitReader {
    pub fn new(buffer: Vec<u8>) -> Self {
        BitReader {
            buffer,
            bit_offset: 0,
        }
    }

    pub fn read_bit(self) -> io::Result<(bool, Self)> {
        let mut new_reader = self;
        let byte_index = new_reader.bit_offset / 8;
        let bit_index = new_reader.bit_offset % 8;

        if byte_index >= new_reader.buffer.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
        }

        let bit = (new_reader.buffer[byte_index] & (1 << (7 - bit_index))) != 0;
        new_reader.bit_offset += 1;
        Ok((bit, new_reader))
    }

    pub fn read_bits(self, bits: usize) -> io::Result<(u64, Self)> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot read more than 64 bits"));
        }

        let mut new_reader = self;
        let mut value = 0u64;
        let mut remaining_bits = bits;

        while remaining_bits > 0 {
            let byte_index = new_reader.bit_offset / 8;
            let bit_index = new_reader.bit_offset % 8;
            let bits_to_read = std::cmp::min(8 - bit_index, remaining_bits);

            if byte_index >= new_reader.buffer.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
            }

            let mask = (1u64 << bits_to_read) - 1;
            let byte_value = (new_reader.buffer[byte_index] >> (8 - bit_index - bits_to_read)) & mask as u8;
            value = (value << bits_to_read) | byte_value as u64;

            new_reader.bit_offset += bits_to_read;
            remaining_bits -= bits_to_read;
        }

        Ok((value, new_reader))
    }
}