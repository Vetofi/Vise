use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};

pub const VERSION: u32 = 1;
pub const MAX_LINE: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ToRelay {
    Host {
        v: u32,
        protocol: i32,
        game: String,
    },
    Join {
        v: u32,
        nameplate: String,
        protocol: i32,
        game: String,
    },
    Attach {
        v: u32,
        session: u64,
        token: String,
    },
    Pong,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FromRelay {
    Registered {
        nameplate: String,
        token: String,
        observed: Option<String>,
    },
    Join {
        session: u64,
    },
    Bridged {
        session: u64,
        observed: Option<String>,
    },
    Ping,
    Error {
        code: ErrorCode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        game: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownCode,
    Ended,
    VersionMismatch,
    HostUnreachable,
    Full,
    RateLimited,
    BadRequest,
}
impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCode => "unknown_code",
            Self::Ended => "ended",
            Self::VersionMismatch => "version_mismatch",
            Self::HostUnreachable => "host_unreachable",
            Self::Full => "full",
            Self::RateLimited => "rate_limited",
            Self::BadRequest => "bad_request",
        }
    }
}

pub fn send<W: Write, T: Serialize>(w: &mut W, message: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    w.write_all(&line)?;
    w.flush()
}

pub fn read_line<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match r.read(&mut byte) {
            Ok(0) if line.is_empty() => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(_) if byte[0] == b'\n' => return Ok(Some(line)),
            Ok(_) => {
                if line.len() == MAX_LINE {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
                }
                line.push(byte[0]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

pub fn recv<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R) -> io::Result<T> {
    let line = read_line(r)?.ok_or(io::ErrorKind::UnexpectedEof)?;
    serde_json::from_slice(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn recv_buffered<R: BufRead, T: for<'de> Deserialize<'de>>(r: &mut R) -> io::Result<T> {
    let mut line = Vec::new();
    let n = r.take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line)?;
    if n == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    if line.last() != Some(&b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    serde_json::from_slice(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        let mut buf = Vec::new();
        send(
            &mut buf,
            &ToRelay::Join {
                v: VERSION,
                nameplate: "K7QM".into(),
                protocol: 777,
                game: "26.3".into(),
            },
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(buf.clone()).unwrap(),
            "{\"op\":\"join\",\"v\":1,\"nameplate\":\"K7QM\",\"protocol\":777,\"game\":\"26.3\"}\n"
        );
        send(
            &mut buf,
            &FromRelay::Error {
                code: ErrorCode::VersionMismatch,
                game: Some("26.2".into()),
            },
        )
        .unwrap();
        buf.extend_from_slice(b"raw");
        let mut r = &buf[..];
        let first: ToRelay = recv(&mut r).unwrap();
        assert!(matches!(first, ToRelay::Join { protocol: 777, .. }));
        let second: FromRelay = recv(&mut r).unwrap();
        assert_eq!(
            second,
            FromRelay::Error {
                code: ErrorCode::VersionMismatch,
                game: Some("26.2".into())
            }
        );
        assert_eq!(r, b"raw");
    }

    #[test]
    fn long_lines_and_garbage_are_refused() {
        let long = vec![b'a'; MAX_LINE + 1];
        assert!(read_line(&mut &long[..]).is_err());
        let mut exact = vec![b'a'; MAX_LINE];
        exact.push(b'\n');
        assert_eq!(read_line(&mut &exact[..]).unwrap().unwrap().len(), MAX_LINE);
        assert!(recv::<_, ToRelay>(&mut &b"{\"op\":\"nope\"}\n"[..]).is_err());
        assert!(recv::<_, ToRelay>(&mut &b""[..]).is_err());
        assert!(read_line(&mut &b""[..]).unwrap().is_none());
        assert!(read_line(&mut &b"partial"[..]).is_err());
        let mut buffered = io::BufReader::new(&long[..]);
        assert!(recv_buffered::<_, ToRelay>(&mut buffered).is_err());
        let mut buffered = io::BufReader::new(&b"{\"op\":\"pong\"}\n"[..]);
        assert_eq!(
            recv_buffered::<_, ToRelay>(&mut buffered).unwrap(),
            ToRelay::Pong
        );
    }
}
