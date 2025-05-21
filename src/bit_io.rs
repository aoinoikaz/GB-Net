use std::io;

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

    pub fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        let byte_index = self.bit_offset / 8;
        let bit_index = self.bit_offset % 8;

        if byte_index >= self.buffer.len() {
            self.buffer.push(0);
        }

        if bit {
            self.buffer[byte_index] |= 1 << (7 - bit_index);
        }

        self.bit_offset += 1;
        Ok(())
    }

    pub fn write_bits(&mut self, value: u64, bits: usize) -> io::Result<()> {
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

    pub fn write_f32(&mut self, value: f32) -> io::Result<()> {
        self.write_bits(value.to_bits() as u64, 32)
    }

    pub fn write_f64(&mut self, value: f64) -> io::Result<()> {
        self.write_bits(value.to_bits(), 64)
    }

    pub fn write_u8(&mut self, value: u8) -> io::Result<()> {
        self.write_bits(value as u64, 8)
    }

    pub fn write_u16(&mut self, value: u16) -> io::Result<()> {
        self.write_bits(value as u64, 16)
    }

    pub fn write_u32(&mut self, value: u32) -> io::Result<()> {
        self.write_bits(value as u64, 32)
    }

    pub fn write_i32(&mut self, value: i32) -> io::Result<()> {
        self.write_bits(value as u64, 32)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
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

    pub fn read_bit(&mut self) -> io::Result<bool> {
        let byte_index = self.bit_offset / 8;
        let bit_index = self.bit_offset % 8;

        if byte_index >= self.buffer.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Buffer too short"));
        }

        let bit = (self.buffer[byte_index] & (1 << (7 - bit_index))) != 0;
        self.bit_offset += 1;
        Ok(bit)
    }

    pub fn read_bits(&mut self, bits: usize) -> io::Result<u64> {
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

    pub fn read_f32(&mut self) -> io::Result<f32> {
        let bits = self.read_bits(32)?;
        Ok(f32::from_bits(bits as u32))
    }

    pub fn read_f64(&mut self) -> io::Result<f64> {
        let bits = self.read_bits(64)?;
        Ok(f64::from_bits(bits))
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let bits = self.read_bits(8)?;
        Ok(bits as u8)
    }

    pub fn read_u16(&mut self) -> io::Result<u16> {
        let bits = self.read_bits(16)?;
        Ok(bits as u16)
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let bits = self.read_bits(32)?;
        Ok(bits as u32)
    }

    pub fn read_i32(&mut self) -> io::Result<i32> {
        let bits = self.read_bits(32)?;
        Ok(bits as i32)
    }
}