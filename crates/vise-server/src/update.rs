
use crate::{api::Client, backup, config::Config, engine, supervisor};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const REPO: &str = "vise-rust/vise";
const ASSETS: &[&str] = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    &[
        "vise-server-linux-x86_64.tar.gz",
        "vise-linux-x86_64.tar.gz",
    ]
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    &["vise-macos-arm64.zip"]
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    &["vise-windows-x64.zip"]
} else {
    &[]
};

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub root: PathBuf,
}

impl Install {
    pub fn find() -> Option<Self> {
        Self::around(&std::env::current_exe().ok()?)
    }

    pub fn around(program: &Path) -> Option<Self> {
        let program = std::fs::canonicalize(program).ok()?;
        let releases = program.parent()?.parent()?;
        if releases.file_name()? != "releases" {
            return None;
        }
        let root = releases.parent()?.to_path_buf();
        root.join("current").is_dir().then_some(Self { root })
    }

    pub fn required() -> Result<Self, String> {
        Self::find().ok_or_else(|| {
            "this vise-server wasn't installed by install.sh or install.ps1 (no releases/ and current \
             around it): update it the way you installed it"
                .into()
        })
    }

    pub fn release(&self, tag: &str) -> PathBuf {
        self.root.join("releases").join(tag)
    }

    pub fn programs(&self) -> PathBuf {
        self.root.join("current")
    }

    pub fn current(&self) -> Option<String> {
        let target = std::fs::canonicalize(self.root.join("current")).ok()?;
        Some(target.file_name()?.to_string_lossy().into_owned())
    }

    pub fn previous(&self) -> Option<String> {
        let tag = std::fs::read_to_string(self.root.join("previous")).ok()?;
        let tag = tag.trim();
        (!tag.is_empty() && self.release(tag).is_dir()).then(|| tag.to_owned())
    }

    pub fn switch(&self, tag: &str) -> Result<(), String> {
        let target = self.release(tag);
        if !target.join(exe("vise-server")).is_file() {
            return Err(format!("{} holds no vise-server", target.display()));
        }
        if let Some(old) = self.current().filter(|old| old != tag) {
            std::fs::write(self.root.join("previous"), format!("{old}\n"))
                .map_err(|e| format!("{}: {e}", self.root.join("previous").display()))?;
        }
        point(&self.root.join("current"), &target)
            .map_err(|e| format!("switching current to {tag}: {e}"))
    }

    pub fn switch_back(&self, from: &str, previous: Option<&str>) -> Result<(), String> {
        point(&self.root.join("current"), &self.release(from))
            .map_err(|e| format!("switching current back to {from}: {e}"))?;
        let file = self.root.join("previous");
        match previous {
            Some(tag) => std::fs::write(&file, format!("{tag}\n")),
            None => std::fs::remove_file(&file).or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            }),
        }
        .map_err(|e| format!("{}: {e}", file.display()))
    }

    pub fn prune(&self, keep: u32) -> (Vec<String>, Vec<String>) {
        let releases = self.root.join("releases");
        let running = std::env::current_exe()
            .ok()
            .and_then(|e| std::fs::canonicalize(e).ok());
        let Some(current) = self.current() else {
            return (Vec::new(), Vec::new());
        };
        let previous = self.previous();
        let mut others: Vec<String> = std::fs::read_dir(&releases)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|tag| {
                !tag.starts_with('.') && *tag != current && previous.as_deref() != Some(tag)
            })
            .filter(|tag| {
                let dir = std::fs::canonicalize(releases.join(tag)).ok();
                !running
                    .as_ref()
                    .zip(dir)
                    .is_some_and(|(exe, dir)| exe.starts_with(dir))
            })
            .collect();
        others.sort_by(|a, b| {
            if newer(a, b) {
                std::cmp::Ordering::Less
            } else if newer(b, a) {
                std::cmp::Ordering::Greater
            } else {
                b.cmp(a)
            }
        });
        let (mut removed, mut failed) = (Vec::new(), Vec::new());
        for tag in others.into_iter().skip(keep as usize) {
            match std::fs::remove_dir_all(releases.join(&tag)) {
                Ok(()) => removed.push(tag),
                Err(e) => failed.push(format!("{tag} ({e})")),
            }
        }
        (removed, failed)
    }
}

pub fn pruned(removed: &[String], failed: &[String]) -> String {
    let mut parts = Vec::new();
    if !removed.is_empty() {
        parts.push(format!("Deleted old releases: {}", removed.join(", ")));
    }
    if !failed.is_empty() {
        parts.push(format!(
            "Couldn't delete old releases: {}",
            failed.join(", ")
        ));
    }
    parts.join(". ")
}

#[cfg(unix)]
fn point(link: &Path, target: &Path) -> std::io::Result<()> {
    let fresh = link.with_extension("switching");
    let _ = std::fs::remove_file(&fresh);
    std::os::unix::fs::symlink(target, &fresh)?;
    std::fs::rename(&fresh, link)
}

#[cfg(windows)]
fn point(link: &Path, target: &Path) -> std::io::Result<()> {
    if link.exists() || link.symlink_metadata().is_ok() {
        std::fs::remove_dir(link)?;
    }
    let status = crate::process::command("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "mklink /J failed ({status})"
        )))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub asset: String,
    pub url: String,
    pub sha256: String,
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    ureq::get(url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "vise-server")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => "404".to_string(),
            e => format!("GitHub didn't answer: {e}"),
        })?
        .into_reader()
        .take(64 << 20)
        .read_to_end(&mut body)
        .map_err(|e| format!("GitHub's answer: {e}"))?;
    Ok(body)
}

pub fn release(tag: Option<&str>) -> Result<Release, String> {
    let api = |tag: &str| format!("https://api.github.com/repos/{REPO}/releases/tags/{tag}");
    let text = match tag {
        Some(tag) => fetch(&api(tag)),
        None => match fetch(&api(vise_release_sign::NIGHTLY)) {
            Err(e) if e == "404" => fetch(&format!(
                "https://api.github.com/repos/{REPO}/releases/latest"
            )),
            found => found,
        },
    }
    .map_err(|e| match e.as_str() {
        "404" => format!("no release {} on GitHub", tag.unwrap_or("")),
        _ => e,
    })?;
    let json = serde_json::from_slice(&text).map_err(|e| format!("GitHub's answer: {e}"))?;
    pick(&json, fetch, &vise_release_sign::trusted_keys())
}

fn pick(
    release: &Value,
    fetch: impl Fn(&str) -> Result<Vec<u8>, String>,
    keys: &[vise_release_sign::PublicKey],
) -> Result<Release, String> {
    let tag = release["tag_name"]
        .as_str()
        .ok_or("GitHub's answer has no tag")?;
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let asset = ASSETS
        .iter()
        .find_map(|name| assets.iter().find(|a| a["name"] == *name))
        .ok_or_else(|| {
            format!(
                "release {tag} has no archive for this computer ({})",
                ASSETS.join(", ")
            )
        })?;
    let name = asset["name"].as_str().unwrap_or_default();
    if let Some(hex) = vise_release_sign::legacy_digest(tag, name) {
        return Ok(Release {
            tag: tag.to_owned(),
            asset: name.to_owned(),
            url: asset["browser_download_url"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            sha256: hex.to_owned(),
        });
    }
    let attached = |file: &str| -> Result<Option<Vec<u8>>, String> {
        match assets.iter().find(|a| a["name"] == file) {
            Some(a) => fetch(a["browser_download_url"].as_str().unwrap_or_default())
                .map(Some)
                .map_err(|e| format!("{file} of {tag}: {e}")),
            None => Ok(None),
        }
    };
    let sums = attached(vise_release_sign::SUMS)?;
    let signature = attached(vise_release_sign::SIGNATURE)?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let (tag, sha256) = if vise_release_sign::is_nightly(tag) {
        let version = vise_release_sign::nightly_version(
            release["name"].as_str().unwrap_or_default(),
            release["published_at"].as_str().unwrap_or_default(),
        );
        let sha256 = vise_release_sign::nightly_digest(
            name,
            sums.as_deref(),
            signature.as_deref(),
            keys,
        )?;
        (version, sha256)
    } else {
        let sha256 = vise_release_sign::signed_digest(
            tag,
            name,
            sums.as_deref(),
            signature.as_deref(),
            keys,
        )?;
        (tag.to_owned(), sha256)
    };
    Ok(Release {
        tag,
        asset: name.to_owned(),
        url: asset["browser_download_url"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        sha256,
    })
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TagOrder {
    Dated(Vec<u64>),
    Semver([u64; 3], Prerelease),
    Nightly(String),
}

type Prerelease = (bool, Vec<Identifier>);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Word(String),
}

const FIRST_YEAR: u64 = 1000;

fn tag_order(tag: &str) -> Option<TagOrder> {
    if let Some(date) = vise_release_sign::nightly_date(tag) {
        return Some(TagOrder::Nightly(date.to_owned()));
    }
    let rest = tag.strip_prefix('v')?;
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let numbers: Vec<u64> = core
        .split('.')
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    match (numbers.as_slice(), pre) {
        (&[major, minor, patch], pre) if major < FIRST_YEAR => {
            let pre = match pre {
                None => (true, Vec::new()),
                Some(pre) => (
                    false,
                    pre.split('.')
                        .map(|id| {
                            if id.is_empty() {
                                None
                            } else {
                                Some(id.parse().map_or_else(
                                    |_| Identifier::Word(id.to_owned()),
                                    Identifier::Number,
                                ))
                            }
                        })
                        .collect::<Option<_>>()?,
                ),
            };
            Some(TagOrder::Semver([major, minor, patch], pre))
        }
        (_, None) => Some(TagOrder::Dated(numbers)),
        (_, Some(_)) => None,
    }
}

pub fn newer(tag: &str, than: &str) -> bool {
    match (tag_order(tag), tag_order(than)) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

pub fn update_available(tag: &str, current: &str) -> bool {
    tag != current && tag_order(tag).is_some() && !newer(current, tag)
}

pub fn download(install: &Install, release: &Release) -> Result<PathBuf, String> {
    use sha2::Digest;
    let folder = install.root.join("downloads");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(&release.asset);
    let part = folder.join(format!("{}.part", release.asset));
    let result = (|| {
        let mut reader = ureq::get(&release.url)
            .set("User-Agent", "vise-server")
            .call()
            .map_err(|e| format!("downloading {}: {e}", release.asset))?
            .into_reader();
        let mut file =
            std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut hasher = sha2::Sha256::new();
        let mut buffer = vec![0; 1 << 16];
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| format!("downloading {}: {e}", release.asset))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            std::io::Write::write_all(&mut file, &buffer[..n]).map_err(|e| e.to_string())?;
        }
        let got: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if got != release.sha256 {
            return Err(format!(
                "{} doesn't match its SHA-256 (got {got}, expected {}): not installed",
                release.asset, release.sha256
            ));
        }
        std::fs::rename(&part, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result.map(|()| path)
}

pub fn unpack(install: &Install, archive: &Path, tag: &str) -> Result<PathBuf, String> {
    if tag.is_empty() || tag.contains(['/', '\\']) || tag.starts_with('.') {
        return Err(format!("{tag:?} isn't a release name"));
    }
    let target = install.release(tag);
    if install.current().as_deref() == Some(tag) {
        return Err(format!("{tag} is the release in use"));
    }
    let scratch = install
        .root
        .join("releases")
        .join(format!(".{tag}.unpacking"));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let result = (|| {
        let name = archive.to_string_lossy();
        if name.ends_with(".zip") {
            let file =
                std::fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
            zip::ZipArchive::new(file)
                .and_then(|mut z| z.extract(&scratch))
                .map_err(|e| format!("{}: {e}", archive.display()))?;
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            let status = crate::process::command("tar")
                .arg("--no-same-owner")
                .arg("-xzf")
                .arg(archive)
                .arg("-C")
                .arg(&scratch)
                .status()
                .map_err(|e| format!("tar: {e}"))?;
            if !status.success() {
                return Err(format!(
                    "tar couldn't unpack {} ({status})",
                    archive.display()
                ));
            }
        } else {
            return Err(format!(
                "{} is neither a .tar.gz nor a .zip",
                archive.display()
            ));
        }
        let source = find_program(&scratch, &exe("vise-server"))
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .ok_or_else(|| format!("{} holds no vise-server", archive.display()))?;
        #[cfg(unix)]
        for name in [
            "vise-server",
            "vise-engine",
            "vise-client",
            "vise-prepare",
        ] {
            use std::os::unix::fs::PermissionsExt;
            let path = source.join(name);
            if path.is_file() {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
        }
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        std::fs::rename(&source, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        let game_files = install.root.join("game-files/vise-client");
        std::fs::create_dir_all(&game_files).map_err(|e| e.to_string())?;
        let local = target.join(".local");
        std::fs::create_dir_all(&local).map_err(|e| e.to_string())?;
        let link = local.join("vise-client");
        if link.symlink_metadata().is_err() {
            point(&link, &game_files).map_err(|e| format!("{}: {e}", link.display()))?;
        }
        Ok(target.clone())
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn find_program(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    while let Some(folder) = queue.pop_front() {
        let mut entries: Vec<_> = std::fs::read_dir(&folder).ok()?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let kind = entry.file_type().ok()?;
            if kind.is_file() && entry.file_name() == name {
                return Some(path);
            }
            if kind.is_dir() {
                queue.push_back(path);
            }
        }
    }
    None
}

pub fn backup_name(tag: &str) -> String {
    let clean: String = tag
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(48)
        .collect();
    format!("before-{clean}")
}

pub fn command(dir: &Path, args: &[String]) -> Result<(), String> {
    let mut check = false;
    let mut as_json = false;
    let mut version = None;
    let mut archive = None;
    let mut timeout = crate::api::UPDATE_TIMEOUT;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--check" => check = true,
            "--json" => as_json = true,
            "--version" => version = Some(value()?),
            "--archive" => archive = Some(PathBuf::from(value()?)),
            "--timeout" => timeout = seconds(&value()?)?,
            other => return Err(format!("update: unknown option {other}")),
        }
    }
    let install = Install::required()?;
    let current = install
        .current()
        .ok_or("the installer's `current` points at no release")?;
    let (tag, archive, downloaded) = match archive {
        Some(_) if check => return Err("--check looks at GitHub's releases: drop --archive".into()),
        Some(archive) => {
            let tag = version
                .ok_or("--archive needs --version NAME: the release name to install it as")?;
            (tag, archive, false)
        }
        None => {
            let release = release(version.as_deref())?;
            if check && as_json {
                println!(
                    "{}",
                    json!({
                        "installed": current,
                        "newest": release.tag,
                        "available": update_available(&release.tag, &current),
                        "previous": install.previous(),
                    })
                );
                return Ok(());
            }
            if check {
                if update_available(&release.tag, &current) {
                    println!(
                        "Vise {} is out; this server runs {current}. `vise-server update` installs it.",
                        release.tag
                    );
                } else {
                    println!(
                        "Up to date: {current} (the newest release is {}).",
                        release.tag
                    );
                }
                return Ok(());
            }
            if version.is_none() && !update_available(&release.tag, &current) {
                println!("Already on the newest release ({current}).");
                return Ok(());
            }
            println!("Downloading Vise {} ({})...", release.tag, release.asset);
            let archive = download(&install, &release)?;
            (release.tag, archive, true)
        }
    };
    if tag == current {
        println!("Already on {tag}.");
        return Ok(());
    }
    let unpacked = unpack(&install, &archive, &tag);
    if downloaded {
        let _ = std::fs::remove_file(&archive);
    }
    unpacked?;
    switch_to(dir, &install, &tag, timeout)
}

pub fn rollback(dir: &Path, args: &[String]) -> Result<(), String> {
    let timeout = match args {
        [] => crate::api::UPDATE_TIMEOUT,
        [flag, value] if flag == "--timeout" => seconds(value)?,
        _ => return Err("rollback takes only --timeout SECONDS".into()),
    };
    let install = Install::required()?;
    let previous = install.previous().ok_or(
        "no release to go back to: this install hasn't been updated, or the one before is gone",
    )?;
    switch_to(dir, &install, &previous, timeout)
}

fn seconds(text: &str) -> Result<Duration, String> {
    text.parse()
        .ok()
        .filter(|&s| s > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| format!("--timeout needs a number of seconds (got {text:?})"))
}

fn switch_to(dir: &Path, install: &Install, tag: &str, timeout: Duration) -> Result<(), String> {
    let Some(client) = Client::open(dir)? else {
        return switch_stopped(dir, install, tag);
    };
    let finished = |status: &Value| status["update"]["finished"].as_u64().unwrap_or(0);
    let before = finished(&client.call("GET", "/api/v1/status", None)?);
    client
        .call(
            "POST",
            "/api/v1/update",
            Some(&json!({ "tag": tag, "timeout": timeout.as_secs() })),
        )
        .map_err(|e| {
            if e == "no such endpoint" {
                "the running server is older than updates: stop it, then run `vise-server update` again".into()
            } else {
                e
            }
        })?;
    print!("Restarting the server on {tag}");
    let deadline = Instant::now() + timeout + Duration::from_secs(3600);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let status = client.call("GET", "/api/v1/status", None)?;
        if finished(&status) > before {
            let update = &status["update"];
            let last = update["last"].as_str().unwrap_or_default().to_owned();
            if update["ok"] == true {
                println!("\n{last}");
                return Ok(());
            }
            let mut phase = status["phase"].as_str().unwrap_or_default().to_owned();
            let until = Instant::now() + crate::api::UPDATE_TIMEOUT;
            while phase != "running" && phase != "stopping" && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(500));
                print!(".");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                phase = client.call("GET", "/api/v1/status", None)?["phase"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
            }
            println!();
            return Err(if phase == "running" {
                format!("{last}. The game is up again.")
            } else {
                last
            });
        }
        if Instant::now() > deadline {
            println!();
            return Err(
                "no word from the server about the update; `vise-server logs` shows where it is"
                    .into(),
            );
        }
        print!(".");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

fn switch_stopped(dir: &Path, install: &Install, tag: &str) -> Result<(), String> {
    let from = install
        .current()
        .ok_or("the installer's `current` points at no release")?;
    let server = dir.join(crate::config::FILE).is_file();
    let _lock = if server {
        Some(supervisor::lock(dir)?)
    } else {
        None
    };
    let config = if server {
        Config::load(dir)?
    } else {
        Config::default()
    };
    let backed_up = if server {
        let file = backup::file_name(std::time::SystemTime::now(), Some(&backup_name(tag)))?;
        backup::create(dir, &config, &file)?;
        format!(" The world is backed up in {}/{file}.", backup::DIR)
    } else {
        String::new()
    };
    let previous = install.previous();
    install.switch(tag)?;
    if let Err(e) =
        engine::program_in(&config, dir, Some(install)).and_then(|p| engine::check_starts(&p))
    {
        install.switch_back(&from, previous.as_deref())?;
        return Err(format!("stayed on {from}: {e}"));
    }
    println!(
        "Switched from {from} to {tag}.{backed_up}{} `vise-server rollback` goes back to {from}.",
        if server {
            " The server is stopped: `vise-server start` runs the new version."
        } else {
            ""
        }
    );
    let (removed, failed) = install.prune(config.updates.keep);
    let pruned = pruned(&removed, &failed);
    if !pruned.is_empty() {
        println!("{pruned}.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vise-server-update-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn installed(name: &str, tags: &[&str]) -> Install {
        let root = std::fs::canonicalize(scratch(name)).unwrap();
        for tag in tags {
            let release = root.join("releases").join(tag);
            std::fs::create_dir_all(&release).unwrap();
            std::fs::write(release.join("vise-server"), tag).unwrap();
        }
        std::os::unix::fs::symlink(root.join("releases").join(tags[0]), root.join("current"))
            .unwrap();
        Install { root }
    }

    #[test]
    fn tags_compare_by_number() {
        assert!(newer("v2026.10.19", "v2026.10.18.27"));
        assert!(newer("v2026.10.18.27", "v2026.10.18.9"));
        assert!(newer("v2026.10.19.1", "v2026.10.19"));
        assert!(!newer("v2026.10.19", "v2026.10.19"));
        assert!(!newer("v2026.10.18", "v2026.10.19"));
        assert!(newer("v2026.10.19", "local"));
        assert!(!newer("local", "v2026.10.19"));
    }

    #[test]
    fn semver_tags_follow_every_date_tag() {
        assert!(newer("v0.2.0", "v2026.10.19.15"));
        assert!(newer("v0.2.0", "v2026.10.21"));
        assert!(!newer("v2026.10.19.15", "v0.2.0"));
        assert!(!newer("v2099.12.31.99", "v0.2.0"));
        assert!(newer("v0.2.1", "v0.2.0"));
        assert!(!newer("v0.2.0", "v0.2.1"));
        assert!(newer("v0.2.10", "v0.2.9"));
        assert!(newer("v0.3.0", "v0.2.57"));
        assert!(newer("v1.0.0", "v0.99.0"));
        assert!(!newer("v0.2.0", "v0.2.0"));
        assert!(newer("v0.2.0", "local"));
        assert!(!newer("local", "v0.2.0"));
    }

    #[test]
    fn prereleases_come_before_their_release() {
        assert!(newer("v0.3.0", "v0.3.0-rc.1"));
        assert!(newer("v0.3.0-rc.1", "v0.2.9"));
        assert!(newer("v0.3.0-beta.2", "v0.3.0-beta.1"));
        assert!(newer("v0.3.0-beta.11", "v0.3.0-beta.2"));
        assert!(newer("v0.3.0-rc.1", "v0.3.0-beta.9"));
        assert!(newer("v0.3.0-beta.1", "v0.3.0-beta"));
        assert!(newer("v0.3.0-alpha", "v0.3.0-1"));
        assert!(newer("v0.2.1-beta.1", "v2026.10.21"));
        assert!(!newer("v0.2.1-beta.1", "v0.2.1"));
        assert!(newer("v0.2.0", "v2026.10.21-x"));
        assert!(newer("v0.2.0", "v0.3.0-"));
        assert!(newer("v0.2.0", "v0.3.0-rc..1"));
    }

    #[test]
    fn nightlies_follow_every_release() {
        let today = "nightly-2026-10-28-0c02067";
        assert!(newer(today, "v0.2.95"));
        assert!(newer(today, "v2026.10.21"));
        assert!(newer(today, "nightly-2026-10-27-1a2b3c4"));
        assert!(!newer("v0.2.95", today));
        assert!(newer(today, "local"));
        let later = "nightly-2026-10-28-9b772b2";
        assert!(!newer(later, today) && !newer(today, later));
        assert!(update_available(later, today));
        assert!(update_available(today, "v0.2.95"));
        assert!(!update_available(today, today));
        assert!(!update_available("v0.2.95", today));
        assert!(!update_available("nightly-2026-10-27-1a2b3c4", today));
        assert!(update_available("v0.2.96", "v0.2.95"));
        assert!(!update_available("v0.2.94", "v0.2.95"));
    }

    #[test]
    fn a_nightly_is_picked_with_its_channel_signature_and_named_by_its_build() {
        use vise_release_sign::{SigningKey, sha256_hex, sums_file};
        let Some(first) = ASSETS.first() else { return };
        let asset = |name: &str| json!({ "name": name, "browser_download_url": format!("https://example.invalid/{name}") });
        let (key, _) = SigningKey::generate().unwrap();
        let sums = sums_file(&[(first.to_string(), sha256_hex(b"server"))]);
        let serving = |signed_for: &str| {
            let (sums, signature) = (sums.clone(), key.sign(signed_for, sums.as_bytes()));
            move |url: &str| -> Result<Vec<u8>, String> {
                match url.rsplit('/').next() {
                    Some("SHA256SUMS") => Ok(sums.clone().into_bytes()),
                    Some("SHA256SUMS.sig") => Ok(signature.clone().into_bytes()),
                    _ => Err("404".into()),
                }
            }
        };
        let nightly = json!({
            "tag_name": "nightly", "name": "Nightly 2026-10-28 · 0c02067",
            "published_at": "2026-10-28T20:40:00Z",
            "assets": [asset(first), asset("SHA256SUMS"), asset("SHA256SUMS.sig")],
        });
        let picked = pick(&nightly, serving("nightly"), &[key.public()]).unwrap();
        assert_eq!(picked.tag, "nightly-2026-10-28-0c02067");
        assert_eq!(picked.sha256, sha256_hex(b"server"));
        assert!(pick(&nightly, serving("v0.2.95"), &[key.public()]).is_err());
        let mut release = nightly.clone();
        release["tag_name"] = json!("v0.2.95");
        assert!(pick(&release, serving("nightly"), &[key.public()]).is_err());
    }

    #[test]
    fn pruning_orders_dated_and_semver_releases() {
        let mut tags = vec!["v2026.10.19.15", "v0.2.1", "v2026.10.21", "v0.2.0", "local"];
        tags.sort_by(|a, b| {
            if newer(a, b) {
                std::cmp::Ordering::Less
            } else if newer(b, a) {
                std::cmp::Ordering::Greater
            } else {
                b.cmp(a)
            }
        });
        assert_eq!(
            tags,
            ["v0.2.1", "v0.2.0", "v2026.10.21", "v2026.10.19.15", "local"]
        );
    }

    #[test]
    fn vise_server_ships_with_a_release_key() {
        assert!(
            !vise_release_sign::trusted_keys().is_empty(),
            "crates/vise-release-sign/release-keys.txt lists no key"
        );
    }

    #[test]
    fn a_release_from_before_signing_is_picked_with_its_pinned_digest() {
        if let Some(first) = ASSETS.first() {
            let pinned = vise_release_sign::legacy_digest("v2026.10.19", first)
                .expect("v2026.10.19 is from before signing and has every platform's archive");
            let old = json!({ "tag_name": "v2026.10.19", "assets": [{ "name": first, "browser_download_url": "u" }] });
            let nothing = |_: &str| -> Result<Vec<u8>, String> { Err("404".into()) };
            assert_eq!(pick(&old, nothing, &[]).unwrap().sha256, pinned);
        }
    }

    #[test]
    fn the_archive_for_this_computer_is_picked_with_its_signed_digest() {
        use vise_release_sign::{SigningKey, sha256_hex, sums_file};
        let asset = |name: &str| json!({ "name": name, "browser_download_url": format!("https://example.invalid/{name}") });
        let (key, _) = SigningKey::generate().unwrap();
        let listed: Vec<(String, String)> = ASSETS
            .iter()
            .map(|n| (n.to_string(), sha256_hex(n.as_bytes())))
            .collect();
        let sums = sums_file(&listed);
        let signature = key.sign("v2099.01.01", sums.as_bytes());
        let files = |url: &str| -> Result<Vec<u8>, String> {
            match url.rsplit('/').next() {
                Some("SHA256SUMS") => Ok(sums.clone().into_bytes()),
                Some("SHA256SUMS.sig") => Ok(signature.clone().into_bytes()),
                _ => Err("404".into()),
            }
        };
        let names = ASSETS.iter().rev().copied().chain(["other.zip"]);
        let signed = json!({
            "tag_name": "v2099.01.01",
            "assets": names.chain(["SHA256SUMS", "SHA256SUMS.sig"]).map(asset).collect::<Vec<_>>(),
        });
        if let Some(first) = ASSETS.first() {
            let picked = pick(&signed, files, &[key.public()]).unwrap();
            assert_eq!(picked.tag, "v2099.01.01");
            assert_eq!(picked.asset, *first);
            assert_eq!(picked.sha256, sha256_hex(first.as_bytes()));
            let (other, _) = SigningKey::generate().unwrap();
            assert!(pick(&signed, files, &[other.public()]).is_err());
            let unsigned = json!({ "tag_name": "v1", "assets": [asset(first)] });
            assert!(
                pick(&unsigned, files, &[key.public()])
                    .unwrap_err()
                    .contains("not signed")
            );
            let mut replayed = signed.clone();
            replayed["tag_name"] = json!("v2099.01.02");
            assert!(pick(&replayed, files, &[key.public()]).is_err());
        }
        assert!(
            pick(
                &json!({ "tag_name": "v1", "assets": [asset("other.zip")] }),
                files,
                &[]
            )
            .unwrap_err()
            .contains("no archive for this computer")
        );
    }

    #[test]
    fn backup_names_are_valid() {
        let name = backup_name("v2026.10.19");
        assert_eq!(name, "before-v2026-10-19");
        crate::backup::file_name(std::time::SystemTime::now(), Some(&name)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn switching_keeps_the_previous_release() {
        let install = installed("switch", &["v1", "v2"]);
        let exe = install.root.join("current/vise-server");
        assert_eq!(Install::around(&exe), Some(install.clone()));
        assert_eq!(
            Install::around(&install.release("v2").join("vise-server")),
            Some(install.clone())
        );
        assert_eq!(install.current().as_deref(), Some("v1"));
        assert_eq!(install.previous(), None);
        install.switch("v2").unwrap();
        assert_eq!(install.current().as_deref(), Some("v2"));
        assert_eq!(install.previous().as_deref(), Some("v1"));
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "v2");
        install.switch_back("v1", None).unwrap();
        assert_eq!(install.current().as_deref(), Some("v1"));
        assert_eq!(install.previous(), None);
        install.switch("v2").unwrap();
        install.switch("v1").unwrap();
        assert_eq!(install.current().as_deref(), Some("v1"));
        assert_eq!(install.previous().as_deref(), Some("v2"));
        install.switch("v1").unwrap();
        assert_eq!(install.previous().as_deref(), Some("v2"));
        assert!(
            install
                .switch("v3")
                .unwrap_err()
                .contains("no vise-server")
        );
        assert_eq!(install.current().as_deref(), Some("v1"));
        std::fs::remove_dir_all(&install.root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pruning_keeps_current_previous_and_the_newest_others() {
        let tags = [
            "v2026.10.19",
            "v2026.10.1",
            "v2026.10.2",
            "v2026.10.10",
            "v2026.10.18.3",
            "local",
        ];
        let install = installed("prune", &tags);
        std::fs::write(install.root.join("previous"), "v2026.10.1\n").unwrap();
        std::fs::create_dir_all(install.root.join("releases/.v2026.10.20.unpacking")).unwrap();
        std::fs::write(install.root.join("releases/notes.txt"), "").unwrap();
        let left = || {
            let mut names: Vec<String> = std::fs::read_dir(install.root.join("releases"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        let (removed, failed) = install.prune(1);
        assert_eq!(removed, ["v2026.10.10", "v2026.10.2", "local"]);
        assert!(failed.is_empty());
        assert_eq!(
            left(),
            [
                ".v2026.10.20.unpacking",
                "notes.txt",
                "v2026.10.1",
                "v2026.10.18.3",
                "v2026.10.19"
            ]
        );
        assert_eq!(
            pruned(&removed, &failed),
            "Deleted old releases: v2026.10.10, v2026.10.2, local"
        );
        assert_eq!(install.prune(1), (Vec::new(), Vec::new()));
        assert_eq!(install.prune(0).0, ["v2026.10.18.3"]);
        assert_eq!(install.current().as_deref(), Some("v2026.10.19"));
        assert_eq!(install.previous().as_deref(), Some("v2026.10.1"));
        std::fs::remove_file(install.root.join("current")).unwrap();
        assert_eq!(install.prune(0), (Vec::new(), Vec::new()));
        assert!(install.release("v2026.10.19").is_dir());
        std::fs::remove_dir_all(&install.root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_archive_unpacks_beside_the_others_with_the_game_files_linked() {
        let install = installed("unpack", &["v1"]);
        let source = scratch("unpack-source");
        let inner = source.join("vise-v2");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("vise-server"), "v2").unwrap();
        std::fs::write(inner.join("vise-engine"), "engine").unwrap();
        let archive = source.join("vise-server-linux-x86_64.tar.gz");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .arg("vise-v2")
            .status()
            .unwrap();
        assert!(status.success());
        let release = unpack(&install, &archive, "v2").unwrap();
        assert_eq!(release, install.release("v2"));
        assert_eq!(
            std::fs::read_to_string(release.join("vise-engine")).unwrap(),
            "engine"
        );
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(release.join("vise-engine"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        assert_eq!(
            std::fs::canonicalize(release.join(".local/vise-client")).unwrap(),
            std::fs::canonicalize(install.root.join("game-files/vise-client")).unwrap()
        );
        assert_eq!(install.current().as_deref(), Some("v1"));
        assert!(!install.root.join("releases/.v2.unpacking").exists());
        assert!(
            unpack(&install, &archive, "v1")
                .unwrap_err()
                .contains("in use")
        );
        assert!(unpack(&install, &archive, "../x").is_err());
        let empty = source.join("empty.zip");
        zip::ZipWriter::new(std::fs::File::create(&empty).unwrap())
            .finish()
            .unwrap();
        assert!(
            unpack(&install, &empty, "v3")
                .unwrap_err()
                .contains("holds no vise-server")
        );
        assert!(!install.release("v3").exists());
        std::fs::remove_dir_all(&install.root).unwrap();
        std::fs::remove_dir_all(&source).unwrap();
    }
}
