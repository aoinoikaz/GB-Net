use super::{bit_io::{BitWriter, BitReader}, Serialize, Deserialize};
use std::io;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

macro_rules! impl_primitive {
    ($($t:ty, $write:ident, $read:ident, $bits:expr),*) => {
        $(
            impl Serialize for $t {
                fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
                    writer.$write(*self)
                }
            }

            impl Deserialize for $t {
                fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
                    reader.$read()
                }
            }
        )*
    };
}

impl_primitive!(
    u8, write_u8, read_u8, 8,
    u16, write_u16, read_u16, 16,
    u32, write_u32, read_u32, 32,
    i32, write_i32, read_i32, 32,
    f32, write_f32, read_f32, 32,
    f64, write_f64, read_f64, 64
);

impl Serialize for bool {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        writer.write_bit(*self)
    }
}

impl Deserialize for bool {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        reader.read_bit()
    }
}

macro_rules! impl_tuple {
    ($($n:tt $T:ident),*) => {
        impl<$($T: Serialize),*> Serialize for ($($T),*) {
            fn serialize(&self, mut writer: BitWriter) -> io::Result<BitWriter> {
                $(
                    writer = self.$n.serialize(writer)?;
                )*
                Ok(writer)
            }
        }
        impl<$($T: Deserialize),*> Deserialize for ($($T),*) {
            fn deserialize(mut reader: BitReader) -> io::Result<(Self, BitReader)> {
                $(
                    let (val_$n, r) = $T::deserialize(reader)?;
                    reader = r;
                )*
                Ok((($(val_$n),*), reader))
            }
        }
    };
}

impl_tuple!(0 T0);
impl_tuple!(0 T0, 1 T1);
impl_tuple!(0 T0, 1 T1, 2 T2);
impl_tuple!(0 T0, 1 T1, 2 T2, 3 T3);

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        self.iter().fold(Ok(writer), |w, item| item.serialize(w?))
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let mut reader = reader;
        let mut arr = [const { std::mem::MaybeUninit::<T>::uninit() }; N];
        for i in 0..N {
            let (item, r) = T::deserialize(reader)?;
            arr[i] = std::mem::MaybeUninit::new(item);
            reader = r;
        }
        let arr = unsafe { std::mem::transmute_copy(&arr) };
        Ok((arr, reader))
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        let writer = writer.write_bits(self.len() as u64, 16)?;
        self.iter().fold(Ok(writer), |w, item| item.serialize(w?))
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (len, mut reader) = reader.read_bits(16)?;
        let mut vec = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let (item, r) = T::deserialize(reader)?;
            vec.push(item);
            reader = r;
        }
        Ok((vec, reader))
    }
}

impl<K: Serialize, V: Serialize> Serialize for HashMap<K, V> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        let writer = writer.write_bits(self.len() as u64, 16)?;
        let mut writer = writer;
        for (key, value) in self {
            writer = key.serialize(writer)?;
            writer = value.serialize(writer)?;
        }
        Ok(writer)
    }
}

impl<K: Deserialize + std::cmp::Eq + std::hash::Hash, V: Deserialize> Deserialize for HashMap<K, V> {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (len, mut reader) = reader.read_bits(16)?;
        let mut map = HashMap::with_capacity(len as usize);
        for _ in 0..len {
            let (key, r) = K::deserialize(reader)?;
            let (value, r) = V::deserialize(r)?;
            map.insert(key, value);
            reader = r;
        }
        Ok((map, reader))
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        match self {
            None => writer.write_bits(0, 1),
            Some(value) => value.serialize(writer.write_bits(1, 1)?),
        }
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (tag, reader) = reader.read_bit()?;
        if tag {
            let (value, reader) = T::deserialize(reader)?;
            Ok((Some(value), reader))
        } else {
            Ok((None, reader))
        }
    }
}

impl Serialize for String {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        let bytes = self.as_bytes();
        let writer = writer.write_bits(bytes.len() as u64, 16)?;
        bytes.iter().fold(Ok(writer), |w, &b| w?.write_bits(b as u64, 8))
    }
}

impl Deserialize for String {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (len, mut reader) = reader.read_bits(16)?;
        let mut bytes = vec![0u8; len as usize];
        for b in bytes.iter_mut() {
            let (byte, r) = reader.read_bits(8)?;
            *b = byte as u8;
            reader = r;
        }
        let s = String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))?;
        Ok((s, reader))
    }
}

impl<T: Serialize> Serialize for Box<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        self.as_ref().serialize(writer)
    }
}

impl<T: Deserialize> Deserialize for Box<T> {
    fn deserialize(reader: BitReader) -> io::Result<(Self, BitReader)> {
        let (value, reader) = T::deserialize(reader)?;
        Ok((Box::new(value), reader))
    }
}

impl<T: Serialize> Serialize for Rc<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        self.as_ref().serialize(writer)
    }
}

impl<T: Serialize> Serialize for Arc<T> {
    fn serialize(&self, writer: BitWriter) -> io::Result<BitWriter> {
        self.as_ref().serialize(writer)
    }
}