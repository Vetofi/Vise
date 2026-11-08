use crate::auth::AuthSession;
use vise_protocol::{
    PROTOCOL_VERSION,
    connection::{ConnReader, ConnWriter, Connection, Counters},
    crypto::{generate_secret, offline_uuid, server_hash},
    packets::{Empty, configuration as cfg, handshake, login, play},
    registry::Dynamic,
    rsa::RsaPublicKey,
    servers::parse_address,
    text::{Language, plain, translatable},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
    time::Duration,
};

pub type Cookies = Arc<Mutex<HashMap<String, Vec<u8>>>>;

pub enum Event {
    Status(String),
    Brand(String),
    QuietCommands,
    Joined {
        registries: Arc<Dynamic>,
        uuid: u128,
        name: String,
    },
    Packet(Box<play::Clientbound>),
    DecodeError(String),
    Reconfigured(Arc<Dynamic>),
    PackPush(cfg::ResourcePackPush),
    PackPop(Option<u128>),
    ShowDialog(vise_world::nbt::Tag),
    ClearDialog,
    CodeOfConduct(String),
    Transfer {
        host: String,
        port: i32,
    },
    ServerLinks(Vec<cfg::ServerLink>),
    Disconnected {
        reason: String,
        from_server: bool,
    },
}

pub struct Session {
    events: Receiver<Event>,
    writer: Arc<Mutex<Option<ConnWriter>>>,
    configuring: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub address: String,
    pub name: String,
    pub counters: Arc<Mutex<Option<Arc<Counters>>>>,
    pub cookies: Cookies,
    debug: bool,
}
impl Session {
    pub fn connect(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        view_distance: i8,
        lang: Arc<Language>,
        resources: PathBuf,
    ) -> Self {
        Self::start(
            address,
            name,
            auth,
            view_distance,
            lang,
            None,
            None,
            resources,
        )
    }
    pub fn connect_transferred(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        view_distance: i8,
        lang: Arc<Language>,
        cookies: Cookies,
        resources: PathBuf,
    ) -> Self {
        Self::start(
            address,
            name,
            auth,
            view_distance,
            lang,
            None,
            Some(cookies),
            resources,
        )
    }
    pub fn connect_local(
        pipe: vise_protocol::connection::Pipe,
        name: &str,
        view_distance: i8,
        lang: Arc<Language>,
        resources: PathBuf,
    ) -> Self {
        Self::start(
            "local",
            name,
            None,
            view_distance,
            lang,
            Some(pipe),
            None,
            resources,
        )
    }
    fn start(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        view_distance: i8,
        lang: Arc<Language>,
        local: Option<vise_protocol::connection::Pipe>,
        cookies: Option<Cookies>,
        resources: PathBuf,
    ) -> Self {
        let (send, events) = channel();
        let writer = Arc::new(Mutex::new(None));
        let configuring = Arc::new(AtomicBool::new(true));
        let counters = Arc::new(Mutex::new(None));
        let transferred = cookies.is_some();
        let cookies = cookies.unwrap_or_default();
        let (thread_writer, thread_counters, thread_cookies) =
            (writer.clone(), counters.clone(), cookies.clone());
        let thread_configuring = configuring.clone();
        let (address_owned, name_owned) = (address.to_owned(), name.to_owned());
        let thread = std::thread::Builder::new()
            .name("vise-net".into())
            .spawn(move || {
                let result = run(
                    local,
                    &address_owned,
                    &name_owned,
                    auth.as_deref(),
                    view_distance,
                    &lang,
                    &send,
                    &thread_writer,
                    &thread_configuring,
                    &thread_counters,
                    &thread_cookies,
                    transferred,
                    &resources,
                );
                let from_server = result.is_ok();
                let reason = result.unwrap_or_else(|e| e);
                if let Some(w) = thread_writer.lock().unwrap().take() {
                    w.shutdown();
                }
                let _ = send.send(Event::Disconnected {
                    reason,
                    from_server,
                });
            })
            .ok();
        let debug = std::env::var_os("VISE_NET_DEBUG").is_some();
        Self {
            events,
            writer,
            configuring,
            thread,
            address: address.to_owned(),
            name: name.to_owned(),
            counters,
            cookies,
            debug,
        }
    }
    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
    pub fn send(&self, packet: play::Serverbound) {
        if self.debug
            && !matches!(
                packet,
                play::Serverbound::ClientTickEnd(_)
                    | play::Serverbound::MovePlayerPos(_)
                    | play::Serverbound::MovePlayerPosRot(_)
            )
        {
            eprintln!("[net] -> {packet:?}");
        }
        let mut writer = self.writer.lock().unwrap();
        if self.configuring.load(Ordering::Relaxed) {
            return;
        }
        if let Some(w) = writer.as_mut()
            && w.send(&packet.encode()).is_err()
        {
            w.shutdown();
        }
    }
    pub fn send_pack_status(&self, id: u128, action: i32) {
        let status = cfg::ResourcePackResponse { id, action };
        let mut writer = self.writer.lock().unwrap();
        let body = if self.configuring.load(Ordering::Relaxed) {
            cfg::Serverbound::ResourcePack(status).encode()
        } else {
            play::Serverbound::ResourcePack(status).encode()
        };
        if let Some(w) = writer.as_mut()
            && w.send(&body).is_err()
        {
            w.shutdown();
        }
    }
    pub fn send_custom_click(&self, id: &str, payload: Option<vise_world::nbt::Tag>) {
        let click = cfg::CustomClickAction {
            id: id.to_owned(),
            payload,
        };
        let mut writer = self.writer.lock().unwrap();
        let body = if self.configuring.load(Ordering::Relaxed) {
            cfg::Serverbound::CustomClickAction(click).encode()
        } else {
            play::Serverbound::CustomClickAction(click).encode()
        };
        if let Some(w) = writer.as_mut()
            && w.send(&body).is_err()
        {
            w.shutdown();
        }
    }
    pub fn accept_code_of_conduct(&self) {
        let mut writer = self.writer.lock().unwrap();
        if let Some(w) = writer.as_mut()
            && w.send(&cfg::Serverbound::AcceptCodeOfConduct(Empty).encode())
                .is_err()
        {
            w.shutdown();
        }
    }
    pub fn disconnect(&mut self) {
        if let Some(w) = self.writer.lock().unwrap().take() {
            w.shutdown();
        }
        drop(self.thread.take());
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.disconnect();
    }
}

pub const FRIEND: &str = vise_friends::ADDRESS_PREFIX;

pub fn shown_address(address: &str) -> String {
    match address.strip_prefix(FRIEND) {
        Some(code) => match vise_friends::invite::parse(code) {
            Some(vise_friends::invite::Parsed::Short(c)) => {
                format!("Friend's world ({}-****)", c.nameplate)
            }
            _ => "Friend's world".to_owned(),
        },
        None => address.to_owned(),
    }
}

pub fn friends_relay_setting() -> Option<String> {
    let dir = std::env::current_dir().ok()?;
    crate::ui_theme::preference(&dir, "friendsRelay")
        .filter(|r| !r.is_empty())
        .or_else(vise_friends::relay_url)
}
pub fn save_friends_relay(relay: &str) {
    if let Ok(dir) = std::env::current_dir() {
        crate::ui_theme::set_preference(&dir, "friendsRelay", (!relay.is_empty()).then_some(relay));
    }
}

fn join_friend(code: &str, send: &Sender<Event>) -> Result<(String, u16), String> {
    use vise_friends::{
        PathMode,
        guest::{self, GuestConfig},
    };
    status(send, "Finding your friend's world...");
    let tunnel = guest::join(&GuestConfig {
        code: code.to_owned(),
        relay: friends_relay_setting(),
        protocol: PROTOCOL_VERSION,
        game: vise_protocol::GAME_VERSION.to_owned(),
        path: PathMode::from_env(),
    })
    .map_err(|e| e.to_string())?;
    status(
        send,
        if tunnel.direct {
            "Connected straight to your friend..."
        } else {
            "Connected to your friend through the relay..."
        },
    );
    let (port, _) = tunnel.serve_loopback().map_err(|e| e.to_string())?;
    Ok(("127.0.0.1".to_owned(), port))
}

fn status(send: &Sender<Event>, text: &str) {
    let _ = send.send(Event::Status(text.to_owned()));
}

pub const NOT_SIGNED_IN: &str = "This server is in online mode. Sign in with your Microsoft account in the Vise launcher to join it.";

fn tr(lang: &Language, key: &str) -> String {
    lang.get(key).cloned().unwrap_or_else(|| key.to_owned())
}

const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn connect_tcp(
    host: &str,
    port: u16,
    relayed: bool,
    read_timeout: Duration,
) -> Result<Connection, String> {
    let c = if relayed {
        Connection::connect(host, port, Duration::from_secs(10))
    } else {
        Connection::connect_server(host, port, Duration::from_secs(10))
    }
    .map_err(|e| e.to_string())?;
    c.reader
        .set_read_timeout(Some(read_timeout))
        .map_err(|e| e.to_string())?;
    Ok(c)
}

fn connection_lost(e: vise_protocol::Error, lang: &Language) -> String {
    if e.is_timed_out() {
        tr(lang, "disconnect.timeout")
    } else if e.is_end_of_stream() {
        tr(lang, "disconnect.endOfStream")
    } else {
        plain(
            &translatable(
                "disconnect.genericReason",
                &[&format!("Internal Exception: {e}")],
            ),
            lang,
        )
    }
}

fn run(
    local: Option<vise_protocol::connection::Pipe>,
    address: &str,
    name: &str,
    auth: Option<&AuthSession>,
    view_distance: i8,
    lang: &Language,
    send: &Sender<Event>,
    shared_writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    shared_counters: &Mutex<Option<Arc<Counters>>>,
    cookies: &Cookies,
    transferred: bool,
    resources: &Path,
) -> Result<String, String> {
    let (host, port, mut c) = if let Some(pipe) = local {
        (address.to_owned(), 0, Connection::local(pipe))
    } else {
        let (host, port) = match address.strip_prefix(FRIEND) {
            Some(code) => join_friend(code, send)?,
            None => {
                let Some(target) = parse_address(address) else {
                    return Ok(format!("Invalid server address: {address}"));
                };
                status(send, "Connecting to the server...");
                target
            }
        };
        let c = connect_tcp(&host, port, address.starts_with(FRIEND), READ_TIMEOUT)?;
        (host, port, c)
    };
    *shared_counters.lock().unwrap() = Some(c.reader.counters.clone());
    let e = |e: vise_protocol::Error| connection_lost(e, lang);
    c.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: PROTOCOL_VERSION,
            host: host.clone(),
            port,
            intent: if transferred {
                handshake::TRANSFER
            } else {
                handshake::LOGIN
            },
        })
        .encode(),
    )
    .map_err(e)?;
    let uuid = auth.map_or_else(|| offline_uuid(name), |a| a.uuid);
    c.send(
        &login::Serverbound::Hello(login::ServerHello {
            name: name.to_owned(),
            uuid,
        })
        .encode(),
    )
    .map_err(e)?;
    let profile = loop {
        let (id, payload) = c.recv().map_err(e)?;
        match login::Clientbound::decode(id, &payload).map_err(e)? {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::LoginFinished(p) => {
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .map_err(e)?;
                break p.profile;
            }
            login::Clientbound::Disconnect(p) => {
                let reason = serde_json::from_str::<serde_json::Value>(&p.reason_json)
                    .map(|v| json_text(&v, lang))
                    .unwrap_or(p.reason_json);
                return Ok(reason);
            }
            login::Clientbound::Hello(hello) => {
                status(send, &tr(lang, "connect.authorizing"));
                let key = RsaPublicKey::from_der(&hello.public_key)
                    .map_err(|err| format!("Protocol error: {err}"))?;
                let secret = generate_secret().map_err(e)?;
                if hello.should_authenticate {
                    let account = auth.ok_or_else(|| NOT_SIGNED_IN.to_owned())?;
                    let hash = server_hash(&hello.server_id, &secret, &hello.public_key);
                    if let Err(err) = crate::auth::join_server(account, &hash) {
                        let reason = match err.reason() {
                            Ok(key) => tr(lang, key),
                            Err(message) => message.to_owned(),
                        };
                        return Err(plain(
                            &translatable("disconnect.loginFailedInfo", &[&reason]),
                            lang,
                        ));
                    }
                }
                status(send, &tr(lang, "connect.encrypting"));
                c.send(
                    &login::Serverbound::Key(login::Key {
                        key: key.encrypt(&secret).map_err(e)?,
                        challenge: key.encrypt(&hello.challenge).map_err(e)?,
                    })
                    .encode(),
                )
                .map_err(e)?;
                c.enable_encryption(&secret);
            }
            login::Clientbound::CustomQuery(q) => {
                c.send(
                    &login::Serverbound::CustomQueryAnswer(login::CustomQueryAnswer {
                        transaction: q.transaction,
                        payload: None,
                    })
                    .encode(),
                )
                .map_err(e)?;
            }
            login::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                c.send(
                    &login::Serverbound::CookieResponse(login::CookieResponse {
                        key: q.key,
                        payload,
                    })
                    .encode(),
                )
                .map_err(e)?;
            }
            login::Clientbound::Unknown(..) => {}
        }
    };
    status(send, "Joining world...");
    c.send(
        &cfg::Serverbound::ClientInformation(cfg::ClientInformation::new(view_distance)).encode(),
    )
    .map_err(e)?;
    c.send(
        &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: brand(),
        })
        .encode(),
    )
    .map_err(e)?;
    let (mut reader, writer) = c.split();
    *shared_writer.lock().unwrap() = Some(writer);
    let registries = configure(
        &mut reader,
        shared_writer,
        configuring,
        lang,
        send,
        cookies,
        resources,
    )?;
    let _ = send.send(Event::Joined {
        registries: Arc::new(registries),
        uuid: profile.uuid,
        name: profile.name,
    });
    receive_play(
        &mut reader,
        shared_writer,
        configuring,
        lang,
        send,
        cookies,
        resources,
    )
}

fn receive_play(
    reader: &mut ConnReader,
    shared_writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    lang: &Language,
    send: &Sender<Event>,
    cookies: &Cookies,
    resources: &Path,
) -> Result<String, String> {
    loop {
        let (id, payload) = reader.recv().map_err(|e| connection_lost(e, lang))?;
        let packet = match play::Clientbound::decode(id, &payload) {
            Ok(p) => p,
            Err(err) => {
                let name = vise_protocol::ids::name(
                    vise_protocol::State::Play,
                    vise_protocol::Direction::Clientbound,
                    id,
                )
                .unwrap_or("?");
                let _ = send.send(Event::DecodeError(format!("{name}: {err}")));
                continue;
            }
        };
        let reply = |p: play::Serverbound| {
            if let Some(w) = shared_writer.lock().unwrap().as_mut() {
                let _ = w.send(&p.encode());
            }
        };
        match packet {
            play::Clientbound::KeepAlive(k) => reply(play::Serverbound::KeepAlive(k)),
            play::Clientbound::BundleDelimiter(_) => {}
            play::Clientbound::ResourcePackPush(p) => {
                let _ = send.send(Event::PackPush(p));
            }
            play::Clientbound::ResourcePackPop(p) => {
                let _ = send.send(Event::PackPop(p.id));
            }
            play::Clientbound::StartConfiguration(_) => {
                if let Some(w) = shared_writer.lock().unwrap().as_mut() {
                    let _ = w.send(&play::Serverbound::ConfigurationAcknowledged(Empty).encode());
                    configuring.store(true, Ordering::Relaxed);
                }
                let registries = configure(
                    reader,
                    shared_writer,
                    configuring,
                    lang,
                    send,
                    cookies,
                    resources,
                )?;
                let _ = send.send(Event::Reconfigured(Arc::new(registries)));
            }
            play::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                reply(play::Serverbound::CookieResponse(cfg::CookieResponse {
                    key: q.key,
                    payload,
                }));
            }
            play::Clientbound::StoreCookie(p) => {
                cookies.lock().unwrap().insert(p.key, p.payload);
            }
            play::Clientbound::ServerLinks(p) => {
                let _ = send.send(Event::ServerLinks(p.links));
            }
            play::Clientbound::Transfer(p) => {
                let _ = send.send(Event::Transfer {
                    host: p.host,
                    port: p.port,
                });
                return Ok(tr(lang, "disconnect.transfer"));
            }
            play::Clientbound::Disconnect(d) => return Ok(plain(&d.reason, lang)),
            play::Clientbound::CustomPayload(p) if p.channel == "minecraft:brand" => {
                receive_brand(&p, send);
            }
            other => {
                if send.send(Event::Packet(Box::new(other))).is_err() {
                    return Ok("Disconnected".into());
                }
            }
        }
    }
}

fn configure(
    reader: &mut ConnReader,
    writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    lang: &Language,
    send: &Sender<Event>,
    cookies: &Cookies,
    resources: &Path,
) -> Result<Dynamic, String> {
    let mut registries = Dynamic::default();
    let reply = |p: cfg::Serverbound| -> Result<(), String> {
        match writer.lock().unwrap().as_mut() {
            Some(w) => w.send(&p.encode()).map_err(|e| connection_lost(e, lang)),
            None => Err("Disconnected".into()),
        }
    };
    loop {
        let (id, payload) = reader.recv().map_err(|e| connection_lost(e, lang))?;
        match cfg::Clientbound::decode(id, &payload).map_err(|e| connection_lost(e, lang))? {
            cfg::Clientbound::KeepAlive(k) => reply(cfg::Serverbound::KeepAlive(k))?,
            cfg::Clientbound::CustomPayload(p) if p.channel == crate::content_identity::CHANNEL => {
                let hash = crate::content_identity::identity(resources)
                    .map_err(|_| crate::content_identity::MISMATCH.to_string())?;
                reply(cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                    channel: crate::content_identity::CHANNEL.into(),
                    data: hash.to_vec(),
                }))?;
            }
            cfg::Clientbound::CustomPayload(p) => receive_brand(&p, send),
            cfg::Clientbound::Ping(p) => reply(cfg::Serverbound::Pong(p))?,
            cfg::Clientbound::SelectKnownPacks(_) => {
                reply(cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks {
                    packs: vec![],
                }))?
            }
            cfg::Clientbound::RegistryData(r) => registries.add(r),
            cfg::Clientbound::UpdateTags(t) => registries.add_tags(&t),
            cfg::Clientbound::CodeOfConduct(c) => {
                let _ = send.send(Event::CodeOfConduct(c.text));
            }
            cfg::Clientbound::ResourcePackPush(p) => {
                let _ = send.send(Event::PackPush(p));
            }
            cfg::Clientbound::ResourcePackPop(p) => {
                let _ = send.send(Event::PackPop(p.id));
            }
            cfg::Clientbound::ShowDialog(d) => {
                let _ = send.send(Event::ShowDialog(d.dialog));
            }
            cfg::Clientbound::ClearDialog(_) => {
                let _ = send.send(Event::ClearDialog);
            }
            cfg::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                reply(cfg::Serverbound::CookieResponse(cfg::CookieResponse {
                    key: q.key,
                    payload,
                }))?
            }
            cfg::Clientbound::StoreCookie(p) => {
                cookies.lock().unwrap().insert(p.key, p.payload);
            }
            cfg::Clientbound::ServerLinks(p) => {
                let _ = send.send(Event::ServerLinks(p.links));
            }
            cfg::Clientbound::Transfer(p) => {
                let _ = send.send(Event::Transfer {
                    host: p.host,
                    port: p.port,
                });
                return Err(tr(lang, "disconnect.transfer"));
            }
            cfg::Clientbound::Disconnect(d) => return Err(plain(&d.reason, lang)),
            cfg::Clientbound::FinishConfiguration(_) => {
                let mut w = writer.lock().unwrap();
                let w = w.as_mut().ok_or("Disconnected")?;
                w.send(&cfg::Serverbound::FinishConfiguration(Empty).encode())
                    .map_err(|e| connection_lost(e, lang))?;
                configuring.store(false, Ordering::Relaxed);
                return Ok(registries);
            }
            _ => {}
        }
    }
}

fn receive_brand(payload: &cfg::CustomPayload, send: &Sender<Event>) {
    if payload.channel == "minecraft:brand"
        && let Ok(brand) = vise_protocol::buf::Reader::new(&payload.data).string(32767)
    {
        let _ = send.send(Event::Brand(brand));
    } else if payload.channel == vise_protocol::quiet::CHANNEL {
        let _ = send.send(Event::QuietCommands);
    }
}

fn brand() -> Vec<u8> {
    let mut w = vise_protocol::buf::Writer::new();
    w.string("vise");
    w.data
}

pub fn json_text(v: &serde_json::Value, lang: &Language) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(|e| json_text(e, lang)).collect(),
        serde_json::Value::Object(o) => {
            let mut out = if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                t.to_owned()
            } else if let Some(key) = o.get("translate").and_then(|t| t.as_str()) {
                let args: Vec<String> = o
                    .get("with")
                    .and_then(|w| w.as_array())
                    .map(|a| a.iter().map(|e| json_text(e, lang)).collect())
                    .unwrap_or_default();
                let pattern = lang
                    .get(key)
                    .cloned()
                    .or_else(|| {
                        o.get("fallback")
                            .and_then(|f| f.as_str())
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| key.to_owned());
                vise_protocol::text::format_java(&pattern, &args)
            } else {
                String::new()
            };
            if let Some(extra) = o.get("extra").and_then(|e| e.as_array()) {
                for e in extra {
                    out.push_str(&json_text(e, lang));
                }
            }
            out
        }
        other => other.to_string(),
    }
}

pub fn load_language(resources: &std::path::Path) -> Language {
    if let Some(lang) = crate::lang::current() {
        return lang.map().clone();
    }
    std::fs::read(resources.join("assets/minecraft/lang/en_us.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn ping(address: &str, timeout: Duration) -> Result<(serde_json::Value, u128), String> {
    use vise_protocol::packets::status;
    let (host, port) = parse_address(address).ok_or("Invalid address")?;
    let mut c = Connection::connect_server(&host, port, timeout).map_err(|e| e.to_string())?;
    c.reader
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let e = |e: vise_protocol::Error| e.to_string();
    c.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: PROTOCOL_VERSION,
            host: host.clone(),
            port,
            intent: handshake::STATUS,
        })
        .encode(),
    )
    .map_err(e)?;
    c.send(&status::Serverbound::StatusRequest(Empty).encode())
        .map_err(e)?;
    let (id, payload) = c.recv().map_err(e)?;
    let json = match status::Clientbound::decode(id, &payload).map_err(e)? {
        status::Clientbound::StatusResponse(s) => {
            serde_json::from_str(&s.json).map_err(|e| e.to_string())?
        }
        _ => return Err("unexpected status reply".into()),
    };
    let start = std::time::Instant::now();
    c.send(&status::Serverbound::PingRequest(status::Ping { time: 1 }).encode())
        .map_err(e)?;
    let _ = c.recv().map_err(e)?;
    Ok((json, start.elapsed().as_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disconnect_language() -> Language {
        [
            ("disconnect.timeout", "Timed out"),
            ("disconnect.endOfStream", "End of stream"),
            ("disconnect.genericReason", "%s"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    }

    #[test]
    fn a_server_that_stops_answering_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::thread::spawn(move || listener.accept().map(|(stream, _)| stream));
        let c = connect_tcp("127.0.0.1", port, true, Duration::from_millis(300)).unwrap();
        let _server = held.join().unwrap().unwrap();
        let (mut reader, writer) = c.split();
        let (done, result) = channel();
        std::thread::spawn(move || {
            let (send, _events) = channel();
            let _ = done.send(receive_play(
                &mut reader,
                &Mutex::new(Some(writer)),
                &AtomicBool::new(false),
                &disconnect_language(),
                &send,
                &Cookies::default(),
                Path::new("."),
            ));
        });
        let ended = result
            .recv_timeout(Duration::from_secs(10))
            .expect("the connection never timed out");
        assert_eq!(ended, Err("Timed out".to_owned()));
    }

    #[test]
    fn lost_connections_read_like_vanillas_disconnect_reasons() {
        let lang = disconnect_language();
        let lost = |e| connection_lost(e, &lang);
        assert_eq!(lost(vise_protocol::Error::timed_out()), "Timed out");
        assert_eq!(
            lost(vise_protocol::Error::end_of_stream()),
            "End of stream"
        );
        assert_eq!(
            lost(vise_protocol::Error::new("Bad VarInt")),
            "Internal Exception: Bad VarInt"
        );
    }

    #[test]
    fn configuration_answers_the_content_identity_challenge() {
        let root =
            std::env::temp_dir().join(format!("vise-content-identity-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("facts.json"),
            br#"{"states":[[0,0,0,0,"minecraft:stone"]]}"#,
        )
        .unwrap();
        let expected = crate::content_identity::identity(&root).unwrap();
        let (client, server) = vise_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Mutex::new(Some(writer));
        let thread = std::thread::spawn(move || {
            let (send, _events) = channel();
            configure(
                &mut reader,
                &writer,
                &AtomicBool::new(true),
                &Language::new(),
                &send,
                &Cookies::default(),
                &root,
            )
        });
        let mut server = Connection::local(server);
        server
            .send(
                &cfg::Clientbound::CustomPayload(cfg::CustomPayload {
                    channel: crate::content_identity::CHANNEL.into(),
                    data: vec![],
                })
                .encode(),
            )
            .unwrap();
        server
            .send(&cfg::Clientbound::Ping(cfg::Ping { id: 17 }).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, &body).unwrap(),
            cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                channel: crate::content_identity::CHANNEL.into(),
                data: expected.to_vec(),
            })
        );
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, &body).unwrap(),
            cfg::Serverbound::Pong(cfg::Ping { id: 17 })
        );
        server
            .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, &body).unwrap(),
            cfg::Serverbound::FinishConfiguration(Empty)
        );
        assert!(thread.join().unwrap().is_ok());
        std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("vise-content-identity-{}", std::process::id())),
        )
        .unwrap();
    }

    #[test]
    fn disconnect_and_drop_do_not_wait_for_a_stalled_reader() {
        for explicit_disconnect in [true, false] {
            let (client, _server) = vise_protocol::connection::Pipe::pair();
            let (_reader, writer) = Connection::local(client).split();
            let writer = Arc::new(Mutex::new(Some(writer)));
            let (_send, events) = channel();
            let (release, stalled) = channel();
            let (finished, completed) = channel();
            let thread = std::thread::spawn(move || {
                let _ = stalled.recv_timeout(std::time::Duration::from_secs(3));
                let _ = finished.send(());
            });
            let mut session = Session {
                events,
                writer: writer.clone(),
                configuring: Arc::new(AtomicBool::new(false)),
                thread: Some(thread),
                address: "local".into(),
                name: "TestPlayer".into(),
                counters: Arc::new(Mutex::new(None)),
                cookies: Cookies::default(),
                debug: false,
            };
            let started = std::time::Instant::now();
            if explicit_disconnect {
                session.disconnect();
                session.disconnect();
            }
            drop(session);
            let elapsed = started.elapsed();
            let reader_still_pending = completed.try_recv().is_err();
            let _ = release.send(());
            if reader_still_pending {
                completed
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
            }
            assert!(
                writer.lock().unwrap().is_none(),
                "connection was not closed"
            );
            assert!(
                reader_still_pending,
                "disconnect waited for the stalled reader"
            );
            assert!(
                elapsed < std::time::Duration::from_secs(1),
                "disconnect took {elapsed:?}"
            );
        }
    }

    #[test]
    fn pings_wait_behind_teleports_but_keepalives_do_not_wait_for_the_game() {
        let (client, server) = vise_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Arc::new(Mutex::new(Some(writer)));
        let (send, events) = channel();
        let thread_writer = writer.clone();
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &AtomicBool::new(false),
                &Language::new(),
                &send,
                &Cookies::default(),
                Path::new("."),
            )
        });
        let mut server = Connection::local(server);
        let teleport = play::Clientbound::PlayerPosition(play::PlayerPosition {
            teleport: 42,
            position: [10.0, 64.0, 10.0],
            delta: [0.0; 3],
            yaw: 90.0,
            pitch: 0.0,
            relatives: 0,
        });
        server.send(&teleport.encode()).unwrap();
        server
            .send(&play::Clientbound::Ping(cfg::Ping { id: 7 }).encode())
            .unwrap();
        server
            .send(&play::Clientbound::KeepAlive(cfg::KeepAlive { id: 9 }).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            play::Serverbound::decode(id, &body).unwrap(),
            play::Serverbound::KeepAlive(cfg::KeepAlive { id: 9 })
        );
        let timeout = Duration::from_secs(5);
        assert!(
            matches!(events.recv_timeout(timeout).unwrap(), Event::Packet(p) if *p == teleport)
        );
        assert!(
            matches!(events.recv_timeout(timeout).unwrap(), Event::Packet(p) if *p == play::Clientbound::Ping(cfg::Ping { id: 7 }))
        );
        writer.lock().unwrap().take().unwrap().shutdown();
        assert!(thread.join().unwrap().is_err());
    }

    #[test]
    fn play_packets_wait_out_a_reconfiguration() {
        let (client, server) = vise_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let (send, events) = channel();
        let session = Session {
            events,
            writer: Arc::new(Mutex::new(Some(writer))),
            configuring: Arc::new(AtomicBool::new(false)),
            thread: None,
            address: "local".into(),
            name: "Player".into(),
            counters: Arc::new(Mutex::new(None)),
            cookies: Cookies::default(),
            debug: false,
        };
        let (thread_writer, thread_configuring) =
            (session.writer.clone(), session.configuring.clone());
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &thread_configuring,
                &Language::new(),
                &send,
                &Cookies::default(),
                Path::new("."),
            )
        });
        let mut server = Connection::local(server);
        let tick_end = play::Serverbound::ClientTickEnd(Empty);
        server
            .send(&play::Clientbound::StartConfiguration(Empty).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            play::Serverbound::decode(id, &body).unwrap(),
            play::Serverbound::ConfigurationAcknowledged(Empty)
        );
        session.send(tick_end.clone());
        server
            .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, &body).unwrap(),
            cfg::Serverbound::FinishConfiguration(Empty)
        );
        session.send(tick_end.clone());
        let (id, body) = server.recv().unwrap();
        assert_eq!(play::Serverbound::decode(id, &body).unwrap(), tick_end);
        drop(session);
        assert!(thread.join().unwrap().is_err());
    }

    #[test]
    fn play_answers_cookie_requests_from_stored_cookies_and_follows_transfers() {
        let (client, server) = vise_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Arc::new(Mutex::new(Some(writer)));
        let (send, events) = channel();
        let cookies = Cookies::default();
        let (thread_writer, thread_cookies) = (writer.clone(), cookies.clone());
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &AtomicBool::new(false),
                &Language::new(),
                &send,
                &thread_cookies,
                Path::new("."),
            )
        });
        let mut server = Connection::local(server);
        let ask = |server: &mut Connection, key: &str| {
            let request = play::Clientbound::CookieRequest(cfg::CookieRequest { key: key.into() });
            server.send(&request.encode()).unwrap();
            let (id, body) = server.recv().unwrap();
            play::Serverbound::decode(id, &body).unwrap()
        };
        assert_eq!(
            ask(&mut server, "proxy:session"),
            play::Serverbound::CookieResponse(cfg::CookieResponse {
                key: "proxy:session".into(),
                payload: None,
            })
        );
        let store = play::Clientbound::StoreCookie(cfg::StoreCookie {
            key: "proxy:session".into(),
            payload: vec![1, 2, 3],
        });
        server.send(&store.encode()).unwrap();
        assert_eq!(
            ask(&mut server, "proxy:session"),
            play::Serverbound::CookieResponse(cfg::CookieResponse {
                key: "proxy:session".into(),
                payload: Some(vec![1, 2, 3]),
            })
        );
        let transfer = play::Clientbound::Transfer(cfg::Transfer {
            host: "example.net".into(),
            port: 25566,
        });
        server.send(&transfer.encode()).unwrap();
        assert!(thread.join().unwrap().is_ok());
        assert!(matches!(
            events.try_iter().last(),
            Some(Event::Transfer { host, port: 25566 }) if host == "example.net"
        ));
        assert_eq!(
            cookies.lock().unwrap().get("proxy:session"),
            Some(&vec![1, 2, 3])
        );
    }

    #[test]
    fn server_brand_payload_is_decoded_and_invalid_or_unrelated_payloads_are_ignored() {
        let (send, events) = channel();
        let mut writer = vise_protocol::buf::Writer::new();
        writer.string("vanilla");
        let mut payload = cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: writer.data,
        };
        receive_brand(&payload, &send);
        assert!(matches!(events.try_recv(), Ok(Event::Brand(brand)) if brand == "vanilla"));
        payload.channel = "example:other".into();
        receive_brand(&payload, &send);
        assert!(events.try_recv().is_err());
        payload.channel = "minecraft:brand".into();
        payload.data = vec![0xff];
        receive_brand(&payload, &send);
        assert!(events.try_recv().is_err());
    }
}
