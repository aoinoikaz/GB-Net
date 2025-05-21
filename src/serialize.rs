use std::io::{self, Read, Write};
use std::time::Instant;
use std::collections::HashMap;

pub trait Serialize {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()>;
}

pub trait Deserialize: Sized {
    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self>;
}

macro_rules! impl_primitive {
    ($($t:ty, $write:ident, $read:ident),*) => {
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
        )*
    };
}

impl_primitive!(
    u8, write_u8, read_u8,
    u16, write_u16, read_u16,
    u32, write_u32, read_u32,
    u64, write_u64, read_u64,
    i8, write_i8, read_i8,
    i16, write_i16, read_i16,
    i32, write_i32, read_i32,
    i64, write_i64, read_i64,
    f32, write_f32, read_f32,
    f64, write_f64, read_f64
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