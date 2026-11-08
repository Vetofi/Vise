
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signature {
    Unsigned,
    Signed,
    Invalid,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Textures {
    pub skin: Option<String>,
    pub slim: bool,
    pub cape: Option<String>,
    pub elytra: Option<String>,
    pub signed: bool,
}

pub const TEXTURE_URIS: [&str; 2] = [
    "https://textures.minecraft.net/texture/{textureId}",
    "http://textures.minecraft.net/texture/{textureId}",
];

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut shift) = (0u32, 18i32);
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        let b = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            '=' => {
                if shift == 6 && (i == bytes.len() || bytes[i] != '=') || shift == 18 {
                    return None;
                }
                if shift == 6 {
                    i += 1;
                }
                break;
            }
            _ => return None,
        };
        bits |= b << shift;
        shift -= 6;
        if shift < 0 {
            out.extend([(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            shift = 18;
            bits = 0;
        }
    }
    match shift {
        6 => out.push((bits >> 16) as u8),
        0 => out.extend([(bits >> 16) as u8, (bits >> 8) as u8]),
        12 => return None,
        _ => {}
    }
    (i >= bytes.len()).then_some(out)
}

pub fn unpack(value: &str, signature: Signature, valid_uris: &[&str]) -> Textures {
    let empty = Textures::default();
    let Some(json) = base64_decode(value)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return empty;
    };
    let Some(textures) = json.get("textures").and_then(|t| t.as_object()) else {
        return empty;
    };
    if textures.is_empty() {
        return empty;
    }
    let mut out = Textures {
        signed: signature == Signature::Signed,
        ..empty.clone()
    };
    for (kind, texture) in textures {
        let Some(url) = texture.get("url").and_then(|u| u.as_str()) else {
            return empty;
        };
        let allowed = valid_uris
            .iter()
            .any(|u| url.starts_with(&u.replace("{textureId}", "")));
        if !allowed {
            return empty;
        }
        if kind == "SKIN" {
            out.slim = texture
                .get("metadata")
                .and_then(|m| m.get("model"))
                .and_then(|m| m.as_str())
                == Some("slim");
        }
        let slot = match kind.as_str() {
            "SKIN" => &mut out.skin,
            "CAPE" => &mut out.cape,
            "ELYTRA" => &mut out.elytra,
            _ => continue,
        };
        *slot = Some(url.to_owned());
    }
    out
}

pub fn profile_skin_url(profile: &serde_json::Value) -> Option<String> {
    let (value, signature) = parse_profile(profile).textures?;
    unpack(&value, signature, &TEXTURE_URIS).skin
}
pub fn profile_skin_url_nbt(profile: &vise_world::nbt::Tag) -> Option<String> {
    let properties = match profile.get("properties")? {
        vise_world::nbt::Tag::List(list) => list,
        _ => return None,
    };
    let value = properties.iter().find_map(|p| {
        let name = nbt_text(p.get("name"))?;
        (name == "textures")
            .then(|| nbt_text(p.get("value")))
            .flatten()
    })?;
    unpack(&value, Signature::Unsigned, &TEXTURE_URIS).skin
}

pub fn profile_uuid_nbt(profile: &vise_world::nbt::Tag) -> Option<u128> {
    match profile.get("id")? {
        vise_world::nbt::Tag::IntArray(v) if v.len() == 4 => Some(
            v.iter()
                .fold(0u128, |a, &w| (a << 32) | u128::from(w as u32)),
        ),
        vise_world::nbt::Tag::String(s) => {
            let digits: String = s.to_string_lossy().chars().filter(|c| *c != '-').collect();
            u128::from_str_radix(&digits, 16).ok()
        }
        _ => None,
    }
}

pub(crate) fn nbt_text(tag: Option<&vise_world::nbt::Tag>) -> Option<String> {
    match tag {
        Some(vise_world::nbt::Tag::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    }
}

pub use crate::chunk::block_entity::NameOrId;

#[derive(Clone, Debug)]
pub struct ResolvedProfile {
    pub id: u128,
    pub name: String,
    pub properties: Vec<(String, String, Option<String>)>,
}
impl ResolvedProfile {
    pub fn textures(&self) -> Option<(String, Signature)> {
        self.properties
            .iter()
            .find(|(n, _, _)| n == "textures")
            .map(|(_, v, s)| {
                (
                    v.clone(),
                    if s.is_some() {
                        Signature::Signed
                    } else {
                        Signature::Unsigned
                    },
                )
            })
    }
}

#[derive(Clone, Debug)]
pub enum Resolve {
    Pending,
    Done(Option<ResolvedProfile>),
}

pub struct Resolver {
    entries: std::collections::HashMap<NameOrId, Entry>,
    done: std::sync::mpsc::Receiver<(NameOrId, Option<ResolvedProfile>)>,
    send: std::sync::mpsc::Sender<(NameOrId, Option<ResolvedProfile>)>,
    last_sweep: std::time::Instant,
}
struct Entry {
    touched: std::time::Instant,
    resolved: Option<Option<ResolvedProfile>>,
}
const MAX_PROFILES: usize = 256;
const PROFILE_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

fn valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| c > 32 && c < 127)
}

fn resolve_profile(key: &NameOrId, doc: &serde_json::Value) -> Option<ResolvedProfile> {
    let id = match key {
        NameOrId::Id(id) => *id,
        NameOrId::Name(name) => {
            let url = crate::auth::endpoint(doc, "profiles", "getByName")?
                .replace("{name}", &name.to_lowercase());
            let body = http_get(&url)?;
            let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
            crate::auth::parse_uuid(v.get("id")?.as_str()?)?
        }
    };
    let url = crate::auth::endpoint(doc, "session", "getProfileById")?
        .replace("{profileId}", &format!("{id:032x}"));
    let separator = if url.contains('?') { '&' } else { '?' };
    let body = http_get(&format!("{url}{separator}unsigned=false"))?;
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    Some(ResolvedProfile {
        id: crate::auth::parse_uuid(v.get("id")?.as_str()?)?,
        name: v.get("name")?.as_str()?.to_owned(),
        properties: v
            .get("properties")?
            .as_array()?
            .iter()
            .map(|p| {
                Some((
                    p.get("name")?.as_str()?.to_owned(),
                    p.get("value")?.as_str()?.to_owned(),
                    p.get("signature")
                        .and_then(|s| s.as_str())
                        .map(str::to_owned),
                ))
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}
impl Resolver {
    pub fn new() -> Self {
        let (send, done) = std::sync::mpsc::channel();
        Self {
            entries: Default::default(),
            done,
            send,
            last_sweep: std::time::Instant::now(),
        }
    }
    fn sweep(&mut self) {
        if self.last_sweep.elapsed() < std::time::Duration::from_secs(30) {
            return;
        }
        self.last_sweep = std::time::Instant::now();
        self.entries
            .retain(|_, e| e.touched.elapsed() < PROFILE_TTL);
        while self.entries.len() > MAX_PROFILES
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
    }
    pub fn state(&mut self, key: &NameOrId) -> Resolve {
        while let Ok((key, resolved)) = self.done.try_recv() {
            self.entries.insert(
                key,
                Entry {
                    touched: std::time::Instant::now(),
                    resolved: Some(resolved),
                },
            );
        }
        self.sweep();
        if let Some(entry) = self.entries.get_mut(key) {
            entry.touched = std::time::Instant::now();
            return match &entry.resolved {
                Some(resolved) => Resolve::Done(resolved.clone()),
                None => Resolve::Pending,
            };
        }
        let resolved = matches!(key, NameOrId::Name(name) if !valid_player_name(name));
        self.entries.insert(
            key.clone(),
            Entry {
                touched: std::time::Instant::now(),
                resolved: resolved.then_some(None),
            },
        );
        if resolved {
            return Resolve::Done(None);
        }
        let (key, send) = (key.clone(), self.send.clone());
        std::thread::spawn(move || {
            let resolved = crate::auth::discovery()
                .ok()
                .and_then(|doc| resolve_profile(&key, &doc));
            let _ = send.send((key, resolved));
        });
        Resolve::Pending
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileInfo {
    pub id: u128,
    pub textures: Option<(String, Signature)>,
    pub dynamic: Option<NameOrId>,
}

fn profile_id(v: &serde_json::Value) -> Option<u128> {
    match v {
        serde_json::Value::Array(a) => {
            let ints: Option<Vec<i64>> = a.iter().map(|x| x.as_i64()).collect();
            let i = ints?;
            (i.len() == 4).then(|| {
                ((i[0] as u32 as u128) << 96)
                    | ((i[1] as u32 as u128) << 64)
                    | ((i[2] as u32 as u128) << 32)
                    | i[3] as u32 as u128
            })
        }
        serde_json::Value::String(s) => crate::auth::parse_uuid(s),
        _ => None,
    }
}

pub fn parse_profile(v: &serde_json::Value) -> ProfileInfo {
    if let serde_json::Value::String(name) = v {
        return ProfileInfo {
            id: vise_protocol::crypto::offline_uuid(name),
            textures: None,
            dynamic: valid_player_name(name).then(|| NameOrId::Name(name.clone())),
        };
    }
    let name = v.get("name").and_then(|n| n.as_str()).map(str::to_owned);
    let id = v.get("id").and_then(profile_id);
    let properties = v.get("properties").and_then(|p| p.as_array());
    let textures = properties
        .into_iter()
        .flatten()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        .and_then(|p| {
            Some((
                p.get("value")?.as_str()?.to_owned(),
                if p.get("signature").is_some() {
                    Signature::Signed
                } else {
                    Signature::Unsigned
                },
            ))
        });
    let dynamic =
        (properties.is_none_or(|p| p.is_empty()) && name.is_some() != id.is_some()).then(|| {
            name.as_deref().map_or_else(
                || NameOrId::Id(id.unwrap()),
                |n| NameOrId::Name(n.to_owned()),
            )
        });
    ProfileInfo {
        id: id
            .or_else(|| name.as_deref().map(vise_protocol::crypto::offline_uuid))
            .unwrap_or(0),
        textures,
        dynamic,
    }
}

pub fn dynamic_nbt(profile: &vise_world::nbt::Tag) -> Option<NameOrId> {
    if let vise_world::nbt::Tag::String(s) = profile {
        let name = s.to_string_lossy();
        return valid_player_name(&name).then_some(NameOrId::Name(name));
    }
    if matches!(profile.get("properties"), Some(vise_world::nbt::Tag::List(l)) if !l.is_empty())
    {
        return None;
    }
    match (nbt_text(profile.get("name")), profile_uuid_nbt(profile)) {
        (Some(name), None) => Some(NameOrId::Name(name)),
        (None, Some(id)) => Some(NameOrId::Id(id)),
        _ => None,
    }
}

pub fn head_skin(
    profile: &serde_json::Value,
    resolver: &mut Resolver,
    downloads: &mut Downloader,
    atlas: impl Fn(&str) -> bool,
) -> String {
    let p = parse_profile(profile);
    let property = p.textures.or_else(|| {
        let key = p.dynamic.as_ref()?;
        match resolver.state(key) {
            Resolve::Done(Some(profile)) => profile.textures(),
            _ => None,
        }
    });
    if let Some((value, signature)) = property
        && let Some(url) = unpack(&value, signature, &TEXTURE_URIS).skin
        && let Some(hash) = texture_hash(&url)
    {
        let location = skin_location(&hash);
        match downloads.state(Kind::Skin, &url) {
            Download::Ready if atlas(&location) => {
                downloads.drawn(&location);
                return location;
            }
            Download::Ready => {
                downloads.again(&location);
            }
            _ => {}
        }
    }
    crate::tab_list::default_skin(p.id).to_owned()
}

pub fn texture_hash(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path = &rest[rest.find('/').unwrap_or(rest.len())..];
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    let name = &path[path.rfind(['/', '\\']).map_or(0, |i| i + 1)..];
    Some(name[..name.rfind('.').unwrap_or(name.len())].to_owned())
}

pub fn skin_location(hash: &str) -> String {
    location("skins/", hash)
}
pub fn cape_location(hash: &str) -> String {
    location("capes/", hash)
}
fn location(prefix: &str, hash: &str) -> String {
    let units: Vec<u8> = hash.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let digest = sha1(&units);
    let mut out = String::from(prefix);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn cape(textures: &Textures) -> Option<String> {
    textures
        .cape
        .as_deref()
        .and_then(texture_hash)
        .map(|hash| cape_location(&hash))
}

pub fn process_cape(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let mut img = vec![0u8; 64 * 64 * 4];
    let (fit_w, fit_h) = if w <= 64 && h <= 32 { (w, h) } else { (64, 32) };
    for y in 0..fit_h {
        for x in 0..fit_w {
            let (sx, sy) = (x * w / fit_w, y * h / fit_h);
            let (s, d) = ((sy * w + sx) * 4, (y * 64 + x) * 4);
            img[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    Some(img)
}

pub fn offline_cape() -> Option<&'static [u8]> {
    static CAPE: std::sync::OnceLock<Option<Vec<u8>>> = std::sync::OnceLock::new();
    CAPE.get_or_init(|| {
        let path = std::env::var_os("VISE_OFFLINE_CAPE").filter(|p| !p.is_empty())?;
        let image = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| crate::hud::decode_bytes(&bytes).map_err(|e| e.to_string()))
            .and_then(|(w, h, rgba)| process_cape(w, h, &rgba).ok_or("not an image".into()));
        image
            .inspect_err(|e| eprintln!("Vise: no offline cape ({}): {e}", path.display()))
            .ok()
    })
    .as_deref()
}

pub fn offline_skin() -> Option<&'static (Vec<u8>, bool)> {
    static SKIN: std::sync::OnceLock<Option<(Vec<u8>, bool)>> = std::sync::OnceLock::new();
    SKIN.get_or_init(|| {
        let path = std::env::var_os("VISE_OFFLINE_SKIN").filter(|p| !p.is_empty())?;
        let model = std::env::var("VISE_OFFLINE_SKIN_MODEL").unwrap_or_default();
        read_offline_skin(std::path::Path::new(&path), &model)
            .inspect_err(|e| eprintln!("Vise: no offline skin ({}): {e}", path.display()))
            .ok()
    })
    .as_ref()
}
fn read_offline_skin(path: &std::path::Path, model: &str) -> Result<(Vec<u8>, bool), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let (w, h, rgba) = crate::hud::decode_bytes(&bytes).map_err(|e| e.to_string())?;
    let skin = process_legacy_skin(w, h, &rgba).ok_or("not a 64x64 or 64x32 skin")?;
    Ok((skin, model == "slim"))
}

pub fn fetch_own_properties(id: u128) -> Vec<(String, String, Option<String>)> {
    crate::auth::discovery()
        .ok()
        .and_then(|doc| resolve_profile(&NameOrId::Id(id), &doc))
        .map(|p| p.properties)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Download {
    Ready,
    Pending,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Skin,
    Cape,
    Elytra,
}

pub fn lookup(
    textures: &Textures,
    require_secure: bool,
    mut download: impl FnMut(Kind, &str) -> Download,
) -> Option<String> {
    let mut skin = None;
    let mut state = Download::Ready;
    let urls = [
        (Kind::Skin, &textures.skin),
        (Kind::Cape, &textures.cape),
        (Kind::Elytra, &textures.elytra),
    ];
    for (kind, url) in urls {
        let Some(url) = url else { continue };
        let hash = texture_hash(url)?;
        if kind == Kind::Skin {
            skin = Some(skin_location(&hash));
        }
        match download(kind, url) {
            Download::Failed => return None,
            Download::Pending => state = Download::Pending,
            Download::Ready => {}
        }
    }
    if state != Download::Ready || require_secure && !textures.signed {
        return None;
    }
    skin
}

pub fn process_legacy_skin(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width != 64 || height != 32 && height != 64 {
        return None;
    }
    let legacy = height == 32;
    let mut img = vec![0u8; 64 * 64 * 4];
    let n = rgba.len().min(img.len());
    img[..n].copy_from_slice(&rgba[..n]);
    let at = |x: i32, y: i32| ((y * 64 + x) * 4) as usize;
    if legacy {
        let rects = [
            (4, 16, 16, 32, 4, 4),
            (8, 16, 16, 32, 4, 4),
            (0, 20, 24, 32, 4, 12),
            (4, 20, 16, 32, 4, 12),
            (8, 20, 8, 32, 4, 12),
            (12, 20, 16, 32, 4, 12),
            (44, 16, -8, 32, 4, 4),
            (48, 16, -8, 32, 4, 4),
            (40, 20, 0, 32, 4, 12),
            (44, 20, -8, 32, 4, 12),
            (48, 20, -16, 32, 4, 12),
            (52, 20, -8, 32, 4, 12),
        ];
        for (sx, sy, ox, oy, w, h) in rects {
            for y in 0..h {
                for x in 0..w {
                    let (s, d) = (at(sx + x, sy + y), at(sx + ox + w - 1 - x, sy + oy + y));
                    let p: [u8; 4] = img[s..s + 4].try_into().unwrap();
                    img[d..d + 4].copy_from_slice(&p);
                }
            }
        }
    }
    let no_alpha = |img: &mut Vec<u8>, x0: i32, y0: i32, x1: i32, y1: i32| {
        for x in x0..x1 {
            for y in y0..y1 {
                img[at(x, y) + 3] = 255;
            }
        }
    };
    no_alpha(&mut img, 0, 0, 32, 16);
    if legacy {
        let opaque = (32..64).all(|x| (0..32).all(|y| img[at(x, y) + 3] >= 128));
        if opaque {
            for x in 32..64 {
                for y in 0..32 {
                    img[at(x, y) + 3] = 0;
                }
            }
        }
    }
    no_alpha(&mut img, 0, 16, 64, 32);
    no_alpha(&mut img, 16, 48, 48, 64);
    Some(img)
}

pub struct Downloader {
    root: std::path::PathBuf,
    states: std::collections::HashMap<(Kind, String), Download>,
    done: std::sync::mpsc::Receiver<((Kind, String), Option<Vec<u8>>)>,
    send: std::sync::mpsc::Sender<((Kind, String), Option<Vec<u8>>)>,
    ready: Vec<(String, Vec<u8>)>,
    queued: std::collections::HashSet<String>,
    images: std::collections::HashMap<String, Vec<u8>>,
    image_order: std::collections::VecDeque<String>,
    drawn: std::collections::HashSet<String>,
}
const MAX_SKIN_IMAGES: usize = 1024;
impl Downloader {
    pub fn new(root: std::path::PathBuf) -> Self {
        let (send, done) = std::sync::mpsc::channel();
        Self {
            root,
            states: Default::default(),
            done,
            send,
            ready: Vec::new(),
            queued: Default::default(),
            images: Default::default(),
            image_order: Default::default(),
            drawn: Default::default(),
        }
    }
    pub fn state(&mut self, kind: Kind, url: &str) -> Download {
        while let Ok((key, image)) = self.done.try_recv() {
            let state = match image {
                Some(rgba) => {
                    match key.0 {
                        Kind::Skin => self.queue(skin_location(&key.1), rgba),
                        Kind::Cape => self.queue(cape_location(&key.1), rgba),
                        Kind::Elytra => {}
                    }
                    Download::Ready
                }
                None => Download::Failed,
            };
            self.states.insert(key, state);
        }
        let Some(hash) = texture_hash(url) else {
            return Download::Failed;
        };
        let key = (kind, hash);
        if let Some(state) = self.states.get(&key) {
            return *state;
        }
        self.states.insert(key.clone(), Download::Pending);
        let digest: String = skin_location(&key.1)["skins/".len()..].to_owned();
        let file = self
            .root
            .join(if digest.len() > 2 { &digest[..2] } else { "xx" })
            .join(&digest);
        let (url, send) = (url.to_owned(), self.send.clone());
        std::thread::spawn(move || {
            let bytes = std::fs::read(&file).ok().or_else(|| {
                let body = http_get(&url)?;
                let _ = std::fs::create_dir_all(file.parent()?);
                let _ = std::fs::write(&file, &body);
                Some(body)
            });
            let image = bytes
                .and_then(|b| crate::hud::decode_bytes(&b).ok())
                .and_then(|(w, h, rgba)| match kind {
                    Kind::Skin => process_legacy_skin(w, h, &rgba),
                    Kind::Cape => process_cape(w, h, &rgba),
                    Kind::Elytra => Some(rgba),
                });
            let _ = send.send((key, image));
        });
        Download::Pending
    }
    fn remember(&mut self, location: String, rgba: Vec<u8>) {
        if !self.images.contains_key(&location) {
            self.image_order.push_back(location.clone());
            while self.image_order.len() > MAX_SKIN_IMAGES
                && let Some(old) = self.image_order.pop_front()
            {
                self.images.remove(&old);
            }
        }
        self.images.insert(location, rgba);
    }
    fn queue(&mut self, location: String, rgba: Vec<u8>) {
        self.remember(location.clone(), rgba.clone());
        self.queued.insert(location.clone());
        self.ready.push((location, rgba));
    }
    pub fn again(&mut self, location: &str) -> bool {
        if self.queued.contains(location) {
            return true;
        }
        let (digest, process): (_, fn(u32, u32, &[u8]) -> Option<Vec<u8>>) =
            if let Some(digest) = location.strip_prefix("skins/") {
                (digest, process_legacy_skin)
            } else if let Some(digest) = location.strip_prefix("capes/") {
                (digest, process_cape)
            } else {
                return false;
            };
        let image = self.images.get(location).cloned().or_else(|| {
            let file = self
                .root
                .join(if digest.len() > 2 { &digest[..2] } else { "xx" })
                .join(digest);
            let (w, h, rgba) = crate::hud::decode_bytes(&std::fs::read(&file).ok()?).ok()?;
            process(w, h, &rgba)
        });
        let Some(image) = image else {
            return false;
        };
        self.queue(location.to_owned(), image);
        true
    }
    pub fn take_ready(&mut self) -> Vec<(String, Vec<u8>)> {
        for (location, _) in &self.ready {
            self.queued.remove(location);
        }
        std::mem::take(&mut self.ready)
    }
    pub fn drawn(&mut self, location: &str) {
        if !self.drawn.contains(location) {
            self.drawn.insert(location.to_owned());
        }
    }
    pub fn take_drawn(&mut self) -> std::collections::HashSet<String> {
        std::mem::take(&mut self.drawn)
    }
}

pub struct SkinSlots {
    capacity: usize,
    slots: Vec<(String, usize)>,
    kept: std::collections::HashSet<String>,
}
impl SkinSlots {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            slots: Vec::new(),
            kept: Default::default(),
        }
    }
    pub fn contains(&self, location: &str) -> bool {
        self.slots.iter().any(|(l, _)| l == location)
    }
    pub fn next_frame(&mut self, drawn: std::collections::HashSet<String>) {
        let (recent, old): (Vec<_>, Vec<_>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|(l, _)| drawn.contains(l));
        self.slots = old;
        self.slots.extend(recent);
        self.kept = drawn;
    }
    pub fn place(&mut self, location: String) -> Option<(usize, Option<String>)> {
        if let Some((_, slot)) = self.slots.iter().find(|(l, _)| *l == location) {
            return Some((*slot, None));
        }
        let (slot, evicted) = if self.slots.len() < self.capacity {
            (self.slots.len(), None)
        } else {
            let i = self
                .slots
                .iter()
                .position(|(l, _)| !self.kept.contains(l))?;
            let (old, slot) = self.slots.remove(i);
            (slot, Some(old))
        };
        self.kept.insert(location.clone());
        self.slots.push((location, slot));
        Some((slot, evicted))
    }
}

fn http_get(url: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Java")
        .build()
        .get(url)
        .call()
        .ok()?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(16 << 20)
        .read_to_end(&mut body)
        .ok()?;
    Some(body)
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend((data.len() as u64 * 8).to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn offline_skins_read_like_downloaded_ones() {
        let png = |w: u32, h: u32| {
            let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
            let mut out = Vec::new();
            let mut encoder = png::Encoder::new(&mut out, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&rgba)
                .unwrap();
            (out, rgba)
        };
        let dir = std::env::temp_dir().join(format!("vise-offline-skin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("steve.png");
        for (h, model, slim) in [(64, "slim", true), (32, "classic", false), (64, "", false)] {
            let (bytes, rgba) = png(64, h);
            std::fs::write(&file, bytes).unwrap();
            let (skin, is_slim) = super::read_offline_skin(&file, model).unwrap();
            assert_eq!(skin, super::process_legacy_skin(64, h, &rgba).unwrap());
            assert_eq!(skin.len(), 64 * 64 * 4);
            assert_eq!(is_slim, slim);
        }
        std::fs::write(&file, png(32, 32).0).unwrap();
        assert!(super::read_offline_skin(&file, "slim").is_err());
        std::fs::write(&file, b"not a png").unwrap();
        assert!(super::read_offline_skin(&file, "slim").is_err());
        assert!(super::read_offline_skin(&dir.join("missing.png"), "").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skins_model_comes_from_its_metadata() {
        use super::{Signature, TEXTURE_URIS, unpack};
        let slim = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYyIsIm1ldGFkYXRhIjp7Im1vZGVsIjoic2xpbSJ9fX19";
        let wide = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYyJ9fX0=";
        let t = unpack(slim, Signature::Signed, &TEXTURE_URIS);
        assert_eq!(
            t.skin.as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
        assert!(t.slim);
        assert!(!unpack(wide, Signature::Signed, &TEXTURE_URIS).slim);
    }
    #[test]
    fn profiles_parse_like_resolvable_profiles() {
        use super::{NameOrId, Signature, parse_profile};
        let p = parse_profile(&serde_json::json!("Notch"));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
        assert_eq!(p.id, vise_protocol::crypto::offline_uuid("Notch"));
        assert!(p.textures.is_none());
        let p = parse_profile(&serde_json::json!({"name": "Notch"}));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
        let id = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_u128;
        let ints: Vec<i64> = [
            (id >> 96) as i32 as i64,
            (id >> 64) as i32 as i64,
            (id >> 32) as i32 as i64,
            id as i32 as i64,
        ]
        .into();
        let p = parse_profile(&serde_json::json!({"id": ints}));
        assert_eq!(p.dynamic, Some(NameOrId::Id(id)));
        let p = parse_profile(&serde_json::json!({"id": "01234567-89ab-cdef-0123-4567abcdef00"}));
        assert_eq!(
            p.dynamic,
            Some(NameOrId::Id(0x0123_4567_89ab_cdef_0123_4567_abcd_ef00))
        );
        let p = parse_profile(&serde_json::json!({"name": "Notch", "id": ints}));
        assert!(p.dynamic.is_none());
        assert_eq!(p.id, id);
        let p = parse_profile(&serde_json::json!({
            "properties": [{"name": "textures", "value": "v"}]
        }));
        assert!(p.dynamic.is_none());
        assert_eq!(p.textures, Some(("v".into(), Signature::Unsigned)));
        let p = parse_profile(&serde_json::json!({
            "properties": [{"name": "textures", "value": "v", "signature": "s"}]
        }));
        assert_eq!(p.textures, Some(("v".into(), Signature::Signed)));
        let p = parse_profile(&serde_json::json!({"name": "Notch", "properties": []}));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
    }

    #[test]
    fn profiles_resolve_through_the_services() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let reply = |status: &str, body: &str| {
                format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            for stream in listener.incoming().take(3) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap();
                while reader.read_line(&mut String::new()).unwrap() > 2 {}
                let body = if line.contains("/users/profiles/minecraft/") {
                    r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#
                } else if line.contains("/session/minecraft/profile/")
                    && line.contains("unsigned=false")
                {
                    r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{"name":"textures","value":"dmFsdWU=","signature":"c2ln"}]}"#
                } else {
                    "{}"
                };
                stream.write_all(reply("200 OK", body).as_bytes()).unwrap();
            }
        });
        let doc = serde_json::json!({"discovery": {
            "profiles": {"endpoints": {"getByName": {"uri": format!("{base}/users/profiles/minecraft/{{name}}")}}},
            "session": {"endpoints": {"getProfileById": {"uri": format!("{base}/session/minecraft/profile/{{profileId}}")}}},
        }});
        let resolved =
            super::resolve_profile(&super::NameOrId::Name("Notch".into()), &doc).unwrap();
        assert_eq!(resolved.id, 0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5);
        assert_eq!(resolved.name, "Notch");
        assert_eq!(
            resolved.properties,
            vec![(
                "textures".to_owned(),
                "dmFsdWU=".to_owned(),
                Some("c2ln".to_owned())
            )]
        );
        let by_id = super::resolve_profile(&super::NameOrId::Id(resolved.id), &doc).unwrap();
        assert_eq!(by_id.name, "Notch");
        let doc = serde_json::json!({"discovery": {
            "session": {"endpoints": {"getProfileById": {"uri": format!("{base}/session/minecraft/profile/{{profileId}}")}}},
        }});
        assert!(super::resolve_profile(&super::NameOrId::Name("Notch".into()), &doc).is_none());
    }

    #[test]
    fn skins_drawn_last_frame_keep_their_slots() {
        let mut d = super::Downloader::new(std::env::temp_dir().join("vise-test-skin-slots"));
        let skins: Vec<String> = (0..6).map(|i| format!("skins/{i}")).collect();
        for (i, s) in skins.iter().enumerate() {
            d.remember(s.clone(), vec![i as u8]);
        }
        let mut slots = super::SkinSlots::new(4);
        let mut last: Option<Vec<String>> = None;
        for frame in 0..8 {
            slots.next_frame(d.take_drawn());
            let mut uploads = 0;
            for (location, _) in d.take_ready() {
                if slots.place(location).is_some() {
                    uploads += 1;
                }
            }
            let mut shown = Vec::new();
            for s in &skins {
                if slots.contains(s) {
                    d.drawn(s);
                    shown.push(s.clone());
                } else {
                    d.again(s);
                }
            }
            if frame >= 2 {
                assert_eq!(uploads, 0, "frame {frame} uploaded again");
                assert_eq!(Some(&shown), last.as_ref(), "frame {frame} lost a skin");
            }
            assert_eq!(shown.len(), if frame == 0 { 0 } else { 4 });
            last = Some(shown);
        }
        let mut slots = super::SkinSlots::new(2);
        slots.next_frame(Default::default());
        assert_eq!(slots.place("skins/a".into()), Some((0, None)));
        assert_eq!(slots.place("skins/b".into()), Some((1, None)));
        assert_eq!(slots.place("skins/c".into()), None);
        slots.next_frame(["skins/b".to_owned()].into());
        assert_eq!(
            slots.place("skins/c".into()),
            Some((0, Some("skins/a".into())))
        );
        assert_eq!(slots.place("skins/d".into()), None);
        assert!(slots.contains("skins/b") && slots.contains("skins/c"));
    }

    #[test]
    fn evicted_skins_upload_again() {
        let mut d = super::Downloader::new(std::env::temp_dir().join("vise-test-skins"));
        d.remember("skins/abc".into(), vec![1, 2, 3]);
        assert!(d.again("skins/abc"));
        assert!(!d.again("skins/missing"));
        assert!(d.again("skins/abc"));
        assert_eq!(
            d.take_ready(),
            vec![("skins/abc".to_owned(), vec![1, 2, 3])]
        );
        assert!(d.take_ready().is_empty());
        assert!(d.again("skins/abc"));
        assert_eq!(d.take_ready().len(), 1);
        d.queue("skins/def".into(), vec![4]);
        assert!(d.again("skins/def"));
        assert_eq!(d.take_ready(), vec![("skins/def".to_owned(), vec![4])]);
    }

    #[test]
    fn capes_fill_the_top_half_of_a_slot() {
        let at = |x: usize, y: usize| (y * 64 + x) * 4;
        let legacy = super::process_cape(22, 17, &[7; 22 * 17 * 4]).unwrap();
        assert_eq!(legacy.len(), 64 * 64 * 4);
        assert_eq!(legacy[at(21, 16)], 7);
        assert_eq!(legacy[at(22, 16)], 0);
        let mut hd = vec![0u8; 128 * 64 * 4];
        hd[(62 * 128 + 126) * 4] = 9;
        let hd = super::process_cape(128, 64, &hd).unwrap();
        assert_eq!(hd[at(63, 31)], 9);
        assert_eq!(hd[at(0, 32)], 0);
        assert!(super::process_cape(64, 32, &[0; 4]).is_none());
        let textures = super::Textures {
            cape: Some("http://textures.minecraft.net/texture/abc".into()),
            ..Default::default()
        };
        let cape = super::cape(&textures).unwrap();
        assert_eq!(
            cape["capes/".len()..],
            super::skin_location("abc")["skins/".len()..]
        );
    }

    #[test]
    fn profile_components_supply_unsigned_head_skins() {
        let value = "eyJ0ZXh0dXJlcyI6IHsiU0tJTiI6IHsidXJsIjogImh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYzEyMyJ9fX0=";
        let profile = serde_json::json!({"properties": [{"name": "textures", "value": value}]});
        assert_eq!(
            super::profile_skin_url(&profile).as_deref(),
            Some("https://textures.minecraft.net/texture/abc123")
        );
        let other = serde_json::json!({"properties": [{"name": "other", "value": value}]});
        assert_eq!(super::profile_skin_url(&other), None);
        let bad = serde_json::json!({"properties": [{"name": "textures", "value": "not base64"}]});
        assert_eq!(super::profile_skin_url(&bad), None);
        assert_eq!(super::profile_skin_url(&serde_json::json!({})), None);
    }

    #[test]
    fn http_get_reads_2xx_bodies() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap();
                while reader.read_line(&mut String::new()).unwrap() > 2 {}
                let reply: &[u8] = if line.contains("/texture/abc") {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPNG!"
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(reply).unwrap();
            }
        });
        assert_eq!(
            http_get(&format!("{base}/texture/abc")),
            Some(b"PNG!".to_vec())
        );
        assert_eq!(http_get(&format!("{base}/texture/missing")), None);
    }

    use super::*;

    #[test]
    fn skull_block_entity_profiles_supply_head_skins() {
        let value = "eyJ0ZXh0dXJlcyI6IHsiU0tJTiI6IHsidXJsIjogImh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYzEyMyJ9fX0=";
        let profile = vise_world::nbt::parse_snbt(&format!(
            r#"{{name:"T",id:[I;-158327916,1572358581,-2125825222,-487948715],properties:[{{name:"textures",value:"{value}"}}]}}"#
        ))
        .unwrap()
        .0;
        assert_eq!(
            super::profile_skin_url_nbt(&profile).as_deref(),
            Some("https://textures.minecraft.net/texture/abc123")
        );
        assert_eq!(
            super::profile_uuid_nbt(&profile),
            Some(
                [-158327916, 1572358581, -2125825222, -487948715]
                    .iter()
                    .fold(0u128, |a, &w: &i32| (a << 32) | u128::from(w as u32))
            )
        );
        let named = vise_world::nbt::Tag::String("MHF_Blaze".into());
        assert_eq!(super::profile_skin_url_nbt(&named), None);
        assert_eq!(super::profile_uuid_nbt(&named), None);
        let text_id = vise_world::nbt::parse_snbt(
            r#"{name:"T",id:"e843a1f4-0000-0001-0000-000200000003"}"#,
        )
        .unwrap()
        .0;
        assert_eq!(
            super::profile_uuid_nbt(&text_id),
            Some(u128::from_str_radix("e843a1f4000000010000000200000003", 16).unwrap())
        );
    }

    #[test]
    fn sha1_and_base64() {
        let hex: String = sha1(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(base64_decode("aGk=").unwrap(), b"hi");
        assert_eq!(base64_decode("aGk").unwrap(), b"hi");
        assert!(base64_decode("a").is_none());
        assert!(base64_decode("aGk=x").is_none());
        assert_eq!(
            texture_hash("http://textures.minecraft.net/texture/abc.png?x=1").unwrap(),
            "abc"
        );
    }
}
