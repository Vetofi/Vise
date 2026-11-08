use std::time::{Duration, Instant};

pub const DELAYS: [u64; 5] = [5, 10, 20, 40, 60];
pub const STABLE: Duration = Duration::from_secs(60);

pub fn allowed(reason: &str, from_server: bool) -> bool {
    let r = reason.to_lowercase();
    const NEVER: [&str; 13] = [
        "ban",
        "kick",
        "white-list",
        "whitelist",
        "incompatible",
        "outdated",
        "another location",
        "online mode",
        "invalid server address",
        "invalid session",
        "multiplayer is disabled",
        "failed to verify username",
        "microsoft account",
    ];
    if NEVER.iter().any(|w| r.contains(w)) {
        return false;
    }
    if !from_server {
        return true;
    }
    const RETRY: [&str; 6] = [
        "server closed",
        "timed out",
        "the server is full",
        "took too long to log in",
        "authentication servers",
        "restart",
    ];
    RETRY.iter().any(|w| r.contains(w))
}

#[derive(Debug, Default)]
pub struct Tracker {
    address: Option<String>,
    name: String,
    attempts: usize,
    joined: Option<Instant>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Retry {
    pub address: String,
    pub name: String,
    pub at: Instant,
    pub attempt: usize,
}
impl Retry {
    pub fn seconds_left(&self, now: Instant) -> u64 {
        let left = self.at.saturating_duration_since(now);
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    }
    pub fn due(&self, now: Instant) -> bool {
        now >= self.at
    }
    pub fn text(&self, now: Instant) -> String {
        format!(
            "Reconnecting in {} s (attempt {} of {})",
            self.seconds_left(now),
            self.attempt,
            DELAYS.len()
        )
    }
}
impl Tracker {
    pub fn connecting(&mut self, address: &str, name: &str) {
        if self.address.as_deref() != Some(address) {
            self.address = Some(address.to_owned());
            self.attempts = 0;
        }
        self.name = name.to_owned();
        self.joined = None;
    }
    pub fn joined(&mut self, now: Instant) {
        self.joined = Some(now);
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn ended(&mut self, reason: &str, from_server: bool, now: Instant) -> Option<Retry> {
        if self
            .joined
            .take()
            .is_some_and(|t| now.saturating_duration_since(t) >= STABLE)
        {
            self.attempts = 0;
        }
        let address = self.address.clone()?;
        if !allowed(reason, from_server) || self.attempts >= DELAYS.len() {
            self.reset();
            return None;
        }
        let delay = Duration::from_secs(DELAYS[self.attempts]);
        self.attempts += 1;
        Some(Retry {
            address,
            name: self.name.clone(),
            at: now + delay,
            attempt: self.attempts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lost_connections_and_shutdowns_are_retried() {
        assert!(allowed("Connection reset by peer (os error 104)", false));
        assert!(allowed("Connection refused (os error 111)", false));
        assert!(allowed("Server closed", true));
        assert!(allowed("Timed out", true));
        assert!(allowed("The server is full!", true));
        assert!(allowed("Server is restarting, back soon", true));
        assert!(!allowed("Kicked by an operator", true));
        assert!(!allowed(
            "You are banned from this server.\nReason: griefing",
            true
        ));
        assert!(!allowed("You are not white-listed on this server!", true));
        assert!(!allowed("You logged in from another location", true));
        assert!(!allowed("Incompatible client! Please use 26.3", true));
        assert!(!allowed("You have been idle for too long!", true));
        assert!(
            !allowed("Go away", true),
            "unknown server reasons are not retried"
        );
        assert!(!allowed(
            "This server is in online mode. Vise only joins offline-mode servers.",
            true
        ));
        assert!(!allowed("Kicked for spamming", false));
    }

    #[test]
    fn online_mode_login_failures() {
        let failed = |why: &str| format!("Failed to log in: {why}");
        assert!(!allowed(
            &failed("Invalid session (Try restarting your game and the launcher)"),
            false
        ));
        assert!(!allowed(
            &failed("You are banned from playing online"),
            false
        ));
        assert!(!allowed(
            &failed("Multiplayer is disabled. Please check your Microsoft account settings."),
            false
        ));
        assert!(!allowed(crate::net::NOT_SIGNED_IN, false));
        assert!(allowed(
            &failed("The authentication servers are currently not reachable. Please try again."),
            false
        ));
        assert!(allowed(&failed("Connection timed out"), false));
        assert!(!allowed("Failed to verify username!", true));
        assert!(allowed(
            "Authentication servers are down. Please try again later. Sorry!",
            true
        ));
    }

    #[test]
    fn attempts_back_off_and_stop() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.connecting("localhost:25565", "Tester");
        let mut delays = Vec::new();
        while let Some(r) = t.ended("Connection refused", false, now) {
            assert_eq!(
                (r.address.as_str(), r.name.as_str()),
                ("localhost:25565", "Tester")
            );
            assert_eq!(r.attempt, delays.len() + 1);
            delays.push(r.at.duration_since(now).as_secs());
            t.connecting(&r.address, &r.name);
        }
        assert_eq!(delays, DELAYS);
        assert_eq!(t.ended("Connection refused", false, now), None);
    }

    #[test]
    fn a_short_stay_does_not_reset_the_count() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.connecting("a", "P");
        for _ in 0..DELAYS.len() {
            t.connecting("a", "P");
            t.joined(now);
            assert!(
                t.ended("Server closed", true, now + Duration::from_secs(5))
                    .is_some()
            );
        }
        t.connecting("a", "P");
        t.joined(now);
        assert_eq!(
            t.ended("Server closed", true, now + Duration::from_secs(5)),
            None
        );
        t.connecting("a", "P");
        for _ in 0..3 {
            t.ended("Timed out", true, now);
            t.connecting("a", "P");
        }
        t.joined(now);
        let r = t.ended("Timed out", true, now + STABLE).unwrap();
        assert_eq!(r.attempt, 1);
    }

    #[test]
    fn kicks_and_leaving_reset_and_new_addresses_start_over() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.connecting("a", "P");
        assert!(t.ended("Server closed", true, now).is_some());
        assert_eq!(t.ended("Kicked by an operator", true, now), None);
        assert_eq!(
            t.ended("Server closed", true, now),
            None,
            "forgotten after a kick"
        );
        t.connecting("a", "P");
        t.ended("Server closed", true, now);
        t.connecting("b", "P");
        assert_eq!(t.ended("Server closed", true, now).unwrap().attempt, 1);
        let mut t = Tracker::default();
        assert_eq!(t.ended("Server closed", true, now), None, "never connected");
    }

    #[test]
    fn the_countdown_rounds_up() {
        let now = Instant::now();
        let r = Retry {
            address: "a".into(),
            name: "P".into(),
            at: now + Duration::from_millis(4200),
            attempt: 2,
        };
        assert_eq!(r.seconds_left(now), 5);
        assert_eq!(r.text(now), "Reconnecting in 5 s (attempt 2 of 5)");
        assert!(!r.due(now));
        assert!(r.due(now + Duration::from_secs(5)));
        assert_eq!(r.seconds_left(now + Duration::from_secs(9)), 0);
    }
}
