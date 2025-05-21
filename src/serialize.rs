use std::io::{self, Read, Write};
use std::time::Instant;
use std::collections::HashMap;
use crate::bit_io::{BitWrite, BitRead, BitWriter, BitReader};

pub trait Serialize {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()>;
}

pub trait Deserialize: Sized {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self>;
}

pub trait BitSerialize {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()>;
}

pub trait BitDeserialize: Sized {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self>;
}

macro_rules! impl_primitive {
    ($($t:ty, $write:ident, $read:ident, $bits:expr),*) => {
        $(
            impl Serialize for $t {
                fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
                    writer.$write::<std::io::LittleEndian>(*self)?;
                    Ok(())
                }
            }
            impl Deserialize for $t {
                fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
                    reader.$read::<std::io::LittleEndian>()
                }
            }
            impl BitSerialize for $t {
                fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
                    writer.write_bits(*self as u64, $bits)?;
                    Ok(())
                }
            }
            impl BitDeserialize for $t {
                fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
                    let value = reader.read_bits($bits)?;
                    Ok(value as $t)
                }
            }
        )*
    };
}

impl_primitive!(
    u8, write_u8, read_u8, 8,
    u16, write_u16, read_u16, 16,
    u32, write_u32, read_u32, 32,
    u64, write_u64, read_u64, 64,
    i8, write_i8, read_i8, 8,
    i16, write_i16, read_i16, 16,
    i32, write_i32, read_i32, 32,
    i64, write_i64, read_i64, 64,
    f32, write_f32, read_f32, 32,
    f64, write_f64, read_f64, 64
);

impl Serialize for bool {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(if *self { 1 } else { 0 })?;
        Ok(())
    }
}

impl Deserialize for bool {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let value = reader.read_u8()?;
        Ok(value != 0)
    }
}

impl BitSerialize for bool {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_bit(*self)?;
        Ok(())
    }
}

impl BitDeserialize for bool {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        reader.read_bit()
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<std::io::LittleEndian>(self.len() as u32)?;
        for item in self {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<std::io::LittleEndian>()?;
        let mut vec = Vec::with_capacity(len as usize);
        for _ in 0..len {
            vec.push(T::deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl<T: BitSerialize> BitSerialize for Vec<T> {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_bits(self.len() as u64, 16)?;
        for item in self {
            item.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: BitDeserialize> BitDeserialize for Vec<T> {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_bits(16)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::bit_deserialize(reader)?);
        }
        Ok(vec)
    }
}

impl Serialize for String {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.as_bytes();
        writer.write_u32::<std::io::LittleEndian>(bytes.len() as u32)?;
        writer.write_all(bytes)?;
        Ok(())
    }
}

impl Deserialize for String {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<std::io::LittleEndian>()?;
        let mut bytes = vec![0u8; len as usize];
        reader.read_exact(&mut bytes)?;
        String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))
    }
}

impl BitSerialize for String {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.as_bytes();
        writer.write_bits(bytes.len() as u64, 16)?;
        for &b in bytes {
            writer.write_bits(b as u64, 8)?;
        }
        Ok(())
    }
}

impl BitDeserialize for String {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_bits(16)? as usize;
        let mut bytes = vec![0u8; len];
        for b in bytes.iter_mut() {
            *b = reader.read_bits(8)? as u8;
        }
        String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))
    }
}

impl<K: Serialize, V: Serialize> Serialize for HashMap<K, V> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<std::io::LittleEndian>(self.len() as u32)?;
        for (key, value) in self {
            key.serialize(writer)?;
            value.serialize(writer)?;
        }
        Ok(())
    }
}

impl<K: Deserialize + std::cmp::Eq + std::hash::Hash, V: Deserialize> Deserialize for HashMap<K, V> {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_u32::<std::io::LittleEndian>()?;
        let mut map = HashMap::with_capacity(len as usize);
        for _ in 0..len {
            let key = K::deserialize(reader)?;
            let value = V::deserialize(reader)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<K: BitSerialize, V: BitSerialize> BitSerialize for HashMap<K, V> {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_bits(self.len() as u64, 16)?;
        for (key, value) in self {
            key.bit_serialize(writer)?;
            value.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<K: BitDeserialize + std::cmp::Eq + std::hash::Hash, V: BitDeserialize> BitDeserialize for HashMap<K, V> {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let len = reader.read_bits(16)? as usize;
        let mut map = HashMap::with_capacity(len);
        for _ in 0..len {
            let key = K::bit_deserialize(reader)?;
            let value = V::bit_deserialize(reader)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        for item in self {
            item.serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut arr = [const { std::mem::MaybeUninit::<T>::uninit() }; N];
        for i in 0..N {
            arr[i] = std::mem::MaybeUninit::new(T::deserialize(reader)?);
        }
        let arr = unsafe { std::mem::transmute_copy(&arr) };
        Ok(arr)
    }
}

impl<T: BitSerialize, const N: usize> BitSerialize for [T; N] {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        for item in self {
            item.bit_serialize(writer)?;
        }
        Ok(())
    }
}

impl<T: BitDeserialize, const N: usize> BitDeserialize for [T; N] {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let mut arr = [const { std::mem::MaybeUninit::<T>::uninit() }; N];
        for i in 0..N {
            arr[i] = std::mem::MaybeUninit::new(T::bit_deserialize(reader)?);
        }
        let arr = unsafe { std::mem::transmute_copy(&arr) };
        Ok(arr)
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            None => writer.write_u8(0),
            Some(value) => {
                writer.write_u8(1)?;
                value.serialize(writer)
            }
        }
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let tag = reader.read_u8()?;
        if tag == 0 {
            Ok(None)
        } else {
            Ok(Some(T::deserialize(reader)?))
        }
    }
}

impl<T: BitSerialize> BitSerialize for Option<T> {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            None => writer.write_bit(false),
            Some(value) => {
                writer.write_bit(true)?;
                value.bit_serialize(writer)
            }
        }
    }
}

impl<T: BitDeserialize> BitDeserialize for Option<T> {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let tag = reader.read_bit()?;
        if !tag {
            Ok(None)
        } else {
            Ok(Some(T::bit_deserialize(reader)?))
        }
    }
}

macro_rules! impl_tuple {
    ($($n:tt $T:ident),*) => {
        impl<$($T: Serialize),*> Serialize for ($($T,)*) {
            fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
                $(
                    self.$n.serialize(writer)?;
                )*
                Ok(())
            }
        }
        impl<$($T: Deserialize),*> Deserialize for ($($T,)*) {
            fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
                $(
                    let val_$n = $T::deserialize(reader)?;
                )*
                Ok(($(val_$n,)*))
            }
        }
        impl<$($T: BitSerialize),*> BitSerialize for ($($T,)*) {
            fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
                $(
                    self.$n.bit_serialize(writer)?;
                )*
                Ok(())
            }
        }
        impl<$($T: BitDeserialize),*> BitDeserialize for ($($T,)*) {
            fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
                $(
                    let val_$n = $T::bit_deserialize(reader)?;
                )*
                Ok(($(val_$n,)*))
            }
        }
    };
}

impl_tuple!();
impl_tuple!(0 T0);
impl_tuple!(0 T0, 1 T1);
impl_tuple!(0 T0, 1 T1, 2 T2);
impl_tuple!(0 T0, 1 T1, 2 T2, 3 T3);

impl Serialize for Instant {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let duration = self.duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"))?;
        writer.write_u64::<std::io::LittleEndian>(duration.as_nanos() as u64)?;
        Ok(())
    }
}

impl Deserialize for Instant {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let nanos = reader.read_u64::<std::io::LittleEndian>()?;
        let duration = std::time::Duration::from_nanos(nanos);
        let instant = std::time::SystemTime::UNIX_EPOCH.checked_add(duration)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"))?;
        Ok(instant)
    }
}

impl BitSerialize for Instant {
    fn bit_serialize<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        let duration = self.duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"))?;
        writer.write_bits(duration.as_nanos() as u64, 64)?;
        Ok(())
    }
}

impl BitDeserialize for Instant {
    fn bit_deserialize<R: BitRead>(reader: &mut R) -> io::Result<Self> {
        let nanos = reader.read_bits(64)?;
        let duration = std::time::Duration::from_nanos(nanos);
        let instant = std::time::SystemTime::UNIX_EPOCH.checked_add(duration)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp"))?;
        Ok(instant)
    }
}