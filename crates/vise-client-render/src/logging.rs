use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};

use chrono::{DateTime, Local, NaiveDate};

pub const MAX_ARCHIVES_PER_DAY: u32 = 7;
const RECENT: usize = 50;

pub fn data_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let home = env("HOME").or_else(|| env("USERPROFILE"));
    if cfg!(windows) {
        if let Some(d) = env("APPDATA") {
            return d;
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = &home {
            return h.join("Library/Application Support");
        }
    } else if let Some(d) = env("XDG_DATA_HOME") {
        return d;
    } else if let Some(h) = &home {
        return h.join(".local/share");
    }
    std::env::temp_dir()
}

static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_game_home(home: PathBuf) {
    let _ = HOME.set(home);
}

pub fn game_home() -> PathBuf {
    if let Some(home) = HOME.get() {
        return home.clone();
    }
    match std::env::var_os("VISE_HOME").filter(|h| !h.is_empty()) {
        Some(h) => PathBuf::from(h),
        None => data_dir().join("vise"),
    }
}

pub fn logs_dir(home: &Path) -> PathBuf {
    home.join("logs")
}
pub fn crash_reports_dir(home: &Path) -> PathBuf {
    home.join("crash-reports")
}

pub fn archive_name(date: NaiveDate, index: u32) -> String {
    format!("{}-{index}.log.gz", date.format("%Y-%m-%d"))
}

fn archive_index(name: &str, date: NaiveDate) -> Option<u32> {
    let prefix = format!("{}-", date.format("%Y-%m-%d"));
    name.strip_prefix(&prefix)?
        .strip_suffix(".log.gz")?
        .parse()
        .ok()
        .filter(|&i| i > 0)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Rollover {
    pub delete: Option<String>,
    pub renames: Vec<(String, String)>,
    pub archive: String,
}

pub fn rollover(existing: &[String], date: NaiveDate, max: u32) -> Rollover {
    let mut present: Vec<u32> = existing
        .iter()
        .filter_map(|n| archive_index(n, date))
        .collect();
    present.sort_unstable();
    present.dedup();
    let highest = present.last().copied().unwrap_or(0);
    if highest < max {
        return Rollover {
            delete: None,
            renames: Vec::new(),
            archive: archive_name(date, highest + 1),
        };
    }
    let delete = Some(archive_name(date, present[0]));
    let kept = &present[1..];
    let renames = kept
        .iter()
        .enumerate()
        .map(|(i, &from)| (from, i as u32 + 1))
        .filter(|(from, to)| from != to)
        .map(|(from, to)| (archive_name(date, from), archive_name(date, to)))
        .collect();
    Rollover {
        delete,
        renames,
        archive: archive_name(date, kept.len() as u32 + 1),
    }
}

pub fn format_line(
    time: &DateTime<Local>,
    thread: &str,
    level: log::Level,
    message: &str,
) -> String {
    format!(
        "[{}] [{thread}/{level}]: {message}",
        time.format("%H:%M:%S")
    )
}

fn archive(dir: &Path, date: NaiveDate, text: &[u8]) -> io::Result<()> {
    let names: Vec<String> = fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    let plan = rollover(&names, date, MAX_ARCHIVES_PER_DAY);
    if let Some(name) = &plan.delete {
        fs::remove_file(dir.join(name))?;
    }
    for (from, to) in &plan.renames {
        fs::rename(dir.join(from), dir.join(to))?;
    }
    let mut gz = flate2::write::GzEncoder::new(
        File::create(dir.join(&plan.archive))?,
        flate2::Compression::default(),
    );
    gz.write_all(text)?;
    gz.finish()?.sync_all()
}

fn roll(file: &mut File, dir: &Path, date: NaiveDate) -> io::Result<()> {
    let mut text = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut text)?;
    if !text.is_empty() {
        archive(dir, date, &text)?;
    }
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

pub struct Latest {
    pub file: File,
    _lock: File,
}

pub fn open_latest(dir: &Path) -> io::Result<Option<Latest>> {
    fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(".lock"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Ok(None),
        Err(fs::TryLockError::Error(e)) => return Err(e),
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("latest.log"))?;
    let modified = file.metadata()?.modified()?;
    let date = DateTime::<Local>::from(modified).date_naive();
    roll(&mut file, dir, date)?;
    Ok(Some(Latest { file, _lock: lock }))
}

enum Out {
    Write {
        date: NaiveDate,
        lines: Vec<String>,
        stderr: Option<String>,
    },
    Flush(mpsc::SyncSender<()>),
}

struct Writer {
    file: Option<Latest>,
    dir: PathBuf,
    date: NaiveDate,
}

impl Writer {
    fn run(mut self, out: mpsc::Receiver<Out>) {
        let stderr = io::stderr();
        for msg in out {
            match msg {
                Out::Write {
                    date,
                    lines,
                    stderr: message,
                } => {
                    if let Some(message) = message {
                        let _ = writeln!(stderr.lock(), "{message}");
                    }
                    if date != self.date {
                        if let Some(latest) = &mut self.file
                            && let Err(e) = roll(&mut latest.file, &self.dir, self.date)
                        {
                            let _ = writeln!(
                                stderr.lock(),
                                "Vise: could not roll over the log: {e}"
                            );
                        }
                        self.date = date;
                    }
                    if let Some(latest) = &mut self.file {
                        let mut text = lines.join("\n");
                        text.push('\n');
                        let _ = latest.file.write_all(text.as_bytes());
                    }
                }
                Out::Flush(done) => {
                    if let Some(latest) = &mut self.file {
                        let _ = latest.file.flush();
                    }
                    let _ = done.send(());
                }
            }
        }
    }
}

struct Logger {
    out: mpsc::Sender<Out>,
    recent: Mutex<VecDeque<String>>,
    main_thread: &'static str,
}

static LOGGER: std::sync::OnceLock<Logger> = std::sync::OnceLock::new();

fn ours(target: &str) -> bool {
    target.starts_with("vise")
}

impl log::Log for Logger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= log::Level::Info
            && (ours(meta.target()) || meta.level() <= log::Level::Error)
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let current = std::thread::current();
        let thread = match current.name() {
            Some("main") => self.main_thread,
            Some(name) => name,
            None => "unnamed",
        };
        let stderr = ours(record.target()).then(|| message.clone());
        self.send(thread, record.level(), &message, stderr);
    }

    fn flush(&self) {
        let (done, wait) = mpsc::sync_channel(1);
        if self.out.send(Out::Flush(done)).is_ok() {
            let _ = wait.recv_timeout(std::time::Duration::from_secs(1));
        }
    }
}

impl Logger {
    fn write(&self, thread: &str, level: log::Level, message: &str) {
        self.send(thread, level, message, None);
    }

    fn send(&self, thread: &str, level: log::Level, message: &str, stderr: Option<String>) {
        let now = Local::now();
        let lines: Vec<String> = message
            .lines()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    format_line(&now, thread, level, line)
                } else {
                    line.to_owned()
                }
            })
            .collect();
        {
            let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
            for line in &lines {
                if recent.len() == RECENT {
                    recent.pop_front();
                }
                recent.push_back(line.clone());
            }
        }
        let msg = Out::Write {
            date: now.date_naive(),
            lines,
            stderr,
        };
        if let Err(mpsc::SendError(Out::Write {
            stderr: Some(message),
            ..
        })) = self.out.send(msg)
        {
            eprintln!("{message}");
        }
    }
}

pub fn init(home: &Path, main_thread: &'static str) {
    let dir = logs_dir(home);
    let file = match open_latest(&dir) {
        Ok(Some(latest)) => Some(latest),
        Ok(None) => {
            eprintln!(
                "Vise: another game is writing {}; this one logs to the terminal only",
                dir.join("latest.log").display()
            );
            None
        }
        Err(e) => {
            eprintln!(
                "Vise: could not open {}: {e}",
                dir.join("latest.log").display()
            );
            None
        }
    };
    let logger = LOGGER.get_or_init(|| {
        let (out, receive) = mpsc::channel();
        let writer = Writer {
            file,
            dir,
            date: Local::now().date_naive(),
        };
        if let Err(e) = std::thread::Builder::new()
            .name("Log writer".into())
            .spawn(move || writer.run(receive))
        {
            eprintln!("Vise: could not start the log writer: {e}");
        }
        Logger {
            out,
            recent: Mutex::new(VecDeque::new()),
            main_thread,
        }
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

pub fn write_only(level: log::Level, message: &str) {
    if let Some(logger) = LOGGER.get() {
        let current = std::thread::current();
        let thread = match current.name() {
            Some("main") | None => logger.main_thread,
            Some(name) => name,
        };
        logger.write(thread, level, message);
    }
}

pub fn write_as(thread: &str, level: log::Level, message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.write(thread, level, message);
    }
}

pub fn recent_lines() -> Vec<String> {
    LOGGER.get().map_or_else(Vec::new, |l| {
        l.recent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    })
}

pub fn flush() {
    if let Some(logger) = LOGGER.get() {
        log::Log::flush(logger);
    }
}

pub fn tail(path: &Path, lines: usize) -> io::Result<Vec<String>> {
    let text = fs::read(path)?;
    let text = String::from_utf8_lossy(&text);
    let all: Vec<&str> = text.lines().collect();
    Ok(all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| l.to_string())
        .collect())
}

pub fn newest_crash_report(home: &Path) -> Option<PathBuf> {
    fs::read_dir(crash_reports_dir(home))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .max_by_key(|p| p.file_name().map(|n| n.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    #[test]
    fn archives_are_named_like_vanillas() {
        assert_eq!(archive_name(day(4), 1), "2026-10-04-1.log.gz");
        let none: Vec<String> = Vec::new();
        assert_eq!(
            rollover(&none, day(14), 7),
            Rollover {
                delete: None,
                renames: vec![],
                archive: "2026-10-14-1.log.gz".into()
            }
        );
        let names: Vec<String> = [
            "2026-10-13-1.log.gz",
            "2026-10-13-2.log.gz",
            "2026-10-14-1.log.gz",
            "2026-10-14-2.log.gz",
            "latest.log",
            "debug-1.log.gz",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(rollover(&names, day(14), 7).archive, "2026-10-14-3.log.gz");
        assert_eq!(rollover(&names, day(13), 7).archive, "2026-10-13-3.log.gz");
        assert_eq!(rollover(&names, day(15), 7).archive, "2026-10-15-1.log.gz");
    }

    #[test]
    fn the_eighth_archive_of_a_day_drops_the_oldest() {
        let names: Vec<String> = (1..=7).map(|i| archive_name(day(14), i)).collect();
        let plan = rollover(&names, day(14), 7);
        assert_eq!(plan.delete.as_deref(), Some("2026-10-14-1.log.gz"));
        assert_eq!(plan.renames.len(), 6);
        assert_eq!(
            plan.renames[0],
            ("2026-10-14-2.log.gz".into(), "2026-10-14-1.log.gz".into())
        );
        assert_eq!(
            plan.renames[5],
            ("2026-10-14-7.log.gz".into(), "2026-10-14-6.log.gz".into())
        );
        assert_eq!(plan.archive, "2026-10-14-7.log.gz");
    }

    #[test]
    fn lines_look_like_log4js_pattern() {
        let time = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 10, 14, 9, 5, 7).unwrap();
        assert_eq!(
            format_line(
                &time,
                "Render thread",
                log::Level::Info,
                "load assets: 1.20s"
            ),
            "[09:05:07] [Render thread/INFO]: load assets: 1.20s"
        );
        assert_eq!(
            format_line(&time, "Worker-1", log::Level::Warn, "x"),
            "[09:05:07] [Worker-1/WARN]: x"
        );
    }

    #[test]
    fn startup_archives_the_last_runs_log() {
        let dir = std::env::temp_dir().join(format!("vise-logs-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        drop(open_latest(&dir).unwrap().unwrap());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::write(
            dir.join("latest.log"),
            "[10:00:00] [Render thread/INFO]: first run\n",
        )
        .unwrap();
        let mut latest = open_latest(&dir).unwrap().unwrap();
        assert!(open_latest(&dir).unwrap().is_none());
        writeln!(latest.file, "second run").unwrap();
        drop(latest);
        let today = Local::now().date_naive();
        let archive = dir.join(archive_name(today, 1));
        let mut text = String::new();
        flate2::read::GzDecoder::new(File::open(&archive).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "[10:00:00] [Render thread/INFO]: first run\n");
        assert_eq!(
            fs::read_to_string(dir.join("latest.log")).unwrap(),
            "second run\n"
        );
        assert_eq!(
            tail(&dir.join("latest.log"), 100).unwrap(),
            vec!["second run"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_newest_crash_report_is_found_by_name() {
        let home = std::env::temp_dir().join(format!("vise-crash-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        assert!(newest_crash_report(&home).is_none());
        let dir = crash_reports_dir(&home);
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "crash-2026-10-13_23.59.59-client.txt",
            "crash-2026-10-14_08.00.01-client.txt",
            "notes.txt",
        ] {
            fs::write(dir.join(name), "x").unwrap();
        }
        assert_eq!(
            newest_crash_report(&home).unwrap(),
            dir.join("crash-2026-10-14_08.00.01-client.txt")
        );
        let _ = fs::remove_dir_all(&home);
    }
}
