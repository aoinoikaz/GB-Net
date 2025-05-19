use log::{debug, trace};
use std::io;

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

    pub fn flush(&mut self) -> io::Result<()> {
        if self.bit_position > 0 {
            self.buffer.push(self.current_byte);
            trace!("Flushed final byte: {:02x}", self.current_byte);
            self.current_byte = 0;
            self.bit_position = 0;
        }
        Ok(())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        trace!("Returning buffer: {:02x?}", self.buffer);
        self.buffer
    }
}

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

    pub fn read_bits(&mut self, bits: u8) -> io::Result<u64> {
        if bits > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Cannot read more than 64 bits"));
        }
        if self.position >= self.buffer.len() && bits > 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer underflow"));
        }

        let mut result: u64 = 0;
        let mut remaining_bits = bits;

        while remaining_bits > 0 {
            let bits_to_read = (8 - self.bit_position).min(remaining_bits);
            let bits_available = (self.buffer.len() - self.position) * 8 - self.bit_position as usize;
            if bits_to_read as usize > bits_available {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Not enough bits available"));
            }

            let byte = self.buffer[self.position];
            let shift = 8 - self.bit_position - bits_to_read;
            let mask = ((1 << bits_to_read) - 1) << shift;
            let value = ((byte & mask) >> shift) as u64;

            result = result.checked_shl(bits_to_read as u32).unwrap_or(u64::MAX) | value;
            self.bit_position += bits_to_read;
            remaining_bits -= bits_to_read;

            if self.bit_position >= 8 {
                self.position += 1;
                self.bit_position = 0;
            }
            debug!("Processed {} bits: value {}, position {}, bit_position {}", 
                   bits_to_read, value, self.position, self.bit_position);
        }

        Ok(result)
    }
}