use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::Thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::command::args::GameMode;

pub const DISCORD_APP_ID: &str = "1563857609738690560";
const MODE_IMAGES: bool = false;
const IPC_DIR_ENV: &str = "VISE_DISCORD_IPC_DIR";

const BURST: usize = 5;
const WINDOW: Duration = Duration::from_secs(20);
const RETRY: Duration = Duration::from_secs(15);
const REFRESH: Duration = Duration::from_secs(60);

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    Singleplayer,
    Multiplayer,
    Hosting {
        players: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Menus,
    World {
        play: Play,
        dimension: Dimension,
        mode: GameMode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wanted {
    status: Status,
    since: Option<u64>,
}
impl Wanted {
    fn update(&mut self, status: Status, now: u64) -> bool {
        if self.status == status {
            return false;
        }
        let in_world = |s: Status| matches!(s, Status::World { .. });
        if !in_world(status) {
            self.since = None;
        } else if !in_world(self.status) {
            self.since = Some(now);
        }
        self.status = status;
        true
    }
}

struct Shared {
    wanted: Wanted,
    stop: bool,
    stopped: bool,
}
static SHARED: Mutex<Shared> = Mutex::new(Shared {
    wanted: Wanted {
        status: Status::Menus,
        since: None,
    },
    stop: false,
    stopped: false,
});
static STOPPED: Condvar = Condvar::new();
static THREAD: OnceLock<Thread> = OnceLock::new();

pub fn start(enabled: bool, automated: bool) {
    let fake_discord = std::env::var_os(IPC_DIR_ENV).is_some();
    if THREAD.get().is_some()
        || std::env::var_os("VISE_NO_DISCORD").is_some_and(|v| v != "0")
        || (automated && !fake_discord)
    {
        return;
    }
    if !enabled {
        lock().wanted.status = Status::Off;
    }
    match std::thread::Builder::new()
        .name("Discord presence".into())
        .spawn(run)
    {
        Ok(handle) => {
            let _ = THREAD.set(handle.thread().clone());
        }
        Err(e) => log::warn!("Vise: no Discord presence: {e}"),
    }
}

pub fn set(status: Status) {
    let Some(thread) = THREAD.get() else {
        return;
    };
    if lock().wanted.update(status, unix_now()) {
        thread.unpark();
    }
}

pub fn stop() {
    let Some(thread) = THREAD.get() else {
        return;
    };
    let mut shared = lock();
    shared.stop = true;
    thread.unpark();
    let _ = STOPPED.wait_timeout_while(shared, Duration::from_millis(300), |s| !s.stopped);
}

fn lock() -> std::sync::MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn run() {
    let mut client: Option<Client> = None;
    let mut sent: Option<(Wanted, Instant)> = None;
    let mut last_try: Option<Instant> = None;
    let mut limit = RateLimit::default();
    loop {
        let (wanted, stop) = {
            let s = lock();
            (s.wanted, s.stop)
        };
        if stop || wanted.status == Status::Off {
            if let Some(mut c) = client.take() {
                let _ = c.set_activity(None);
                c.close();
            }
            sent = None;
            if stop {
                lock().stopped = true;
                STOPPED.notify_all();
                return;
            }
            std::thread::park();
            continue;
        }
        let now = Instant::now();
        if client.is_none() {
            if let Some(wait) = last_try.and_then(|t| RETRY.checked_sub(now - t)) {
                std::thread::park_timeout(wait);
                continue;
            }
            last_try = Some(now);
            client = Client::connect();
            if client.is_none() {
                continue;
            }
        }
        let due = sent.is_none_or(|(w, at)| w != wanted || now - at >= REFRESH);
        if !due {
            let (_, at) = sent.expect("sent");
            std::thread::park_timeout(REFRESH - (now - at));
            continue;
        }
        let wait = limit.wait(now);
        if !wait.is_zero() {
            std::thread::park_timeout(wait);
            continue;
        }
        limit.record(now);
        let c = client.as_mut().expect("connected");
        match c.set_activity(activity(wanted.status, wanted.since)) {
            Ok(()) => sent = Some((wanted, now)),
            Err(e) => {
                log::info!("Vise: Discord presence disconnected ({e})");
                client = None;
                sent = None;
            }
        }
    }
}

fn activity(status: Status, since: Option<u64>) -> Option<Value> {
    let mut assets = json!({"large_image": "https://i.ibb.co/Q380m2C2/icon-1024.png", "large_text": "Vise"});
    let (details, state, party) = match status {
        Status::Off => return None,
        Status::Menus => ("In the menus", None, None),
        Status::World {
            play,
            dimension,
            mode,
        } => {
            if MODE_IMAGES {
                let (key, text) = match mode {
                    GameMode::Survival => ("survival", "Survival"),
                    GameMode::Creative => ("creative", "Creative"),
                    GameMode::Adventure => ("adventure", "Adventure"),
                    GameMode::Spectator => ("spectator", "Spectator"),
                };
                assets["small_image"] = key.into();
                assets["small_text"] = text.into();
            }
            let place = match dimension {
                Dimension::Overworld => "In the Overworld",
                Dimension::Nether => "In the Nether",
                Dimension::End => "In the End",
            };
            let (details, party) = match play {
                Play::Singleplayer => ("Singleplayer", None),
                Play::Multiplayer => ("Multiplayer", None),
                Play::Hosting { players } => ("Hosting a world", Some(players)),
            };
            (details, Some(place), party)
        }
    };
    let mut a = json!({"name": "Vise", "details": details, "assets": assets});
    if let Some(state) = state {
        a["state"] = state.into();
    }
    if let Some(start) = since {
        a["timestamps"] = json!({"start": start});
    }
    if let Some(players) = party {
        let max = crate::lan::MAX_PLAYERS;
        a["party"] = json!({
            "id": format!("vise-{}", std::process::id()),
            "size": [players.clamp(1, max), max],
        });
    }
    Some(a)
}

#[derive(Default)]
struct RateLimit {
    sent: VecDeque<Instant>,
}
impl RateLimit {
    fn wait(&self, now: Instant) -> Duration {
        if self.sent.len() < BURST {
            return Duration::ZERO;
        }
        (self.sent[0] + WINDOW).saturating_duration_since(now)
    }
    fn record(&mut self, now: Instant) {
        if self.sent.len() == BURST {
            self.sent.pop_front();
        }
        self.sent.push_back(now);
    }
}

fn encode(op: u32, payload: &Value) -> Vec<u8> {
    let body = payload.to_string();
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(body.as_bytes());
    frame
}

fn read_frame(r: &mut impl Read) -> io::Result<(u32, Value)> {
    let mut header = [0; 8];
    r.read_exact(&mut header)?;
    let op = u32::from_le_bytes(header[..4].try_into().expect("4 bytes"));
    let len = u32::from_le_bytes(header[4..].try_into().expect("4 bytes")) as usize;
    if len > 1 << 20 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    Ok((op, serde_json::from_slice(&body)?))
}

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

struct Client {
    pipe: Box<dyn Pipe>,
    nonce: u64,
}
impl Client {
    fn connect() -> Option<Self> {
        sockets().into_iter().find_map(|path| {
            let mut client = Client {
                pipe: open(&path).ok()?,
                nonce: 0,
            };
            match client.handshake() {
                Ok(()) => {
                    log::info!("Vise: connected to Discord for Rich Presence");
                    Some(client)
                }
                Err(e) => {
                    log::debug!("Vise: Discord at {} refused: {e}", path.display());
                    None
                }
            }
        })
    }
    fn handshake(&mut self) -> io::Result<()> {
        let hello = json!({"v": 1, "client_id": DISCORD_APP_ID});
        self.pipe.write_all(&encode(HANDSHAKE, &hello))?;
        self.reply(|frame| frame["evt"] == "READY")
    }
    fn set_activity(&mut self, activity: Option<Value>) -> io::Result<()> {
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let command = json!({
            "cmd": "SET_ACTIVITY",
            "args": {"pid": std::process::id(), "activity": activity},
            "nonce": nonce,
        });
        self.pipe.write_all(&encode(FRAME, &command))?;
        self.reply(|frame| frame["nonce"] == nonce.as_str())
    }
    fn reply(&mut self, ours: impl Fn(&Value) -> bool) -> io::Result<()> {
        loop {
            match read_frame(&mut self.pipe)? {
                (FRAME, frame) if ours(&frame) => {
                    if frame["evt"] == "ERROR" {
                        log::info!("Vise: Discord presence: {}", frame["data"]["message"]);
                    }
                    return Ok(());
                }
                (PING, frame) => self.pipe.write_all(&encode(PONG, &frame))?,
                (CLOSE, frame) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        frame["message"].as_str().unwrap_or("closed").to_owned(),
                    ));
                }
                _ => {}
            }
        }
    }
    fn close(mut self) {
        let _ = self.pipe.write_all(&encode(CLOSE, &json!({})));
    }
}

fn sockets() -> Vec<PathBuf> {
    sockets_with(|key| std::env::var_os(key), cfg!(windows))
}

fn sockets_with(var: impl Fn(&str) -> Option<std::ffi::OsString>, windows: bool) -> Vec<PathBuf> {
    let names = (0..10).map(|n| format!("discord-ipc-{n}"));
    if let Some(dir) = var(IPC_DIR_ENV) {
        return names.map(|n| PathBuf::from(&dir).join(n)).collect();
    }
    if windows {
        return names.map(|n| PathBuf::from(r"\\?\pipe\").join(n)).collect();
    }
    let mut dirs: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(&var)
        .map(PathBuf::from)
        .collect();
    dirs.push("/tmp".into());
    dirs.dedup();
    let mut paths = Vec::new();
    for dir in dirs {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
            paths.extend(names.clone().map(|n| dir.join(sub).join(n)));
        }
    }
    paths
}

#[cfg(unix)]
fn open(path: &std::path::Path) -> io::Result<Box<dyn Pipe>> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok(Box::new(stream))
}

#[cfg(windows)]
fn open(path: &std::path::Path) -> io::Result<Box<dyn Pipe>> {
    let pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    Ok(Box::new(pipe))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(play: Play, dimension: Dimension) -> Status {
        Status::World {
            play,
            dimension,
            mode: GameMode::Survival,
        }
    }

    #[test]
    fn sockets_cover_where_discord_listens() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| std::ffi::OsString::from(v))
            }
        };
        let macos = sockets_with(env(&[("TMPDIR", "/var/folders/xy/abc/T/")]), false);
        assert_eq!(
            macos[0],
            PathBuf::from("/var/folders/xy/abc/T/discord-ipc-0")
        );
        assert!(macos.contains(&PathBuf::from("/var/folders/xy/abc/T/discord-ipc-9")));
        assert!(macos.contains(&PathBuf::from("/tmp/discord-ipc-0")));

        let linux = sockets_with(
            env(&[("XDG_RUNTIME_DIR", "/run/user/1000"), ("TMPDIR", "/tmp")]),
            false,
        );
        assert_eq!(linux[0], PathBuf::from("/run/user/1000/discord-ipc-0"));
        for path in [
            "/run/user/1000/app/com.discordapp.Discord/discord-ipc-0",
            "/run/user/1000/snap.discord/discord-ipc-0",
            "/tmp/discord-ipc-0",
        ] {
            assert!(linux.contains(&PathBuf::from(path)), "{path}");
        }
        assert_eq!(linux.len(), 2 * 3 * 10, "/tmp is listed once");

        let bare = sockets_with(env(&[]), false);
        assert_eq!(bare[0], PathBuf::from("/tmp/discord-ipc-0"));

        let windows = sockets_with(env(&[("TEMP", r"C:\Temp")]), true);
        assert_eq!(windows.len(), 10);
        #[cfg(windows)]
        assert_eq!(windows[0], PathBuf::from(r"\\?\pipe\discord-ipc-0"));

        let fake = sockets_with(
            env(&[("VISE_DISCORD_IPC_DIR", "/tmp/hz"), ("TMPDIR", "/x")]),
            false,
        );
        assert_eq!(fake.len(), 10);
        assert_eq!(fake[0], PathBuf::from("/tmp/hz/discord-ipc-0"));
    }

    #[test]
    fn frames_round_trip() {
        let frame = encode(FRAME, &json!({"cmd": "SET_ACTIVITY"}));
        assert_eq!(&frame[..4], &[1, 0, 0, 0]);
        assert_eq!(frame[4..8], (frame.len() as u32 - 8).to_le_bytes());
        let (op, value) = read_frame(&mut frame.as_slice()).unwrap();
        assert_eq!((op, value), (FRAME, json!({"cmd": "SET_ACTIVITY"})));
        assert!(read_frame(&mut &frame[..6]).is_err());
        let huge = [1, 0, 0, 0, 0, 0, 0, 2];
        assert!(read_frame(&mut huge.as_slice()).is_err());
    }

    #[test]
    fn activities_say_what_the_player_does_and_nothing_private() {
        assert_eq!(activity(Status::Off, None), None);
        let menus = activity(Status::Menus, None).unwrap();
        assert_eq!(menus["details"], "In the menus");
        assert!(menus.get("state").is_none() && menus.get("timestamps").is_none());
        assert_eq!(menus["assets"]["large_image"], "https://i.ibb.co/Q380m2C2/icon-1024.png");

        let single = activity(world(Play::Singleplayer, Dimension::Nether), Some(1000)).unwrap();
        assert_eq!(single["details"], "Singleplayer");
        assert_eq!(single["state"], "In the Nether");
        assert_eq!(single["timestamps"]["start"], 1000);
        assert!(single.get("party").is_none());
        assert!(single["assets"].get("small_image").is_none());

        let multi = activity(world(Play::Multiplayer, Dimension::End), Some(5)).unwrap();
        assert_eq!(multi["details"], "Multiplayer");
        assert_eq!(multi["state"], "In the End");

        let host = world(Play::Hosting { players: 3 }, Dimension::Overworld);
        let host = activity(host, Some(5)).unwrap();
        assert_eq!(host["details"], "Hosting a world");
        assert_eq!(host["state"], "In the Overworld");
        assert_eq!(host["party"]["size"], json!([3, 8]));
    }

    #[test]
    fn time_counts_from_entering_the_world() {
        let mut w = Wanted {
            status: Status::Menus,
            since: None,
        };
        assert!(!w.update(Status::Menus, 10));
        assert!(w.update(world(Play::Singleplayer, Dimension::Overworld), 20));
        assert_eq!(w.since, Some(20));
        assert!(w.update(world(Play::Singleplayer, Dimension::Nether), 30));
        assert!(w.update(world(Play::Hosting { players: 1 }, Dimension::Nether), 40));
        assert_eq!(w.since, Some(20));
        assert!(w.update(Status::Menus, 50));
        assert_eq!(w.since, None);
        assert!(w.update(world(Play::Multiplayer, Dimension::Overworld), 60));
        assert_eq!(w.since, Some(60));
        assert!(w.update(Status::Off, 70));
        assert_eq!(w.since, None);
    }

    #[test]
    fn at_most_five_updates_in_twenty_seconds() {
        let t = Instant::now();
        let mut limit = RateLimit::default();
        for i in 0..5 {
            let now = t + Duration::from_secs(i);
            assert_eq!(limit.wait(now), Duration::ZERO);
            limit.record(now);
        }
        assert_eq!(
            limit.wait(t + Duration::from_secs(5)),
            Duration::from_secs(15)
        );
        assert_eq!(limit.wait(t + WINDOW), Duration::ZERO);
        limit.record(t + WINDOW);
        assert_eq!(limit.wait(t + WINDOW), Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn talks_to_discord_over_its_socket() {
        let dir = std::env::temp_dir().join(format!("vise-discord-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("discord-ipc-0");
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let (op, hello) = read_frame(&mut s).unwrap();
            assert_eq!((op, &hello["v"]), (HANDSHAKE, &json!(1)));
            let ready = json!({"cmd": "DISPATCH", "evt": "READY", "data": {"v": 1}});
            s.write_all(&encode(FRAME, &ready)).unwrap();
            let mut got = Vec::new();
            for _ in 0..2 {
                let (op, cmd) = read_frame(&mut s).unwrap();
                assert_eq!(op, FRAME);
                s.write_all(&encode(PING, &json!({"n": 1}))).unwrap();
                assert_eq!(read_frame(&mut s).unwrap(), (PONG, json!({"n": 1})));
                let reply = json!({"cmd": "SET_ACTIVITY", "nonce": cmd["nonce"], "data": null});
                s.write_all(&encode(FRAME, &reply)).unwrap();
                got.push(cmd);
            }
            got
        });
        let mut client = Client {
            pipe: open(&path).unwrap(),
            nonce: 0,
        };
        client.handshake().unwrap();
        let shown = activity(world(Play::Singleplayer, Dimension::End), Some(7));
        client.set_activity(shown.clone()).unwrap();
        client.set_activity(None).unwrap();
        let got = server.join().unwrap();
        assert_eq!(got[0]["cmd"], "SET_ACTIVITY");
        assert_eq!(got[0]["args"]["pid"], std::process::id());
        assert_eq!(got[0]["args"]["activity"], shown.unwrap());
        assert_eq!(got[1]["args"]["activity"], Value::Null);
        assert_ne!(got[0]["nonce"], got[1]["nonce"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
