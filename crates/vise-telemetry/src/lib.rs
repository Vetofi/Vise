use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod batch;
pub mod consent;
pub mod live;
pub mod outbox;
pub mod payload;
pub mod scrub;
pub mod system;
#[cfg(feature = "upload")]
pub mod upload;

pub use consent::{Choices, Consent};
pub use payload::{Item, Kind, Payload, Source};

pub const SCHEMA: u32 = 1;

pub const CONSENT_VERSION: u32 = 2;

pub const DEFAULT_ENDPOINT: &str = "https://telemetry.playvise.dev";

pub fn endpoint() -> Option<String> {
    resolve(std::env::var("VISE_TELEMETRY_URL").ok().as_deref())
}

fn resolve(var: Option<&str>) -> Option<String> {
    match var.map(str::trim) {
        Some("off" | "none" | "0") => None,
        Some(url) if !url.is_empty() => Some(url.trim_end_matches('/').to_owned()),
        _ => Some(DEFAULT_ENDPOINT.to_owned()),
    }
}

pub fn dir(home: &Path) -> PathBuf {
    home.join("telemetry")
}

pub fn today() -> u32 {
    (now_secs() / 86_400) as u32
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if getrandom::fill(&mut buf).is_err() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (nanos >> (8 * (i % 16))) as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", random_hex(4)));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
pub(crate) mod test_home {
    use std::path::PathBuf;

    pub fn new(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vise-telemetry-{name}-{}", super::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_defaults_to_the_server_and_can_be_moved_or_turned_off() {
        assert_eq!(resolve(None).as_deref(), Some(DEFAULT_ENDPOINT));
        assert_eq!(resolve(Some(" ")).as_deref(), Some(DEFAULT_ENDPOINT));
        assert_eq!(
            resolve(Some("http://127.0.0.1:8080/")).as_deref(),
            Some("http://127.0.0.1:8080")
        );
        assert_eq!(resolve(Some("off")), None);
        assert!(DEFAULT_ENDPOINT.starts_with("https://") && !DEFAULT_ENDPOINT.ends_with('/'));
    }
}
