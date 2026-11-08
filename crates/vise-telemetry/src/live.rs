use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const INTERVAL: Duration = Duration::from_secs(60);

pub const WINDOW: Duration = Duration::from_secs(120);

pub const PLATFORMS: [&str; 4] = ["linux", "macos", "windows", "other"];

pub const MAX_PING_BYTES: usize = 512;

const MAX_VERSION: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub session: String,
    pub version: String,
    pub platform: String,
    pub lan: bool,
}

impl Ping {
    pub fn new(session: &str, version: &str, lan: bool) -> Self {
        Self {
            session: session.to_owned(),
            version: version.to_owned(),
            platform: platform().to_owned(),
            lan,
        }
    }

    pub fn check(&self) -> Result<(), &'static str> {
        if !crate::consent::is_install_id(&self.session) {
            return Err("session: 32 lowercase hex digits");
        }
        let version_ok = !self.version.is_empty()
            && self.version.len() <= MAX_VERSION
            && self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b" .+-_".contains(&b));
        if !version_ok {
            return Err("version: at most 40 letters, digits, spaces and .+-_");
        }
        if !PLATFORMS.contains(&self.platform.as_str()) {
            return Err("platform: linux, macos, windows or other");
        }
        Ok(())
    }
}

pub fn session_id() -> String {
    crate::random_hex(16)
}

pub fn platform() -> &'static str {
    match std::env::consts::OS {
        os @ ("linux" | "macos" | "windows") => os,
        _ => "other",
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Count {
    pub playing: u64,
    pub lan: u64,
    pub versions: BTreeMap<String, u64>,
    pub platforms: BTreeMap<String, u64>,
}

pub fn round(n: u64) -> u64 {
    let step = match n {
        0..10 => return n,
        10..100 => 5,
        100..1000 => 10,
        _ => 100,
    };
    (n + step / 2) / step * step
}

#[cfg(feature = "live")]
pub fn send(endpoint: &str, ping: &Ping) -> Result<(), String> {
    agent()
        .post(&format!("{endpoint}/v1/live"))
        .set("Content-Type", "application/json")
        .send_string(&serde_json::to_string(ping).map_err(|e| e.to_string())?)
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "live")]
pub fn fetch(endpoint: &str) -> Result<Count, String> {
    let body = agent()
        .get(&format!("{endpoint}/v1/live"))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

#[cfg(feature = "live")]
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping() -> Ping {
        Ping::new(&session_id(), "v2026.10.15", false)
    }

    #[test]
    fn a_ping_holds_only_the_four_fields() {
        let json = serde_json::to_value(Ping::new(&"a".repeat(32), "0.2.0", true)).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["lan", "platform", "session", "version"]);
        assert!(serde_json::to_string(&ping()).unwrap().len() < MAX_PING_BYTES);
    }

    #[test]
    fn session_ids_are_fresh_each_time() {
        let (a, b) = (session_id(), session_id());
        assert_ne!(a, b);
        assert!(ping().check().is_ok());
    }

    #[test]
    fn bad_pings_are_refused() {
        let bad = |f: fn(&mut Ping)| {
            let mut p = ping();
            f(&mut p);
            p.check().is_err()
        };
        assert!(bad(|p| p.session = "ABC".into()));
        assert!(bad(|p| p.session = "G".repeat(32)));
        assert!(bad(|p| p.version = String::new()));
        assert!(bad(|p| p.version = "1".repeat(41)));
        assert!(bad(|p| p.version = "<script>".into()));
        assert!(bad(|p| p.platform = "haiku".into()));
        assert!(!bad(|p| p.version = "dev 0.1.0".into()));
        let extra = r#"{"session":"00000000000000000000000000000000","version":"0.2.0","platform":"linux","lan":false,"name":"Steve"}"#;
        assert!(serde_json::from_str::<Ping>(extra).is_err());
    }

    #[test]
    fn counts_round_more_as_they_grow() {
        let cases = [
            (0, 0),
            (1, 1),
            (9, 9),
            (10, 10),
            (12, 10),
            (13, 15),
            (97, 95),
            (98, 100),
            (104, 100),
            (105, 110),
            (994, 990),
            (995, 1000),
            (1049, 1000),
            (1050, 1100),
        ];
        for (n, shown) in cases {
            assert_eq!(round(n), shown, "{n}");
        }
    }
}
