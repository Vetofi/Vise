pub mod configuration;
pub mod handshake;
pub mod login;
pub mod play;
pub mod recipes;
pub mod status;

use crate::buf::{Codec, Reader, Writer};

#[macro_export]
macro_rules! packet_enum {
    ($(#[$meta:meta])* $name:ident, $ids:path { $($variant:ident($ty:ty) = $id:ident),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub enum $name {
            $($variant($ty),)*
            Unknown(i32, Vec<u8>),
        }
        impl $name {
            pub fn id(&self) -> i32 {
                use $ids as ids;
                match self {
                    $(Self::$variant(_) => ids::$id,)*
                    Self::Unknown(id, _) => *id,
                }
            }
            pub fn decode(id: i32, payload: &[u8]) -> $crate::Result<Self> {
                use $ids as ids;
                let mut r = $crate::buf::Reader::new(payload);
                let packet = match id {
                    $(ids::$id => Self::$variant(<$ty as $crate::buf::Codec<$ty>>::read(&mut r)?),)*
                    _ => return Ok(Self::Unknown(id, payload.to_vec())),
                };
                if r.remaining() != 0 {
                    return Err($crate::Error::new(format!(
                        "packet {} ({}) was larger than expected: {} extra bytes",
                        id,
                        stringify!($name),
                        r.remaining()
                    )));
                }
                Ok(packet)
            }
            pub fn encode(&self) -> Vec<u8> {
                let mut w = $crate::buf::Writer::new();
                w.varint(self.id());
                match self {
                    $(Self::$variant(p) => <$ty as $crate::buf::Codec<$ty>>::write(p, &mut w),)*
                    Self::Unknown(_, payload) => w.bytes(payload),
                }
                w.data
            }
        }
    };
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Empty;
impl Codec<Empty> for Empty {
    fn read(_: &mut Reader) -> crate::Result<Empty> {
        Ok(Empty)
    }
    fn write(_: &Empty, _: &mut Writer) {}
}

#[cfg(test)]
pub(crate) fn round_trip<T: Clone + PartialEq + std::fmt::Debug>(
    packet: T,
    encode: impl Fn(&T) -> Vec<u8>,
    decode: impl Fn(i32, &[u8]) -> crate::Result<T>,
) {
    let body = encode(&packet);
    let mut r = Reader::new(&body);
    let id = r.varint().unwrap();
    let back = decode(id, r.rest()).unwrap();
    assert_eq!(back, packet);
}
