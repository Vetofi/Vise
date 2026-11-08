mod advancements;
pub mod anticheat;
pub mod attributes;
pub mod border;
pub mod chunks;
mod command_order;
pub mod command_suggestions;
pub mod commands;
pub mod conn;
pub mod containers;
pub mod data;
pub mod discovery;
pub mod entity;
pub mod guest;
pub mod hub;
pub mod menu;
pub mod playerdata;
mod recipes;
pub mod scoreboard;
pub mod tracking;
pub mod waypoints;

pub use conn::{Holding, PlayerSender, flush_all};
use vise_protocol::packets::play;
use vise_protocol::{
    GAME_VERSION, PROTOCOL_VERSION, crypto::uuid_string, packets::configuration::ClientInformation,
    text::translatable,
};
use std::{
    collections::HashSet,
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
    time::Duration,
};

pub const MAX_PLAYERS: usize = 8;

pub enum NetEvent {
    Joined(Box<Joined>),
    Packet(u32, Box<play::Serverbound>, usize),
    Left(u32, String),
    Duplicate(u128),
}
pub struct Joined {
    pub conn: u32,
    pub uuid: u128,
    pub name: String,
    pub properties: Vec<vise_protocol::packets::login::Property>,
    pub info: ClientInformation,
    pub brand: String,
    pub sender: PlayerSender,
    pub latency: Arc<AtomicI32>,
    pub address: SocketAddr,
}
pub enum Out {
    Packet(Vec<u8>),
    Packets(Vec<Vec<u8>>),
    Disconnect(vise_world::nbt::Tag),
    Close,
}

pub type LoginGate =
    Arc<dyn Fn(SocketAddr, u128, &str) -> Option<vise_world::nbt::Tag> + Send + Sync>;

pub struct OnlineMode {
    pub key: vise_protocol::rsa::RsaPrivateKey,
    pub public_der: Vec<u8>,
    pub prevent_proxy: bool,
}

impl OnlineMode {
    pub fn generate(prevent_proxy: bool) -> Result<Self, String> {
        let key = vise_protocol::rsa::RsaPrivateKey::generate(1024)
            .map_err(|e| format!("Failed to generate key pair: {e}"))?;
        Ok(Self {
            public_der: key.public_der(),
            key,
            prevent_proxy,
        })
    }
    pub fn lan() -> Option<Arc<Self>> {
        crate::auth::current()?;
        Self::generate(false)
            .map_err(|e| log::warn!("LAN stays in offline mode: {e}"))
            .ok()
            .map(Arc::new)
    }
}

#[derive(Clone)]
pub struct NetLog {
    pub write: Arc<dyn Fn(&str, &str, &str) + Send + Sync>,
    pub lang: Arc<vise_protocol::text::Language>,
    pub log_ips: bool,
}

pub const VISE_CLIENTS_ONLY: &str = "vise-clients-only";
pub const VISE_CLIENTS_ONLY_REASON: &str =
    "This server is for the Vise client: playvise.dev";

#[derive(Clone)]
pub struct NetConfig {
    pub bind: Option<std::net::IpAddr>,
    pub port: u16,
    pub motd: String,
    pub max_players: usize,
    pub compression_threshold: i32,
    pub rate_limit: i32,
    pub enable_status: bool,
    pub hide_online_players: bool,
    pub favicon: Option<Vec<u8>>,
    pub secure_chat: Option<Arc<SecureChat>>,
    pub accepts_transfers: bool,
    pub announce: bool,
    pub listen: bool,
    pub bypasses_limit: Option<Arc<dyn Fn(u128) -> bool + Send + Sync>>,
    pub gate: Option<LoginGate>,
    pub online: Option<Arc<OnlineMode>>,
    pub log: Option<NetLog>,
    pub resource_pack: Option<crate::server::properties::ResourcePackInfo>,
    pub server_links: Vec<vise_protocol::packets::configuration::ServerLink>,
    pub owner_profile: Option<Arc<OwnerProfile>>,
    pub vise_clients_only: bool,
    pub levels: crate::chunk::LevelShapes,
}
impl NetConfig {
    pub fn enforces_secure_chat(&self) -> bool {
        self.secure_chat.as_ref().is_some_and(|s| s.enforced())
    }
    pub fn lan(port: u16, motd: String, max_players: usize) -> Self {
        Self {
            bind: None,
            port,
            motd,
            max_players,
            compression_threshold: 256,
            rate_limit: 0,
            enable_status: true,
            hide_online_players: false,
            favicon: None,
            secure_chat: None,
            accepts_transfers: false,
            announce: true,
            listen: true,
            bypasses_limit: None,
            gate: None,
            online: None,
            log: None,
            resource_pack: None,
            server_links: Vec::new(),
            vise_clients_only: false,
            levels: crate::chunk::LevelShapes::VANILLA,
            owner_profile: None,
        }
    }
}

pub struct OwnerProfile {
    properties: std::sync::OnceLock<Vec<vise_protocol::packets::login::Property>>,
    fetch: Mutex<Receiver<Vec<vise_protocol::packets::login::Property>>>,
}

impl OwnerProfile {
    pub fn new(
        fetch: impl FnOnce() -> Vec<vise_protocol::packets::login::Property> + Send + 'static,
    ) -> Self {
        let (send, receive) = channel();
        let _ = std::thread::Builder::new()
            .name("Profile fetcher".into())
            .spawn(move || {
                let _ = send.send(fetch());
            });
        Self {
            properties: std::sync::OnceLock::new(),
            fetch: Mutex::new(receive),
        }
    }
    pub fn properties(&self) -> &[vise_protocol::packets::login::Property] {
        if let Some(properties) = self.properties.get() {
            return properties;
        }
        let received = self.fetch.lock().ok().and_then(|r| match r.try_recv() {
            Ok(properties) => Some(properties),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
        });
        match received {
            Some(properties) => self.properties.get_or_init(|| properties),
            None => &[],
        }
    }
}

pub struct SecureChat {
    pub enforce: bool,
    keys: std::sync::OnceLock<Vec<vise_protocol::rsa::RsaPublicKey>>,
    fetch: Mutex<Option<JoinHandle<Vec<vise_protocol::rsa::RsaPublicKey>>>>,
}

impl SecureChat {
    pub fn new(
        enforce: bool,
        fetch: impl FnOnce() -> Vec<vise_protocol::rsa::RsaPublicKey> + Send + 'static,
    ) -> Self {
        let handle = std::thread::Builder::new()
            .name("Services keys".into())
            .spawn(fetch)
            .ok();
        Self {
            enforce,
            keys: std::sync::OnceLock::new(),
            fetch: Mutex::new(handle),
        }
    }
    pub fn keys(&self) -> &[vise_protocol::rsa::RsaPublicKey] {
        self.keys.get_or_init(|| {
            let handle = self.fetch.lock().ok().and_then(|mut h| h.take());
            handle.and_then(|h| h.join().ok()).unwrap_or_default()
        })
    }
    pub fn enforced(&self) -> bool {
        self.enforce && !self.keys().is_empty()
    }
}

pub struct Shared {
    pub sync: Arc<data::SyncData>,
    pub events: Sender<NetEvent>,
    pub config: NetConfig,
    motd: Mutex<String>,
    max_players: AtomicUsize,
    online: Mutex<Vec<(u128, String)>>,
    reserved: Mutex<HashSet<u128>>,
    next: AtomicU32,
    stop: AtomicBool,
    random: Mutex<u64>,
}
impl Shared {
    pub fn next_id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
    pub fn try_admit(
        &self,
        address: SocketAddr,
        uuid: u128,
        name: &str,
    ) -> Option<vise_world::nbt::Tag> {
        if let Some(gate) = &self.config.gate
            && let Some(reason) = gate(address, uuid, name)
        {
            return Some(reason);
        }
        if self.reserved.lock().unwrap().contains(&uuid) {
            return Some(translatable("multiplayer.disconnect.duplicate_login", &[]));
        }
        {
            let online = self.online.lock().unwrap();
            let bypass = self.config.bypasses_limit.as_ref().is_some_and(|b| b(uuid));
            let taken = online.len() + self.reserved.lock().unwrap().len();
            if taken >= self.max_players() && !bypass {
                return Some(translatable("multiplayer.disconnect.server_full", &[]));
            }
        }
        if self.is_online(uuid) {
            let _ = self.events.send(NetEvent::Duplicate(uuid));
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            while self.is_online(uuid) {
                if std::time::Instant::now() > deadline {
                    return Some(translatable("multiplayer.disconnect.slow_login", &[]));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.online.lock().unwrap().push((uuid, name.to_owned()));
        None
    }
    fn is_online(&self, uuid: u128) -> bool {
        self.online.lock().unwrap().iter().any(|(u, _)| *u == uuid)
    }
    pub fn motd(&self) -> String {
        self.motd.lock().unwrap().clone()
    }
    pub fn max_players(&self) -> usize {
        self.max_players.load(Ordering::Relaxed)
    }
    pub fn release(&self, uuid: u128) {
        self.online.lock().unwrap().retain(|(u, _)| *u != uuid);
    }
    pub fn online(&self) -> usize {
        self.online.lock().unwrap().len() + self.reserved.lock().unwrap().len()
    }
    fn next_random(&self, bound: usize) -> usize {
        let mut r = self.random.lock().unwrap();
        *r = r
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((*r >> 33) as usize) % bound.max(1)
    }
    pub fn status_json(&self) -> String {
        use crate::server::json::Json;
        let motd = self.motd.lock().unwrap().clone();
        let online: Vec<(u128, String)> = self.online.lock().unwrap().clone();
        let count = self.online();
        let mut players = vec![
            (
                "max".to_owned(),
                Json::Number(self.max_players().to_string()),
            ),
            ("online".to_owned(), Json::Number(count.to_string())),
        ];
        if !self.config.hide_online_players && !online.is_empty() {
            let size = online.len().min(12);
            let offset = self.next_random(online.len() - size + 1);
            let mut sample: Vec<(u128, String)> = online[offset..offset + size].to_vec();
            for i in (1..sample.len()).rev() {
                let j = self.next_random(i + 1);
                sample.swap(i, j);
            }
            players.push((
                "sample".to_owned(),
                Json::Array(
                    sample
                        .into_iter()
                        .map(|(u, n)| {
                            Json::Object(vec![
                                ("id".to_owned(), Json::String(uuid_string(u))),
                                ("name".to_owned(), Json::String(n)),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        let mut status = vec![
            ("description".to_owned(), Json::String(motd)),
            ("players".to_owned(), Json::Object(players)),
            (
                "version".to_owned(),
                Json::Object(vec![
                    ("name".to_owned(), Json::String(GAME_VERSION.to_owned())),
                    (
                        "protocol".to_owned(),
                        Json::Number(PROTOCOL_VERSION.to_string()),
                    ),
                ]),
            ),
        ];
        if let Some(icon) = &self.config.favicon {
            status.push((
                "favicon".to_owned(),
                Json::String(format!("data:image/png;base64,{}", base64(icon))),
            ));
        }
        if self.config.enforces_secure_chat() {
            status.push(("enforcesSecureChat".to_owned(), Json::Bool(true)));
        }
        Json::Object(status).compact()
    }
}

pub fn base64(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(char::from(A[(n >> 18) as usize & 63]));
        out.push(char::from(A[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(A[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(A[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

pub struct Lan {
    pub shared: Arc<Shared>,
    events: Receiver<NetEvent>,
    pub port: u16,
    listener: Option<JoinHandle<()>>,
    listener_stop: Arc<AtomicBool>,
    _pinger: Option<discovery::Pinger>,
}
impl Lan {
    pub fn start(
        sync: Arc<data::SyncData>,
        port: u16,
        motd: String,
        max_players: usize,
    ) -> io::Result<Self> {
        Self::start_with(sync, NetConfig::lan(port, motd, max_players))
    }
    pub fn start_with(sync: Arc<data::SyncData>, config: NetConfig) -> io::Result<Self> {
        let (tx, events) = channel();
        let (listen, port, announce) = (config.listen, config.port, config.announce);
        let shared = Arc::new(Shared {
            sync,
            events: tx,
            motd: Mutex::new(config.motd.clone()),
            max_players: AtomicUsize::new(config.max_players),
            online: Mutex::new(Vec::new()),
            reserved: Mutex::new(HashSet::new()),
            next: AtomicU32::new(1),
            stop: AtomicBool::new(false),
            random: Mutex::new(crate::server::java_properties::now_millis() as u64 | 1),
            config,
        });
        let mut lan = Self {
            shared,
            events,
            port: 0,
            listener: None,
            listener_stop: Arc::new(AtomicBool::new(false)),
            _pinger: None,
        };
        if listen {
            lan.listen(port, announce)?;
        }
        Ok(lan)
    }
    pub fn listen(&mut self, port: u16, announce: bool) -> io::Result<u16> {
        if self.listener.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "already listening",
            ));
        }
        let listener = Self::bind(self.shared.config.bind, port)?;
        self.listen_on(listener, announce)
    }
    pub fn bind(address: Option<std::net::IpAddr>, port: u16) -> io::Result<TcpListener> {
        let bind = address.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        let listener = TcpListener::bind((bind, port))?;
        listener.set_nonblocking(true)?;
        Ok(listener)
    }
    pub fn listen_on(&mut self, listener: TcpListener, announce: bool) -> io::Result<u16> {
        if self.listener.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "already listening",
            ));
        }
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let (accept_shared, accept_stop) = (self.shared.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("vise-lan-accept".into())
            .spawn(move || accept_loop(listener, accept_shared, &accept_stop))?;
        self._pinger = if announce {
            discovery::Pinger::start(self.shared.motd(), port).ok()
        } else {
            None
        };
        self.listener = Some(thread);
        self.listener_stop = stop;
        self.port = port;
        Ok(port)
    }
    pub fn stop_listening(&mut self) {
        self.listener_stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.listener.take() {
            let _ = t.join();
        }
        self._pinger = None;
        self.port = 0;
    }
    pub fn listening(&self) -> bool {
        self.listener.is_some()
    }
    pub fn connect_local(&self) -> io::Result<vise_protocol::connection::Pipe> {
        let (client, server) = vise_protocol::connection::Pipe::pair();
        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("vise-local-conn".into())
            .spawn(move || {
                if let Err(e) = conn::serve_local(server, shared)
                    && std::env::var_os("VISE_NET_DEBUG").is_some()
                {
                    log::warn!("[lan] local: {e}");
                }
            })?;
        Ok(client)
    }
    pub fn poll(&self) -> Vec<NetEvent> {
        self.events.try_iter().collect()
    }
    pub fn reserve(&self, uuid: u128) {
        self.shared.reserved.lock().unwrap().insert(uuid);
    }
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.stop_listening();
    }
}
impl Drop for Lan {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>, stop: &AtomicBool) {
    while !shared.stop.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let shared = shared.clone();
                let _ = std::thread::Builder::new()
                    .name("vise-lan-conn".into())
                    .spawn(move || handle(stream, addr, shared));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
fn handle(stream: TcpStream, addr: SocketAddr, shared: Arc<Shared>) {
    let _ = stream.set_nonblocking(false);
    if let Err(e) = conn::serve(stream, addr, shared)
        && std::env::var_os("VISE_NET_DEBUG").is_some()
    {
        log::warn!("[lan] {addr}: {e}");
    }
}

pub fn available_port() -> u16 {
    TcpListener::bind(("0.0.0.0", 0))
        .and_then(|l| l.local_addr())
        .map_or(25565, |a| a.port())
}
