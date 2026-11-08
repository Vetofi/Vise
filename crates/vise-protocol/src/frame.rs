use crate::{
    Error, Result,
    buf::{Reader, write_varint},
};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};

pub const MAX_FRAME: usize = (1 << 21) - 1;
pub const MAX_UNCOMPRESSED: usize = 8_388_608;

pub fn encode(body: &[u8], threshold: Option<usize>) -> Result<Vec<u8>> {
    Ok(prefixed(&contents(body, threshold)?))
}

pub fn contents(body: &[u8], threshold: Option<usize>) -> Result<Vec<u8>> {
    let inner = match threshold {
        None => body.to_vec(),
        Some(t) if body.len() < t => {
            let mut v = Vec::with_capacity(body.len() + 1);
            v.push(0);
            v.extend_from_slice(body);
            v
        }
        Some(_) => {
            let mut v = Vec::with_capacity(body.len() / 2 + 8);
            write_varint(&mut v, body.len() as i32);
            let mut z = ZlibEncoder::new(v, Compression::default());
            z.write_all(body)?;
            z.finish()?
        }
    };
    if inner.len() > MAX_FRAME {
        return Err(Error::new(format!(
            "packet too large: {} bytes",
            inner.len()
        )));
    }
    Ok(inner)
}

pub fn prefixed(contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(contents.len() + 3);
    write_varint(&mut out, contents.len() as i32);
    out.extend_from_slice(contents);
    out
}

pub fn split(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>> {
    let mut len = 0usize;
    for i in 0..3 {
        let Some(&b) = buffer.get(i) else {
            return Ok(None);
        };
        len |= usize::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            if len == 0 {
                return Err(Error::new("Frame length cannot be zero"));
            }
            let start = i + 1;
            if buffer.len() < start + len {
                return Ok(None);
            }
            let frame = buffer[start..start + len].to_vec();
            buffer.drain(..start + len);
            return Ok(Some(frame));
        }
    }
    Err(Error::new("length wider than 21 bits"))
}

pub fn decompress(frame: Vec<u8>, threshold: Option<usize>) -> Result<Vec<u8>> {
    let Some(threshold) = threshold else {
        return Ok(frame);
    };
    let mut r = Reader::new(&frame);
    let size = r.varint()?;
    if size == 0 {
        return Ok(frame[r.position()..].to_vec());
    }
    let size = usize::try_from(size).map_err(|_| Error::new("negative uncompressed length"))?;
    if size < threshold {
        return Err(Error::new(format!(
            "Badly compressed packet - size of {size} is below server threshold of {threshold}"
        )));
    }
    if size > MAX_UNCOMPRESSED {
        return Err(Error::new(format!(
            "Badly compressed packet - size of {size} is larger than protocol maximum"
        )));
    }
    let start = r.position();
    let mut out = Vec::with_capacity(size);
    ZlibDecoder::new(&frame[start..])
        .take(size as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() != size {
        return Err(Error::new(format!(
            "Badly compressed packet - actual length {} does not match declared size {size}",
            out.len()
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncompressed_frames_round_trip() {
        let body = vec![0x00, 1, 2, 3];
        let framed = encode(&body, None).unwrap();
        assert_eq!(framed, [4, 0, 1, 2, 3]);
        let mut buffer = framed.clone();
        buffer.extend_from_slice(&framed[..2]);
        assert_eq!(split(&mut buffer).unwrap(), Some(body));
        assert_eq!(split(&mut buffer).unwrap(), None);
        assert_eq!(buffer, [4, 0]);
    }

    #[test]
    fn compression_threshold_like_vanilla() {
        let small = vec![7u8; 10];
        let framed = encode(&small, Some(256)).unwrap();
        assert_eq!(&framed[..2], &[11, 0]);
        let mut buffer = framed;
        let frame = split(&mut buffer).unwrap().unwrap();
        assert_eq!(decompress(frame, Some(256)).unwrap(), small);
        let big: Vec<u8> = (0..5000u32).map(|i| (i % 7) as u8).collect();
        let framed = encode(&big, Some(256)).unwrap();
        assert!(framed.len() < big.len());
        let mut buffer = framed;
        let frame = split(&mut buffer).unwrap().unwrap();
        assert_eq!(Reader::new(&frame).varint().unwrap(), 5000);
        assert_eq!(decompress(frame, Some(256)).unwrap(), big);
    }

    #[test]
    fn rejects_bad_compressed_lengths() {
        let mut frame = vec![10];
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&[1; 10]).unwrap();
        frame.extend(z.finish().unwrap());
        assert!(decompress(frame, Some(256)).is_err());
        let mut frame = Vec::new();
        write_varint(&mut frame, 300);
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&[1; 299]).unwrap();
        frame.extend(z.finish().unwrap());
        assert!(decompress(frame, Some(256)).is_err());
    }

    #[test]
    fn a_noncanonical_length_prefix_still_reads() {
        assert_eq!(
            decompress(vec![0x80, 0x00, 7, 1, 2], Some(256)).unwrap(),
            [7, 1, 2]
        );
        let body = vec![9; 300];
        let mut frame = vec![0xAC, 0x82, 0x00];
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&body).unwrap();
        frame.extend(z.finish().unwrap());
        assert_eq!(decompress(frame, Some(256)).unwrap(), body);
    }
}
