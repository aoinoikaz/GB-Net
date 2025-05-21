use std::io::{self, Read, Write};

pub trait BitWrite: Write {
    fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()>;
    fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        self.write_bits(if bit { 1 } else { 0 }, 1)
    }
}

pub trait BitRead: Read {
    fn read_bits(&mut self, bits: usize) -> io::Result<u64>;
    fn read_bit(&mut self) -> io::Result<bool> {
        self.read_bits(1).map(|v| v != 0)
    }
}

pub struct BitWriter {
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

    pub fn with_capacity(capacity: usize) -> Self {
        BitWriter {
            buffer: Vec::with_capacity(capacity),
            bit_offset: 0,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

impl Write for BitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl BitWrite for BitWriter {
    fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot write more than 64 bits"));
        }

        let mut remaining_bits = bits;
        let mut current_value = value;

        while remaining_bits > 0 {
            let bits_to_write = std::cmp::min(8 - (self.bit_offset % 8), remaining_bits);
            let shift = remaining_bits - bits_to_write;
            let mask = (1u64 << bits_to_write) - 1;
            let byte_value = ((current_value >> shift) & mask) as u8;

            let byte_index = self.bit_offset / 8;
            let bit_index = self.bit_offset % 8;

            if byte_index >= self.buffer.len() {
                self.buffer.push(0);
            }

            self.buffer[byte_index] |= byte_value << (8 - bit_index - bits_to_write);
            self.bit_offset += bits_to_write;
            remaining_bits -= bits_to_write;
            current_value &= (1u64 << shift) - 1;
        }

        Ok(())
    }
}

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
}

impl Read for BitReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_left = self.buffer.len() - (self.bit_offset / 8);
        let bytes_to_read = std::cmp::min(buf.len(), bytes_left);
        let start = self.bit_offset / 8;
        buf[..bytes_to_read].copy_from_slice(&self.buffer[start..start + bytes_to_read]);
        self.bit_offset += bytes_to_read * 8;
        Ok(bytes_to_read)
    }
}

impl BitRead for BitReader {
    fn read_bits(&mut self, bits: usize) -> io::Result<u64> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot read more than 64 bits"));
        }

        let mut value = 0u64;
        let mut remaining_bits = bits;

        while remaining_bits > 0 {
            let byte_index = self.bit_offset / 8;
            let bit_index = self.bit_offset % 8;
            let bits_to_read = std::cmp::min(8 - bit_index, remaining_bits);

            if byte_index >= self.buffer.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
            }

            let mask = (1u64 << bits_to_read) - 1;
            let byte_value = (self.buffer[byte_index] >> (8 - bit_index - bits_to_read)) & mask as u8;
            value = (value << bits_to_read) | byte_value as u64;

            self.bit_offset += bits_to_read;
            remaining_bits -= bits_to_read;
        }

        Ok(value)
    }
}