pub mod code;
pub mod crypto;
pub mod direct;
pub mod guest;
pub mod host;
pub mod invite;
pub mod portmap;
pub mod reach;
mod session;
pub mod stun;
pub mod wire;

pub use code::Code;
pub use guest::{JoinError, Tunnel};
pub use host::{Host, HostConfig, HostEvent, HostState};

use std::{
    io,
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    sync::mpsc::{RecvTimeoutError, Sender, channel},
    thread::JoinHandle,
    time::Duration,
};

pub const ADDRESS_PREFIX: &str = "friend:";

pub(crate) fn fill_random(buf: &mut [u8]) -> io::Result<()> {
    getrandom::fill(buf).map_err(|e| io::Error::other(e.to_string()))
}

pub struct Deadline {
    _cancel: Sender<()>,
}

pub fn deadline(stream: &TcpStream, limit: Duration) -> io::Result<Deadline> {
    let stream = stream.try_clone()?;
    let (tx, rx) = channel::<()>();
    std::thread::Builder::new()
        .name("vise-friends-deadline".into())
        .spawn(move || {
            if rx.recv_timeout(limit) == Err(RecvTimeoutError::Timeout) {
                let _ = stream.shutdown(Shutdown::Both);
            }
        })?;
    Ok(Deadline { _cancel: tx })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathMode {
    Auto,
    RelayOnly,
    DirectOnly,
}
impl PathMode {
    pub fn from_env() -> Self {
        match std::env::var("VISE_FRIENDS_PATH").as_deref() {
            Ok("relay") => Self::RelayOnly,
            Ok("direct") => Self::DirectOnly,
            _ => Self::Auto,
        }
    }
}

pub fn relay_url() -> Option<String> {
    std::env::var("VISE_RELAY_URL")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn resolve_relay(url: &str) -> io::Result<Vec<SocketAddr>> {
    let address = url
        .trim()
        .trim_start_matches("tcp://")
        .trim_end_matches('/');
    let found: Vec<SocketAddr> = address.to_socket_addrs()?.collect();
    if found.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the relay's name didn't resolve",
        ));
    }
    Ok(found)
}

pub fn pump(
    plain: TcpStream,
    mut reader: crypto::SecureReader<TcpStream>,
    mut writer: crypto::SecureWriter<TcpStream>,
) -> io::Result<JoinHandle<()>> {
    use io::{Read, Write};
    for s in [&plain, reader.get_ref(), writer.get_ref()] {
        s.set_read_timeout(None)?;
        let _ = s.set_nodelay(true);
    }
    let mut plain_in = plain.try_clone()?;
    let mut plain_out = plain;
    let outbound = std::thread::Builder::new()
        .name("vise-friends-out".into())
        .spawn(move || {
            let mut buf = vec![0u8; crypto::MAX_PAYLOAD];
            loop {
                match plain_in.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if writer.write_data(&buf[..n]).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = writer.get_ref().shutdown(std::net::Shutdown::Both);
            let _ = plain_in.shutdown(std::net::Shutdown::Both);
        })?;
    std::thread::Builder::new()
        .name("vise-friends-in".into())
        .spawn(move || {
            while let Ok(Some(frame)) = reader.read_frame() {
                if let crypto::Frame::Data(d) = frame
                    && plain_out.write_all(&d).is_err()
                {
                    break;
                }
            }
            let _ = plain_out.shutdown(std::net::Shutdown::Both);
            let _ = reader.get_ref().shutdown(std::net::Shutdown::Both);
            let _ = outbound.join();
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Read,
        net::TcpListener,
        time::{Duration, Instant},
    };

    #[test]
    fn a_deadline_ends_the_stream_unless_dropped() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let _kept = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (mut kept, _) = l.accept().unwrap();
        let _ended = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (mut ended, _) = l.accept().unwrap();
        drop(deadline(&kept, Duration::from_millis(100)).unwrap());
        let _running = deadline(&ended, Duration::from_millis(100)).unwrap();
        let started = Instant::now();
        assert_eq!(ended.read(&mut [0u8; 1]).unwrap(), 0);
        assert!(started.elapsed() < Duration::from_secs(2));
        kept.set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let e = kept.read(&mut [0u8; 1]).unwrap_err();
        assert!(matches!(
            e.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
    }
}
