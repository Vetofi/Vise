use std::collections::HashMap;
use std::hash::{Hash, RandomState};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use vise_telemetry::batch;

pub mod classes;
pub mod github;
pub mod hub;
pub mod intake;
pub mod live;
pub mod store;
pub mod summary;

use github::Filer;
use vise_telemetry::live::{MAX_PING_BYTES, Ping};
use live::Live;
use store::Store;

pub struct Config {
    pub retention_days: u32,
    pub min_install_days: u64,
    pub requests_per_hour: f64,
    pub items_per_day: u32,
    pub trust_proxy: bool,
    pub allow_origin: String,
    pub live_per_hour: f64,
    pub live_window: Duration,
    pub live_max_sessions: usize,
    pub hub_per_hour: f64,
    pub hub_posts_per_hour: f64,
    pub hub_reports_per_day: u32,
    pub hub_keep_days: u32,
    pub github_token: Option<String>,
    pub github_repo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retention_days: 180,
            min_install_days: 5,
            requests_per_hour: 60.0,
            items_per_day: 100,
            trust_proxy: false,
            allow_origin: "https://playvise.dev".into(),
            live_per_hour: 600.0,
            live_window: vise_telemetry::live::WINDOW,
            live_max_sessions: 200_000,
            hub_per_hour: 600.0,
            hub_posts_per_hour: 20.0,
            hub_reports_per_day: 30,
            hub_keep_days: 90,
            github_token: None,
            github_repo: "vise-rust/vise".into(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(format!("VISE_TELEMETRY_{k}")).ok();
        let secret = |k: &str| {
            var(k)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let d = Self::default();
        Self {
            retention_days: var("RETENTION_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.retention_days),
            min_install_days: var("MIN_INSTALL_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.min_install_days),
            requests_per_hour: var("REQUESTS_PER_HOUR")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.requests_per_hour),
            items_per_day: var("ITEMS_PER_DAY")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.items_per_day),
            trust_proxy: var("TRUST_PROXY").is_some_and(|v| v == "1"),
            allow_origin: var("ALLOW_ORIGIN").unwrap_or(d.allow_origin),
            live_per_hour: var("LIVE_PER_HOUR")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.live_per_hour),
            live_window: var("LIVE_WINDOW_SECONDS")
                .and_then(|v| v.parse().ok())
                .map_or(d.live_window, Duration::from_secs),
            live_max_sessions: var("LIVE_MAX_SESSIONS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.live_max_sessions),
            hub_per_hour: var("HUB_PER_HOUR")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.hub_per_hour),
            hub_posts_per_hour: var("HUB_POSTS_PER_HOUR")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.hub_posts_per_hour),
            hub_reports_per_day: var("HUB_REPORTS_PER_DAY")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.hub_reports_per_day),
            hub_keep_days: var("HUB_KEEP_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.hub_keep_days),
            github_token: secret("GITHUB_TOKEN"),
            github_repo: var("GITHUB_REPO").unwrap_or(d.github_repo),
        }
    }
}

pub struct App {
    store: Mutex<Store>,
    config: Config,
    buckets: Mutex<Buckets>,
    live_buckets: Mutex<Buckets>,
    live: Mutex<Live>,
    live_cache: Mutex<Option<(Instant, String)>>,
    cache: Mutex<HashMap<u32, (Instant, String)>>,
    summary_cache: Mutex<Option<(Instant, String)>>,
    today: Box<dyn Fn() -> u32 + Send + Sync>,
    hub_key: RandomState,
    hub_buckets: Mutex<hub::HashedBuckets>,
    hub_post_buckets: Mutex<hub::HashedBuckets>,
    votes_cache: Mutex<Option<(Instant, String)>>,
    filer: Option<Arc<Filer>>,
}

type Buckets = HashMap<IpAddr, (f64, Instant)>;

const CACHE_FOR: Duration = Duration::from_secs(600);

fn status(code: StatusCode, message: &str) -> Response {
    (code, message.to_owned()).into_response()
}

impl App {
    pub fn new(store: Store, config: Config) -> Arc<Self> {
        Self::with_clock(store, config, Box::new(vise_telemetry::today))
    }

    pub fn with_clock(
        store: Store,
        config: Config,
        today: Box<dyn Fn() -> u32 + Send + Sync>,
    ) -> Arc<Self> {
        let filer = config
            .github_token
            .clone()
            .map(|token| Arc::from(github::github(config.github_repo.clone(), token)));
        Self::with_filer(store, config, today, filer)
    }

    pub fn with_filer(
        store: Store,
        config: Config,
        today: Box<dyn Fn() -> u32 + Send + Sync>,
        filer: Option<Arc<Filer>>,
    ) -> Arc<Self> {
        let live = Live::new(config.live_window, config.live_max_sessions);
        Arc::new(Self {
            store: Mutex::new(store),
            config,
            buckets: Mutex::default(),
            live_buckets: Mutex::default(),
            live: Mutex::new(live),
            live_cache: Mutex::default(),
            cache: Mutex::default(),
            summary_cache: Mutex::default(),
            today,
            hub_key: RandomState::new(),
            hub_buckets: Mutex::default(),
            hub_post_buckets: Mutex::default(),
            votes_cache: Mutex::default(),
            filer,
        })
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn allow(&self, ip: IpAddr) -> bool {
        take(&self.buckets, self.config.requests_per_hour, ip)
    }

    fn allow_live(&self, ip: IpAddr) -> bool {
        take(&self.live_buckets, self.config.live_per_hour, ip)
    }

    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn client(&self, headers: &HeaderMap, peer: Option<SocketAddr>) -> IpAddr {
        let header = |name: &str| headers.get(name)?.to_str().ok();
        let forwarded = self
            .config
            .trust_proxy
            .then(|| {
                header("x-real-ip")
                    .and_then(|ip| ip.trim().parse().ok())
                    .or_else(|| {
                        header("x-forwarded-for")?
                            .rsplit(',')
                            .next()?
                            .trim()
                            .parse()
                            .ok()
                    })
            })
            .flatten();
        rate_key(
            forwarded
                .or(peer.map(|p| p.ip()))
                .unwrap_or(IpAddr::from([0, 0, 0, 0])),
        )
    }

    pub fn newest_day(&self, install: &[u8]) -> Option<u32> {
        self.store().newest_day(install).ok().flatten()
    }

    fn forget_cached(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.summary_cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn maintain(&self) -> rusqlite::Result<usize> {
        let oldest = (self.today)().saturating_sub(self.config.retention_days);
        let removed = self.store().expire(oldest)?;
        self.expire_intake()?;
        self.forget_hub_buckets();
        for buckets in [&self.buckets, &self.live_buckets] {
            buckets
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|_, (_, at)| at.elapsed() < Duration::from_secs(3600));
        }
        self.live().forget_expired(Instant::now());
        if removed > 0 {
            self.forget_cached();
        }
        Ok(removed)
    }
}

fn take<K: Hash + Eq>(buckets: &Mutex<HashMap<K, (f64, Instant)>>, per_hour: f64, key: K) -> bool {
    let rate = per_hour / 3600.0;
    let burst = (per_hour / 10.0).max(1.0);
    let mut buckets = buckets.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let (tokens, at) = buckets.entry(key).or_insert((burst, now));
    *tokens = (*tokens + now.duration_since(*at).as_secs_f64() * rate).min(burst);
    *at = now;
    if *tokens < 1.0 {
        return false;
    }
    *tokens -= 1.0;
    true
}

fn rate_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

fn peer(req: &Request) -> Option<SocketAddr> {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
}

async fn receive(State(app): State<Arc<App>>, req: Request) -> Response {
    let ip = app.client(req.headers(), peer(&req));
    if !app.allow(ip) {
        return status(StatusCode::TOO_MANY_REQUESTS, "too many requests");
    }
    let zstd = req
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"zstd"));
    if !zstd {
        return status(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send zstd (Content-Encoding: zstd)",
        );
    }
    let Ok(body) = axum::body::to_bytes(req.into_body(), batch::MAX_COMPRESSED).await else {
        return status(StatusCode::PAYLOAD_TOO_LARGE, "a batch is at most 256 KiB");
    };
    let batch = match batch::decode(&body) {
        Ok(batch) => batch,
        Err(e) => return status(StatusCode::BAD_REQUEST, &e),
    };
    let today = (app.today)();
    let install = store::install_key(&batch.install_id).unwrap_or_default();
    let mut store = app.store();
    let sent = store.count_on(&install, today).unwrap_or(u32::MAX);
    if sent.saturating_add(batch.items.len() as u32) > app.config.items_per_day {
        return status(
            StatusCode::TOO_MANY_REQUESTS,
            "this install's daily limit is reached",
        );
    }
    match store.insert(&batch, today) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            eprintln!("vise-telemetry-server: could not store a batch: {e}");
            status(StatusCode::SERVICE_UNAVAILABLE, "try again later")
        }
    }
}

async fn forget(State(app): State<Arc<App>>, Path(id): Path<String>, req: Request) -> Response {
    if !app.allow(app.client(req.headers(), peer(&req))) {
        return status(StatusCode::TOO_MANY_REQUESTS, "too many requests");
    }
    let Some(install) = store::install_key(&id) else {
        return status(StatusCode::BAD_REQUEST, "not an install ID");
    };
    match app.store().delete_install(&install) {
        Ok(_) => {
            app.forget_cached();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            eprintln!("vise-telemetry-server: could not delete an install: {e}");
            status(StatusCode::SERVICE_UNAVAILABLE, "try again later")
        }
    }
}

#[derive(serde::Deserialize)]
struct Window {
    days: Option<u32>,
}

async fn aggregate(State(app): State<Arc<App>>, Query(window): Query<Window>) -> Response {
    let days = window.days.unwrap_or(30).clamp(1, 366);
    let cached = app
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&days)
        .filter(|(at, _)| at.elapsed() < CACHE_FOR)
        .map(|(_, json)| json.clone());
    let json = match cached {
        Some(json) => json,
        None => {
            let to = (app.today)();
            let made =
                app.store()
                    .aggregate(to + 1 - days.min(to + 1), to, app.config.min_install_days);
            match made {
                Ok(value) => {
                    let json = value.to_string();
                    app.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(days, (Instant::now(), json.clone()));
                    json
                }
                Err(e) => {
                    eprintln!("vise-telemetry-server: could not build the totals: {e}");
                    return status(StatusCode::SERVICE_UNAVAILABLE, "try again later");
                }
            }
        }
    };
    json_response(&app, json, CACHE_FOR.as_secs())
}

async fn benchmarks(State(app): State<Arc<App>>) -> Response {
    let cached = app
        .summary_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(at, _)| at.elapsed() < CACHE_FOR)
        .map(|(_, json)| json.clone());
    let json = match cached {
        Some(json) => json,
        None => {
            let oldest = (app.today)().saturating_sub(app.config.retention_days);
            let uploads = match app.store().benchmark_uploads(oldest) {
                Ok(uploads) => uploads,
                Err(e) => {
                    eprintln!("vise-telemetry-server: could not read the benchmarks: {e}");
                    return status(StatusCode::SERVICE_UNAVAILABLE, "try again later");
                }
            };
            let json = summary::summary(&uploads, summary::MIN_INSTALLS, &utc_now()).to_string();
            *app.summary_cache.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((Instant::now(), json.clone()));
            json
        }
    };
    json_response(&app, json, CACHE_FOR.as_secs())
}

fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

async fn ping(State(app): State<Arc<App>>, req: Request) -> Response {
    if !app.allow_live(app.client(req.headers(), peer(&req))) {
        return status(StatusCode::TOO_MANY_REQUESTS, "too many requests");
    }
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_PING_BYTES).await else {
        return status(StatusCode::PAYLOAD_TOO_LARGE, "a ping is at most 512 bytes");
    };
    let ping: Ping = match serde_json::from_slice(&body) {
        Ok(ping) => ping,
        Err(e) => return status(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if let Err(e) = ping.check() {
        return status(StatusCode::BAD_REQUEST, e);
    }
    match app.live().ping(&ping, Instant::now()) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(live::Full) => status(StatusCode::SERVICE_UNAVAILABLE, "try again later"),
    }
}

async fn playing(State(app): State<Arc<App>>, req: Request) -> Response {
    if !app.allow_live(app.client(req.headers(), peer(&req))) {
        return status(StatusCode::TOO_MANY_REQUESTS, "too many requests");
    }
    let fresh_for = app.config.live_window / 8;
    let json = {
        let mut cache = app.live_cache.lock().unwrap_or_else(|e| e.into_inner());
        match &*cache {
            Some((at, json)) if at.elapsed() < fresh_for => json.clone(),
            _ => {
                let json =
                    serde_json::to_string(&app.live().count(Instant::now())).unwrap_or_default();
                *cache = Some((Instant::now(), json.clone()));
                json
            }
        }
    };
    json_response(&app, json, fresh_for.as_secs())
}

fn json_response(app: &App, json: String, max_age: u64) -> Response {
    let origin =
        HeaderValue::from_str(&app.config.allow_origin).unwrap_or(HeaderValue::from_static("*"));
    let cache = HeaderValue::from_str(&format!("public, max-age={max_age}"))
        .unwrap_or(HeaderValue::from_static("no-cache"));
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::CACHE_CONTROL, cache),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, origin),
        ],
        json,
    )
        .into_response()
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/v1/batch", post(receive))
        .route("/v1/live", post(ping).get(playing))
        .route("/v1/installs/{id}", delete(forget))
        .route("/v1/aggregate", get(aggregate))
        .route("/v1/benchmarks/summary", get(benchmarks))
        .route("/hub/vote", post(hub::vote).options(hub::preflight))
        .route("/hub/votes", get(hub::votes))
        .route("/hub/report", post(intake::report).options(hub::preflight))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_rates_count_by_64() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(rate_key(ip("2001:db8::1")), rate_key(ip("2001:db8::99")));
        assert_ne!(rate_key(ip("2001:db8::1")), rate_key(ip("2001:db8:1::1")));
        assert_eq!(
            rate_key(ip("::ffff:203.0.113.7")),
            IpAddr::from([203, 0, 113, 7])
        );
        assert_eq!(rate_key(ip("203.0.113.8")), IpAddr::from([203, 0, 113, 8]));
    }
}
