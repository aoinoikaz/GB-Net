use super::{bit_io::{BitWriter, BitReader}, Serialize, Deserialize};
use std::io;
use std::time::{Instant, Duration};

impl Serialize for Instant {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        let since_epoch = self.duration_since(std::time::UNIX_EPOCH).unwrap_or(Duration::ZERO);
        writer.write_bits(since_epoch.as_millis() as u64, 32)
    }
}

impl Deserialize for Instant {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (millis, reader) = reader.read_bits(32)?;
        let instant = std::time::UNIX_EPOCH + Duration::from_millis(millis);
        Ok((instant, reader))
    }
}