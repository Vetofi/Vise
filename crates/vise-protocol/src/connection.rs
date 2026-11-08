use crate::{Error, Result, buf::Reader, crypto::Cfb8, frame};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Channel {
    state: Mutex<(std::collections::VecDeque<u8>, bool)>,
    ready: Condvar,
}
impl Channel {
    fn close(&self) {
        self.state.lock().unwrap().1 = true;
        self.ready.notify_all();
    }
}
struct End {
    incoming: Arc<Channel>,
    outgoing: Arc<Channel>,
}
impl Drop for End {
    fn drop(&mut self) {
        self.incoming.close();
        self.outgoing.close();
    }
}
pub struct Pipe {
    end: Arc<End>,
    timeout: Mutex<Option<Duration>>,
}
impl Pipe {
    pub fn pair() -> (Self, Self) {
        let (a, b) = (Arc::new(Channel::default()), Arc::new(Channel::default()));
        let end = |incoming, outgoing| Self {
            end: Arc::new(End { incoming, outgoing }),
            timeout: Mutex::new(None),
        };
        (end(a.clone(), b.clone()), end(b, a))
    }
    fn try_clone(&self) -> Self {
        Self {
            end: self.end.clone(),
            timeout: Mutex::new(*self.timeout.lock().unwrap()),
        }
    }
    pub fn shutdown(&self) {
        self.end.incoming.close();
        self.end.outgoing.close();
    }
}
impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let deadline = self.timeout.lock().unwrap().map(|t| Instant::now() + t);
        let mut state = self.end.incoming.state.lock().unwrap();
        while state.0.is_empty() && !state.1 {
            state = match deadline {
                None => self.end.incoming.ready.wait(state).unwrap(),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(std::io::ErrorKind::TimedOut.into());
                    }
                    self.end.incoming.ready.wait_timeout(state, left).unwrap().0
                }
            };
        }
        let n = out.len().min(state.0.len());
        for (o, b) in out.iter_mut().zip(state.0.drain(..n)) {
            *o = b;
        }
        Ok(n)
    }
}
impl Write for Pipe {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut state = self.end.outgoing.state.lock().unwrap();
        if state.1 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        state.0.extend(data);
        self.end.outgoing.ready.notify_all();
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

enum Stream {
    Tcp(TcpStream),
    Memory(Pipe),
}
impl Stream {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(match self {
            Self::Tcp(s) => Self::Tcp(s.try_clone()?),
            Self::Memory(p) => Self::Memory(p.try_clone()),
        })
    }
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.set_read_timeout(timeout),
            Self::Memory(p) => {
                *p.timeout.lock().unwrap() = timeout;
                Ok(())
            }
        }
    }
    fn shutdown(&self) {
        match self {
            Self::Tcp(s) => {
                let _ = s.shutdown(Shutdown::Both);
            }
            Self::Memory(p) => p.shutdown(),
        }
    }
}
impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(out),
            Self::Memory(p) => p.read(out),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(data),
            Self::Memory(p) => p.write(data),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            Self::Memory(p) => p.flush(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Counters {
    pub received: AtomicI64,
    pub sent: AtomicI64,
    pub bytes_in: AtomicI64,
    pub bytes_out: AtomicI64,
}

pub struct ConnReader {
    stream: Stream,
    buffer: Vec<u8>,
    decrypt: Option<Cfb8>,
    pub threshold: Option<usize>,
    pub counters: Arc<Counters>,
}
pub struct Closer(Stream);
impl Closer {
    pub fn close(&self) {
        self.0.shutdown();
    }
}

pub struct ConnWriter {
    stream: Stream,
    encrypt: Option<Cfb8>,
    pub threshold: Option<usize>,
    counters: Arc<Counters>,
}
fn ipv4_first(mut addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    addrs.sort_by_key(SocketAddr::is_ipv6);
    addrs
}
const ATTEMPT_DELAY: Duration = Duration::from_millis(250);
pub struct Connection {
    pub reader: ConnReader,
    pub writer: ConnWriter,
}
impl Connection {
    pub fn connect(host: &str, port: u16, timeout: Duration) -> Result<Self> {
        let addrs: Vec<_> = (host, port)
            .to_socket_addrs()
            .map_err(|e| Error::new(format!("Unknown host {host}: {e}")))?
            .collect();
        Self::connect_addresses(host, addrs, timeout)
    }
    pub fn connect_server(host: &str, port: u16, timeout: Duration) -> Result<Self> {
        let addrs = crate::resolver::resolve(host, port)?;
        Self::connect_addresses(host, addrs, timeout)
    }
    fn connect_addresses(host: &str, addrs: Vec<SocketAddr>, timeout: Duration) -> Result<Self> {
        let addrs = ipv4_first(addrs);
        let (send, recv) = std::sync::mpsc::channel();
        let mut errors: Vec<Option<std::io::Error>> = addrs.iter().map(|_| None).collect();
        let (mut started, mut pending) = (0, 0);
        loop {
            if let Some(&addr) = addrs.get(started) {
                let (send, index) = (send.clone(), started);
                let spawned = std::thread::Builder::new()
                    .name("vise-connect".into())
                    .spawn(move || {
                        let _ = send.send((index, TcpStream::connect_timeout(&addr, timeout)));
                    });
                match spawned {
                    Ok(_) => pending += 1,
                    Err(e) => errors[index] = Some(e),
                }
                started += 1;
            }
            if pending == 0 {
                if started < addrs.len() {
                    continue;
                }
                break;
            }
            let reply = if started < addrs.len() {
                match recv.recv_timeout(ATTEMPT_DELAY) {
                    Ok(reply) => reply,
                    Err(_) => continue,
                }
            } else {
                recv.recv().expect("a sender is held here")
            };
            pending -= 1;
            match reply {
                (_, Ok(stream)) => return Self::from_stream(stream),
                (index, Err(e)) => errors[index] = Some(e),
            }
        }
        match errors.into_iter().flatten().next() {
            Some(e) => Err(Error::new(format!("Connection refused: {e}"))),
            None => Err(Error::new(format!("Unknown host {host}"))),
        }
    }
    pub fn from_stream(stream: TcpStream) -> Result<Self> {
        stream.set_nodelay(true)?;
        Self::over(Stream::Tcp(stream))
    }
    pub fn local(pipe: Pipe) -> Self {
        Self::over(Stream::Memory(pipe)).expect("a pipe end clones")
    }
    fn over(stream: Stream) -> Result<Self> {
        let counters = Arc::new(Counters::default());
        let write = stream.try_clone()?;
        Ok(Self {
            reader: ConnReader {
                stream,
                buffer: Vec::new(),
                decrypt: None,
                threshold: None,
                counters: counters.clone(),
            },
            writer: ConnWriter {
                stream: write,
                encrypt: None,
                threshold: None,
                counters,
            },
        })
    }
    pub fn is_memory(&self) -> bool {
        matches!(self.reader.stream, Stream::Memory(_))
    }
    pub fn set_compression(&mut self, threshold: i32) {
        let t = usize::try_from(threshold).ok();
        self.reader.threshold = t;
        self.writer.threshold = t;
    }
    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        let mut decrypt = Cfb8::new(secret, secret);
        decrypt.decrypt(&mut self.reader.buffer);
        self.reader.decrypt = Some(decrypt);
        self.writer.encrypt = Some(Cfb8::new(secret, secret));
    }
    pub fn send(&mut self, body: &[u8]) -> Result<()> {
        self.writer.send(body)
    }
    pub fn recv(&mut self) -> Result<(i32, Vec<u8>)> {
        self.reader.recv()
    }
    pub fn split(self) -> (ConnReader, ConnWriter) {
        (self.reader, self.writer)
    }
}
impl ConnReader {
    pub fn recv(&mut self) -> Result<(i32, Vec<u8>)> {
        loop {
            if let Some(frame) = frame::split(&mut self.buffer)? {
                let body = frame::decompress(frame, self.threshold)?;
                if body.is_empty() {
                    continue;
                }
                let mut r = Reader::new(&body);
                let id = r.varint()?;
                let start = r.position();
                self.counters.received.fetch_add(1, Ordering::Relaxed);
                return Ok((id, body[start..].to_vec()));
            }
            let mut chunk = [0u8; 16384];
            let n = match self.stream.read(&mut chunk) {
                Ok(0) => return Err(Error::end_of_stream()),
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err(Error::timed_out());
                }
                Err(e) => return Err(e.into()),
            };
            self.counters
                .bytes_in
                .fetch_add(n as i64, Ordering::Relaxed);
            let data = &mut chunk[..n];
            if let Some(c) = &mut self.decrypt {
                c.decrypt(data);
            }
            self.buffer.extend_from_slice(data);
        }
    }
    pub fn closer(&self) -> Result<Closer> {
        Ok(Closer(self.stream.try_clone()?))
    }
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.stream.set_read_timeout(timeout)?)
    }
}
impl ConnWriter {
    pub fn send(&mut self, body: &[u8]) -> Result<()> {
        self.send_contents(&frame::contents(body, self.threshold)?)
    }
    pub fn send_contents(&mut self, contents: &[u8]) -> Result<()> {
        let mut framed = frame::prefixed(contents);
        if let Some(c) = &mut self.encrypt {
            c.encrypt(&mut framed);
        }
        self.stream.write_all(&framed)?;
        self.counters.sent.fetch_add(1, Ordering::Relaxed);
        self.counters
            .bytes_out
            .fetch_add(framed.len() as i64, Ordering::Relaxed);
        Ok(())
    }
    pub fn shutdown(&self) {
        self.stream.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn ipv4_first_keeps_each_familys_order() {
        let a = |s: &str| s.parse::<SocketAddr>().unwrap();
        let resolved = vec![
            a("[2001:db8::1]:25565"),
            a("192.0.2.1:25565"),
            a("[2001:db8::2]:25565"),
            a("10.0.0.1:25565"),
        ];
        assert_eq!(
            ipv4_first(resolved),
            [
                a("192.0.2.1:25565"),
                a("10.0.0.1:25565"),
                a("[2001:db8::1]:25565"),
                a("[2001:db8::2]:25565"),
            ]
        );
    }

    fn peer(c: &Connection) -> SocketAddr {
        match &c.reader.stream {
            Stream::Tcp(s) => s.peer_addr().unwrap(),
            Stream::Memory(_) => panic!("not a socket"),
        }
    }

    fn ipv6_loopback() -> Option<TcpListener> {
        let listener = TcpListener::bind("[::1]:0");
        if listener.is_err() {
            eprintln!("skipped: no IPv6 loopback");
        }
        listener.ok()
    }

    #[test]
    fn connects_to_ipv4_although_the_resolver_lists_ipv6_first() {
        let Some(v6_listener) = ipv6_loopback() else {
            return;
        };
        let v4_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let v6 = v6_listener.local_addr().unwrap();
        let v4 = v4_listener.local_addr().unwrap();
        let c =
            Connection::connect_addresses("test", vec![v6, v4], Duration::from_secs(5)).unwrap();
        assert_eq!(peer(&c), v4);
    }

    #[test]
    fn falls_back_to_ipv6_when_ipv4_refuses() {
        let Some(v6_listener) = ipv6_loopback() else {
            return;
        };
        let v6 = v6_listener.local_addr().unwrap();
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let c = Connection::connect_addresses("test", vec![v6, closed], Duration::from_secs(5))
            .unwrap();
        assert_eq!(peer(&c), v6);
    }

    #[test]
    fn a_dead_address_does_not_hold_up_the_next() {
        let dead: SocketAddr = "192.0.2.1:25565".parse().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let live = listener.local_addr().unwrap();
        let start = Instant::now();
        let c = Connection::connect_addresses("test", vec![dead, live], Duration::from_secs(10))
            .unwrap();
        assert_eq!(peer(&c), live);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn every_failure_reports_the_first_address() {
        let closed = |_| {
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
        };
        let error = |host, addrs| {
            Connection::connect_addresses(host, addrs, Duration::from_secs(5))
                .err()
                .unwrap()
                .to_string()
        };
        let refused = error("test", (0..3).map(closed).collect());
        assert!(refused.starts_with("Connection refused"), "{refused}");
        assert_eq!(error("nowhere", Vec::new()), "Unknown host nowhere");
    }

    #[test]
    fn a_silent_peer_times_out_and_a_closed_one_ends_the_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
        let (server, _) = listener.accept().unwrap();
        c.reader
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let silent = c.recv().unwrap_err();
        assert!(silent.is_timed_out(), "{silent}");
        assert_eq!(silent.to_string(), "Timed out");
        drop(server);
        let closed = c.recv().unwrap_err();
        assert!(closed.is_end_of_stream(), "{closed}");
    }

    #[test]
    fn loopback_with_compression_and_encryption() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut c = Connection::from_stream(stream).unwrap();
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.as_slice()), (0, &b"plain"[..]));
            c.enable_encryption(&[7; 16]);
            c.set_compression(64);
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.len()), (5, 1000));
            let mut body = vec![9];
            body.extend_from_slice(b"reply");
            c.send(&body).unwrap();
        });
        let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
        let mut body = vec![0];
        body.extend_from_slice(b"plain");
        c.send(&body).unwrap();
        c.enable_encryption(&[7; 16]);
        c.set_compression(64);
        let mut big = vec![5];
        big.extend(std::iter::repeat_n(3u8, 1000));
        c.send(&big).unwrap();
        let (id, payload) = c.recv().unwrap();
        assert_eq!((id, payload.as_slice()), (9, &b"reply"[..]));
        server.join().unwrap();
    }

    #[test]
    fn local_pipe_carries_packets_and_closes() {
        let (a, b) = Pipe::pair();
        let server = std::thread::spawn(move || {
            let mut c = Connection::local(b);
            c.set_compression(64);
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.len()), (5, 1000));
            c.send(&[9, 1, 2]).unwrap();
            let (_, writer) = c.split();
            writer.shutdown();
        });
        let mut c = Connection::local(a);
        c.set_compression(64);
        let mut big = vec![5];
        big.extend(std::iter::repeat_n(3u8, 1000));
        c.send(&big).unwrap();
        assert_eq!(c.recv().unwrap(), (9, vec![1, 2]));
        assert_eq!(c.reader.counters.sent.load(Ordering::Relaxed), 1);
        assert_eq!(c.reader.counters.received.load(Ordering::Relaxed), 1);
        server.join().unwrap();
        assert!(c.recv().is_err());
    }

    #[test]
    fn dropping_a_pipe_end_closes_it() {
        let (a, b) = Pipe::pair();
        let mut c = Connection::local(a);
        drop(b);
        assert!(c.recv().is_err());
        assert!(c.send(&[1]).is_err());
    }

    #[test]
    fn local_pipe_read_times_out() {
        let (a, _b) = Pipe::pair();
        let mut c = Connection::local(a);
        c.reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        assert!(c.recv().is_err());
    }

    #[test]
    fn encrypted_bytes_read_before_enabling_encryption() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sent, both_sent) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
            c.send(&[0, 1, 2]).unwrap();
            c.enable_encryption(&[7; 16]);
            c.send(&[5, 4, 3]).unwrap();
            sent.send(c).unwrap();
        });
        let (stream, _) = listener.accept().unwrap();
        let mut c = Connection::from_stream(stream).unwrap();
        let _keep_open = both_sent.recv().unwrap();
        assert_eq!(c.recv().unwrap(), (0, vec![1, 2]));
        c.enable_encryption(&[7; 16]);
        assert_eq!(c.recv().unwrap(), (5, vec![4, 3]));
        client.join().unwrap();
    }

    #[test]
    fn an_empty_compressed_frame_is_skipped() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut c = Connection::from_stream(stream).unwrap();
            c.set_compression(64);
            c.writer.send_contents(&[0]).unwrap();
            let mut body = vec![3];
            body.extend_from_slice(b"after");
            c.writer.send(&body).unwrap();
        });
        let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
        c.set_compression(64);
        let (id, payload) = c.recv().unwrap();
        assert_eq!((id, payload.as_slice()), (3, &b"after"[..]));
        server.join().unwrap();
    }
}
