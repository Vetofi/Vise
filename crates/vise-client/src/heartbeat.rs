use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Thread;
use std::time::Duration;

use vise_telemetry::live::{self, Ping};

static LAN: AtomicBool = AtomicBool::new(false);
static PINGER: OnceLock<Thread> = OnceLock::new();

const MAX_WAIT: Duration = Duration::from_secs(600);

pub fn start() {
    if PINGER.get().is_some() || std::env::var_os("VISE_NO_HEARTBEAT").is_some() {
        return;
    }
    let Some(endpoint) = vise_telemetry::endpoint() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("Heartbeat".into())
        .spawn(move || run(&endpoint));
    match spawned {
        Ok(handle) => {
            let _ = PINGER.set(handle.thread().clone());
        }
        Err(e) => log::warn!("Vise: no live count ping: {e}"),
    }
}

pub fn set_lan(open: bool) {
    if LAN.swap(open, Ordering::Relaxed) != open
        && let Some(pinger) = PINGER.get()
    {
        pinger.unpark();
    }
}

fn run(endpoint: &str) {
    let session = live::session_id();
    let version = crate::telemetry::version();
    let mut wait = live::INTERVAL;
    let mut failed = false;
    loop {
        let ping = Ping::new(&session, &version, LAN.load(Ordering::Relaxed));
        match live::send(endpoint, &ping) {
            Ok(()) => {
                wait = live::INTERVAL;
                failed = false;
            }
            Err(e) => {
                if !failed {
                    log::info!("Vise: the live count ping failed ({e}); trying again later");
                }
                failed = true;
                wait = (wait * 2).min(MAX_WAIT);
            }
        }
        std::thread::park_timeout(wait);
    }
}
