use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Local};

static DETAILS: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
static REPORTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static CONTAINED: Cell<u32> = const { Cell::new(0) };
    static CAUGHT: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn contain<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    CONTAINED.with(|c| c.set(c.get() + 1));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CONTAINED.with(|c| c.set(c.get() - 1));
    result.map_err(|payload| {
        CAUGHT
            .with(|c| c.borrow_mut().take())
            .unwrap_or_else(|| payload_text(&*payload))
    })
}

fn contained_panic(info: &std::panic::PanicHookInfo) -> bool {
    if CONTAINED.with(Cell::get) == 0 {
        return false;
    }
    let message = panic_message(info);
    let stack = stack_lines(
        &std::backtrace::Backtrace::force_capture().to_string(),
        true,
    );
    log::error!(
        "Recovered from an error: {message}\n{}",
        stack
            .lines()
            .map(|l| format!("\tat {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    CAUGHT.with(|c| *c.borrow_mut() = Some(message));
    true
}

pub fn set_detail(key: &'static str, value: impl Into<String>) {
    let mut details = DETAILS.lock().unwrap_or_else(|e| e.into_inner());
    let value = value.into();
    match details.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value,
        None => details.push((key, value)),
    }
}

pub fn clear_detail(key: &'static str) {
    DETAILS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(k, _)| *k != key);
}

fn details() -> Vec<(&'static str, String)> {
    DETAILS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

const COMMENTS: &[&str] = &[
    "Something went wrong.",
    "The game stopped here.",
    "This should not have happened.",
    "Details below.",
];

pub struct Report {
    pub time: DateTime<Local>,
    pub description: String,
    pub error: String,
    pub stack: String,
    pub thread: String,
    pub side: &'static str,
    pub details: Vec<(String, String)>,
    pub log: Vec<String>,
}

pub fn file_name(time: &DateTime<Local>, side: &str) -> String {
    format!("crash-{}-{side}.txt", time.format("%Y-%m-%d_%H.%M.%S"))
}

impl Report {
    pub fn text(&self) -> String {
        let comment = COMMENTS[self.time.timestamp().rem_euclid(COMMENTS.len() as i64) as usize];
        let mut out = String::new();
        let _ = writeln!(out, "---- Vise Crash Report ----");
        let _ = writeln!(out, "// {comment}\n");
        let _ = writeln!(out, "Time: {}", self.time.format("%Y-%m-%d %H:%M:%S"));
        let _ = writeln!(out, "Description: {}\n", self.description);
        let _ = writeln!(out, "{}", self.error);
        for line in self.stack.lines() {
            let _ = writeln!(out, "\tat {line}");
        }
        let _ = writeln!(
            out,
            "\n\nA detailed walkthrough of the error, its code path and all known details is as follows:"
        );
        let _ = writeln!(out, "{}\n", "-".repeat(87));
        let _ = writeln!(out, "-- Head --");
        let _ = writeln!(out, "Thread: {}\n", self.thread);
        let _ = writeln!(out, "-- System Details --");
        let _ = writeln!(out, "Details:");
        for (key, value) in &self.details {
            let _ = writeln!(out, "\t{key}: {value}");
        }
        let _ = writeln!(out, "\n-- Last {} log lines --", self.log.len());
        for line in &self.log {
            let _ = writeln!(out, "{line}");
        }
        out
    }

    pub fn save(&self, home: &Path) -> std::io::Result<PathBuf> {
        let dir = crate::logging::crash_reports_dir(home);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(file_name(&self.time, self.side));
        std::fs::write(&path, self.text())?;
        Ok(path)
    }
}

pub fn version() -> String {
    if let Some(label) = vise_core::release::nightly_label() {
        return label;
    }
    let version = env!("CARGO_PKG_VERSION");
    match (vise_core::release::RELEASE, release_tag()) {
        (Some(_), Some(tag)) => tag,
        (None, Some(tag)) => format!("{tag} (client {version})"),
        (_, None) => version.to_owned(),
    }
}

pub fn release_tag() -> Option<String> {
    if let Some(nightly) = vise_core::release::nightly_version() {
        return Some(nightly);
    }
    if let Some(version) = vise_core::release::RELEASE {
        return Some(format!("v{version}"));
    }
    let exe = std::env::current_exe().ok()?;
    let folder = exe.parent()?.file_name()?.to_str()?;
    (folder.starts_with('v') && folder[1..].starts_with(|c: char| c.is_ascii_digit()))
        .then(|| folder.to_owned())
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn operating_system() -> String {
    let arch = std::env::consts::ARCH;
    let version = if cfg!(target_os = "linux") {
        let name = read("/etc/os-release").and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        });
        let kernel = read("/proc/sys/kernel/osrelease").map(|k| k.trim().to_owned());
        match (name, kernel) {
            (Some(n), Some(k)) => Some(format!("{n}, kernel {k}")),
            (n, k) => n.or(k),
        }
    } else if cfg!(target_os = "macos") {
        command_output("sw_vers", &["-productVersion"]).map(|v| format!("macOS {v}"))
    } else if cfg!(windows) {
        command_output("cmd", &["/C", "ver"])
    } else {
        None
    };
    format!(
        "{} ({arch}) version {}",
        std::env::consts::OS,
        version.unwrap_or_else(|| "unknown".into())
    )
}

pub fn cpu() -> String {
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let name = if cfg!(target_os = "linux") {
        read("/proc/cpuinfo").and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_owned())
        })
    } else if cfg!(target_os = "macos") {
        command_output("sysctl", &["-n", "machdep.cpu.brand_string"])
    } else {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    };
    format!(
        "{threads}x {}",
        name.unwrap_or_else(|| std::env::consts::ARCH.into())
    )
}

fn memory() -> String {
    const MIB: u64 = 1024 * 1024;
    let kib = |text: &str, key: &str| -> Option<u64> {
        text.lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let (total, used) = if cfg!(target_os = "linux") {
        (
            read("/proc/meminfo")
                .and_then(|t| kib(&t, "MemTotal:"))
                .map(|k| k * 1024),
            read("/proc/self/status")
                .and_then(|t| kib(&t, "VmRSS:"))
                .map(|k| k * 1024),
        )
    } else if cfg!(target_os = "macos") {
        (
            command_output("sysctl", &["-n", "hw.memsize"]).and_then(|v| v.parse().ok()),
            None,
        )
    } else if cfg!(windows) {
        (
            command_output(
                "powershell",
                &[
                    "-NoProfile",
                    "-Command",
                    "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
                ],
            )
            .and_then(|v| v.parse().ok()),
            None,
        )
    } else {
        (None, None)
    };
    match (total, used) {
        (Some(t), Some(u)) => format!(
            "{} MiB used by the game, {} MiB installed",
            u / MIB,
            t / MIB
        ),
        (Some(t), None) => format!("{} MiB installed", t / MIB),
        _ => "unknown".into(),
    }
}

pub fn system_details(side: &str) -> Vec<(String, String)> {
    let mut out = vec![
        ("Vise Version".to_owned(), version()),
        (
            "Minecraft Version".to_owned(),
            vise_core::VANILLA_VERSION.to_owned(),
        ),
        ("Operating System".to_owned(), operating_system()),
        ("CPU".to_owned(), cpu()),
        ("Memory".to_owned(), memory()),
    ];
    out.extend(
        details()
            .into_iter()
            .filter(|(k, _)| side != "server" || *k != "Render Settings")
            .map(|(k, v)| (k.to_owned(), v)),
    );
    out
}

fn thread_name(side: &str) -> String {
    let current = std::thread::current();
    match current.name() {
        Some("main") | None if side == "server" => "Server thread".into(),
        Some("main") | None => "Render thread".into(),
        Some(name) => name.into(),
    }
}

pub const SAVED_LINE: &str = "Vise: crash report saved to ";

fn deliver(home: &Path, report: Report) -> Option<PathBuf> {
    if REPORTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    crate::logging::write_only(
        log::Level::Error,
        &format!(
            "Vise crashed: {}\n{}",
            report.error,
            report
                .stack
                .lines()
                .map(|l| format!("\tat {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    );
    crate::logging::flush();
    crate::telemetry::keep_crash(home, &report);
    match report.save(home) {
        Ok(path) => {
            eprintln!("{SAVED_LINE}{}", path.display());
            Some(path)
        }
        Err(e) => {
            eprintln!("Vise: could not save a crash report: {e}");
            eprintln!("{}", report.text());
            None
        }
    }
}

pub fn report_watchdog(home: &Path, side: &'static str, message: String) -> Option<PathBuf> {
    deliver(
        home,
        Report {
            time: Local::now(),
            description: "Watching Server".into(),
            error: format!("{message} (Watching Server)"),
            stack: String::new(),
            thread: thread_name(side),
            side,
            details: system_details(side),
            log: crate::logging::recent_lines(),
        },
    )
}

fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Box<dyn Any>".into())
}

fn panic_message(info: &std::panic::PanicHookInfo) -> String {
    let text = payload_text(info.payload());
    match info.location() {
        Some(at) => format!("panicked at {at}:\n{text}"),
        None => format!("panicked:\n{text}"),
    }
}

pub fn stack_lines(backtrace: &str, from_panic: bool) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    let root = manifest
        .strip_suffix("crates/vise-client-render")
        .unwrap_or(&manifest);
    let mut frames: Vec<(String, Option<String>)> = Vec::new();
    for line in backtrace.lines() {
        let line = line.trim();
        if let Some(at) = line.strip_prefix("at ") {
            if let Some(frame) = frames.last_mut() {
                frame.1 = Some(short_path(&at.replace('\\', "/"), root).to_owned());
            }
        } else if let Some((n, name)) = line.split_once(": ")
            && n.chars().all(|c| c.is_ascii_digit())
        {
            frames.push((name.to_owned(), None));
        }
    }
    let machinery = |name: &str| {
        [
            "std::panicking::",
            "core::panicking::",
            "std::sys::backtrace::",
            "__rustc::",
        ]
        .iter()
        .any(|p| name.starts_with(p))
            || name.contains("rust_begin_unwind")
    };
    let top = frames.iter().take(30).position(|(name, _)| machinery(name));
    let first = match top.filter(|_| from_panic) {
        Some(start) => {
            start
                + frames[start..]
                    .iter()
                    .take_while(|(name, _)| machinery(name))
                    .count()
        }
        None => 0,
    };
    let first = first
        + frames[first..]
            .iter()
            .take_while(|(name, _)| name.trim_start_matches('<').starts_with("std::backtrace"))
            .count();
    frames[first..]
        .iter()
        .map(|(name, at)| match at {
            Some(at) => format!("{name} ({at})\n"),
            None => format!("{name}\n"),
        })
        .collect()
}

fn short_path<'a>(path: &'a str, root: &str) -> &'a str {
    if let Some(rest) = path.strip_prefix(root) {
        return rest;
    }
    if let Some(i) = path.find("/registry/src/") {
        let rest = &path[i + "/registry/src/".len()..];
        if let Some((_index, krate)) = rest.split_once('/') {
            return krate;
        }
    }
    path
}

pub fn panic_report(info: &std::panic::PanicHookInfo, side: &'static str) -> Report {
    Report {
        time: Local::now(),
        description: "Unexpected error".into(),
        error: panic_message(info),
        stack: stack_lines(
            &std::backtrace::Backtrace::force_capture().to_string(),
            true,
        ),
        thread: thread_name(side),
        side,
        details: system_details(side),
        log: crate::logging::recent_lines(),
    }
}

pub fn install(home: PathBuf, side: &'static str) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if contained_panic(info) {
            return;
        }
        default(info);
        let main = std::thread::current().name() == Some("main");
        if main {
            deliver(&home, panic_report(info, side));
        } else {
            log::error!(
                "Caught previously unhandled exception in thread {}: {}",
                thread_name(side),
                panic_message(info)
            );
        }
    }));
}

#[derive(Debug)]
pub struct DebugCrash {
    stack: String,
}

impl DebugCrash {
    pub const MESSAGE: &str = "Manually triggered debug crash";
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            stack: stack_lines(
                &std::backtrace::Backtrace::force_capture().to_string(),
                false,
            ),
        }
    }
}

impl std::fmt::Display for DebugCrash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(Self::MESSAGE)
    }
}

impl std::error::Error for DebugCrash {}

pub fn report_error(
    home: &Path,
    side: &'static str,
    error: &(dyn std::error::Error + 'static),
) -> Option<PathBuf> {
    if let Some(debug) = error.downcast_ref::<DebugCrash>() {
        return deliver(
            home,
            Report {
                time: Local::now(),
                description: DebugCrash::MESSAGE.into(),
                error: DebugCrash::MESSAGE.into(),
                stack: debug.stack.clone(),
                thread: thread_name(side),
                side,
                details: system_details(side),
                log: crate::logging::recent_lines(),
            },
        );
    }
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(e) = source {
        let _ = write!(message, "\nCaused by: {e}");
        source = e.source();
    }
    deliver(
        home,
        Report {
            time: Local::now(),
            description: "Fatal error".into(),
            error: message,
            stack: String::new(),
            thread: thread_name(side),
            side,
            details: system_details(side),
            log: crate::logging::recent_lines(),
        },
    )
}

pub fn debug_crash_tick() {
    static AT: std::sync::OnceLock<Option<(std::time::Instant, std::time::Duration)>> =
        std::sync::OnceLock::new();
    if let Some((first, after)) =
        AT.get_or_init(|| debug_crash_after().map(|d| (std::time::Instant::now(), d)))
        && first.elapsed() >= *after
    {
        panic!("Manually triggered debug crash");
    }
}

pub fn debug_crash_after() -> Option<std::time::Duration> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let value = std::env::var("VISE_DEBUG_CRASH").ok()?;
    let seconds = value.parse::<f64>().unwrap_or(5.0).max(0.0);
    Some(std::time::Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_reports_its_built_in_version() {
        if let Some(label) = vise_core::release::nightly_label() {
            assert_eq!(super::version(), label);
            assert_eq!(release_tag(), vise_core::release::nightly_version());
            return;
        }
        match vise_core::release::RELEASE {
            Some(version) => {
                assert_eq!(release_tag(), Some(format!("v{version}")));
                assert_eq!(super::version(), format!("v{version}"));
            }
            None => assert!(release_tag().is_none() || super::version().contains("(client ")),
        }
    }

    fn report() -> Report {
        Report {
            time: chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 10, 14, 9, 5, 7).unwrap(),
            description: "Unexpected error".into(),
            error: "panicked at src/game.rs:10:5:\nboom".into(),
            stack: "vise_client::game::frame (src/game.rs:10:5)\n".into(),
            thread: "Render thread".into(),
            side: "client",
            details: vec![
                ("Vise Version".into(), "0.1.0".into()),
                ("Minecraft Version".into(), "26.3".into()),
                ("Graphics".into(), "Test GPU (Vulkan), driver 1.0".into()),
            ],
            log: vec!["[09:05:06] [Render thread/INFO]: load assets: 1.20s".into()],
        }
    }

    #[test]
    fn file_names_are_vanillas() {
        assert_eq!(
            file_name(&report().time, "client"),
            "crash-2026-10-14_09.05.07-client.txt"
        );
    }

    #[test]
    fn the_report_reads_like_vanillas() {
        let text = report().text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "---- Vise Crash Report ----");
        assert!(lines[1].starts_with("// "));
        assert_eq!(lines[3], "Time: 2026-10-14 09:05:07");
        assert_eq!(lines[4], "Description: Unexpected error");
        assert_eq!(lines[6], "panicked at src/game.rs:10:5:");
        assert_eq!(lines[7], "boom");
        assert_eq!(
            lines[8],
            "\tat vise_client::game::frame (src/game.rs:10:5)"
        );
        assert!(text.contains("-- Head --\nThread: Render thread\n"));
        assert!(text.contains(
            "-- System Details --\nDetails:\n\tVise Version: 0.1.0\n\tMinecraft Version: 26.3\n\tGraphics: Test GPU (Vulkan), driver 1.0\n"
        ));
        assert!(text.ends_with(
            "-- Last 1 log lines --\n[09:05:06] [Render thread/INFO]: load assets: 1.20s\n"
        ));
    }

    #[test]
    fn a_dedicated_servers_report_names_its_server_thread_and_no_render_settings() {
        let unnamed = |side: &'static str| {
            std::thread::spawn(move || thread_name(side))
                .join()
                .unwrap()
        };
        assert_eq!(unnamed("server"), "Server thread");
        assert_eq!(unnamed("client"), "Render thread");
        set_detail("Render Settings", "render distance 12");
        let has = |side| {
            system_details(side)
                .iter()
                .any(|(k, _)| k == "Render Settings")
        };
        assert!(has("client"));
        assert!(!has("server"));
        clear_detail("Render Settings");
    }

    #[test]
    fn stacks_start_at_the_panicking_frame() {
        let bt = format!(
            "   0: vise_client::crash::panic_report\n             at {root}/src/crash.rs:296:16\n   1: std::panicking::panic_with_hook\n             at /rustc/abc/library/std/src/panicking.rs:823:13\n   2: core::panicking::panic_fmt\n             at /rustc/abc/library/core/src/panicking.rs:80:14\n   3: vise_client::crash::debug_crash_tick\n             at {root}/src/crash.rs:359:9\n   4: winit::dispatch\n             at /home/someone/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/winit-0.30.13/src/event_loop.rs:642:56\n   5: main\n",
            root = env!("CARGO_MANIFEST_DIR")
        );
        assert_eq!(
            stack_lines(&bt, true),
            "vise_client::crash::debug_crash_tick (crates/vise-client-render/src/crash.rs:359:9)\nwinit::dispatch (winit-0.30.13/src/event_loop.rs:642:56)\nmain\n"
        );
    }

    #[test]
    fn contained_panics_come_back_as_errors() {
        assert_eq!(contain(|| 7), Ok(7));
        let error = contain(|| -> u8 { panic!("a pack's program") }).unwrap_err();
        assert!(error.contains("a pack's program"), "{error}");
        let outer = contain(|| contain(|| -> u8 { panic!("inner") }));
        assert!(outer.unwrap().unwrap_err().contains("inner"));
        assert_eq!(CONTAINED.with(Cell::get), 0);
    }

    #[test]
    fn the_debug_crash_is_reported_as_vanillas() {
        let error: Box<dyn std::error::Error> = DebugCrash::new().into();
        assert_eq!(error.to_string(), "Manually triggered debug crash");
        let debug = error.downcast_ref::<DebugCrash>().unwrap();
        let first = debug.stack.lines().next().unwrap();
        assert!(first.contains("crash.rs"), "{}", debug.stack);
    }

    #[test]
    fn a_panic_becomes_a_report() {
        let caught = std::sync::Arc::new(Mutex::new(None));
        let sink = caught.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if panic_message(info).contains("the test crash") {
                *sink.lock().unwrap() = Some(panic_report(info, "client"));
            }
        }));
        let _ = std::panic::catch_unwind(|| panic!("the test crash"));
        std::panic::set_hook(previous);
        let report = caught.lock().unwrap().take().expect("the hook ran");
        assert!(report.error.contains("the test crash"));
        assert!(report.error.contains("crash.rs"));
        let first = report.stack.lines().next().unwrap();
        assert!(first.contains("crash.rs"), "{}", report.stack);
        let text = report.text();
        assert!(text.contains("Description: Unexpected error"));
        assert!(text.contains(&format!(
            "\tMinecraft Version: {}\n",
            vise_core::VANILLA_VERSION
        )));
        assert!(text.contains("\tOperating System: "));
        assert!(text.contains("\tCPU: "));
        let home = std::env::temp_dir().join(format!("vise-crash-save-{}", std::process::id()));
        let path = report.save(&home).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("-client.txt")
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
