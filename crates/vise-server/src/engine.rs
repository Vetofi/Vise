
use crate::config::Config;
use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

const PROGRAMS: [&str; 2] = ["vise-engine", "vise-client"];

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

pub fn program(config: &Config, dir: &Path) -> Result<PathBuf, String> {
    program_in(config, dir, crate::update::Install::find().as_ref())
}

pub fn program_in(
    config: &Config,
    dir: &Path,
    install: Option<&crate::update::Install>,
) -> Result<PathBuf, String> {
    if !config.engine.path.is_empty() {
        let path = dir.join(&config.engine.path);
        return path
            .is_file()
            .then_some(path)
            .ok_or_else(|| format!("engine.path {} is not a file", config.engine.path));
    }
    let beside = install.map(crate::update::Install::programs).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
    });
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    PROGRAMS
        .iter()
        .flat_map(|name| {
            beside
                .iter()
                .chain(&path_dirs)
                .map(move |d| d.join(exe(name)))
        })
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "the game program (vise-client) is neither beside vise-server nor on the PATH: \
             install it there, or set engine.path in server.toml"
                .into()
        })
}

pub fn check_starts(program: &Path) -> Result<(), String> {
    let mut command = crate::process::command(program);
    let output = command
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("the game ({}) can't start: {e}", program.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(start_failure(program, output.status.code(), &stderr))
}

fn start_failure(program: &Path, code: Option<i32>, stderr: &str) -> String {
    let reason = stderr
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut text = format!(
        "the game ({}) can't start{}{}",
        program.display(),
        code.map_or(String::new(), |c| format!(" (exit status {c})")),
        if reason.is_empty() {
            String::new()
        } else {
            format!(": {reason}")
        }
    );
    if reason.contains("error while loading shared libraries")
        && (reason.contains("libasound") || reason.contains("libudev"))
    {
        text.push_str(
            "\nThe full game needs ALSA's and udev's libraries (libasound.so.2, libudev.so.1). Install the \
             server build, which needs neither (packaging/server/install.sh picks it on Linux), or the \
             libraries:\n  Debian, Ubuntu: sudo apt install libasound2 libudev1   (Ubuntu 24.04 and later: \
             libasound2t64)\n  Fedora, RHEL: sudo dnf install alsa-lib systemd-libs\n  Arch: sudo pacman -S \
             alsa-lib systemd-libs\n  NixOS: enable programs.nix-ld with alsa-lib and udev in its libraries",
        );
    }
    text
}

pub fn resource_candidates(config: &Config, dir: &Path, program: Option<&Path>) -> Vec<PathBuf> {
    if !config.engine.resources.is_empty() {
        return vec![dir.join(&config.engine.resources)];
    }
    let version = "26.3";
    let mut out = vec![dir.join("resources")];
    if let Some(program) = program {
        for ancestor in program.ancestors().skip(1).take(4) {
            out.push(ancestor.join(".local/vise-client").join(version));
        }
    }
    out
}

pub fn resources(config: &Config, dir: &Path, program: Option<&Path>) -> Result<PathBuf, String> {
    let candidates = resource_candidates(config, dir, program);
    candidates
        .iter()
        .find(|d| d.join("vise-resources.json").is_file())
        .cloned()
        .ok_or_else(|| {
            let tried: Vec<String> = candidates
                .iter()
                .map(|d| format!("  {}", d.display()))
                .collect();
            format!(
                "no prepared game files. Vise reads them from your own copy of Minecraft 26.3:\n\
                 prepare them with `vise-prepare --prepare --server-jar PATH` (or the launcher) and copy \
                 the folder to resources/ here, or set engine.resources in server.toml. Looked in:\n{}",
                tried.join("\n")
            )
        })
}

pub fn spawn(program: &Path, resources: &Path, dir: &Path) -> std::io::Result<Child> {
    let dir = std::path::absolute(dir)?;
    let resources = std::path::absolute(resources)?;
    let mut command = Command::new(program);
    command
        .arg("--resources")
        .arg(resources)
        .arg("--control-socket")
        .arg(control_path(&dir))
        .args(["--dedicated", "--nogui"])
        .current_dir(&dir)
        .env("VISE_NO_AUDIO", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP
            | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
    );
    command.spawn()
}

pub fn control_path(dir: &Path) -> PathBuf {
    dir.join(".vise/engine.json")
}

pub struct Control {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Control {
    pub fn connect(dir: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(control_path(dir))?;
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a control endpoint");
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| bad())?;
        let port = v["port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(bad)?;
        let token = v["token"].as_str().ok_or_else(bad)?;
        let writer = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
        writer.set_read_timeout(Some(Duration::from_secs(30)))?;
        let mut control = Self {
            reader: BufReader::new(writer.try_clone()?),
            writer,
        };
        let reply = control.ask(&format!("auth {token}"))?;
        if reply["ok"] != true {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the game refused the token",
            ));
        }
        Ok(control)
    }

    pub fn ask(&mut self, line: &str) -> std::io::Result<serde_json::Value> {
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        let mut reply = String::new();
        if self.reader.read_line(&mut reply)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        serde_json::from_str(&reply)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources_are_searched_in_order() {
        let mut config = Config::default();
        let dir = Path::new("/srv/mc");
        let program = Path::new("/opt/vise/target/release/vise-client");
        let found = resource_candidates(&config, dir, Some(program));
        assert_eq!(found[0], Path::new("/srv/mc/resources"));
        assert!(found.contains(&PathBuf::from("/opt/vise/.local/vise-client/26.3")));
        config.engine.resources = "/data/res".into();
        assert_eq!(
            resource_candidates(&config, dir, Some(program)),
            [PathBuf::from("/data/res")]
        );
    }

    #[test]
    fn resources_prepared_without_sounds_are_found() {
        let root = std::env::temp_dir().join(format!("vise-server-res-{}", std::process::id()));
        let prepared = root.join(".local/vise-client/26.3");
        std::fs::create_dir_all(&prepared).unwrap();
        let program = root.join("vise-client");
        let config = Config::default();
        let server = root.join("server");
        assert!(resources(&config, &server, Some(&program)).is_err());
        std::fs::write(prepared.join("vise-resources.json"), "{}").unwrap();
        assert_eq!(resources(&config, &server, Some(&program)), Ok(prepared));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_library_says_what_to_install() {
        let program = Path::new("/opt/vise/vise-client");
        let text = start_failure(
            program,
            Some(127),
            "/opt/vise/vise-client: error while loading shared libraries: libasound.so.2: cannot open \
             shared object file: No such file or directory\n",
        );
        assert!(text.starts_with(
            "the game (/opt/vise/vise-client) can't start (exit status 127): /opt/vise/vise-client: \
             error while loading shared libraries: libasound.so.2"
        ));
        assert!(text.contains("sudo apt install libasound2 libudev1"));
        let engine = start_failure(
            Path::new("/opt/vise/vise-engine"),
            Some(127),
            "/opt/vise/vise-engine: error while loading shared libraries: libgcc_s.so.1: cannot open \
             shared object file: No such file or directory\n",
        );
        assert!(!engine.contains("libasound"), "{engine}");
        let other = start_failure(program, Some(1), "something else\n");
        assert_eq!(
            other,
            "the game (/opt/vise/vise-client) can't start (exit status 1): something else"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_folder_reaches_the_game_whole() {
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from(format!("reldir-test-{}", std::process::id()));
        let server = root.join("srv");
        std::fs::create_dir_all(server.join(".vise")).unwrap();
        std::fs::create_dir_all(root.join("res")).unwrap();
        let game = root.join("game");
        std::fs::write(
            &game,
            "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --resources) r=$2; shift;; \
             --control-socket) c=$2; shift;; esac; shift; done\n\
             [ -d \"$r\" ] && [ -d \"$(dirname \"$c\")\" ] && [ -f server.toml ]\n",
        )
        .unwrap();
        std::fs::set_permissions(&game, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(server.join("server.toml"), "").unwrap();
        let mut tries = 0;
        let status = loop {
            match spawn(
                &std::path::absolute(&game).unwrap(),
                &root.join("res"),
                &server,
            ) {
                Err(e) if e.raw_os_error() == Some(26) && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                child => break child.unwrap().wait(),
            }
        };
        std::fs::remove_dir_all(&root).unwrap();
        let status = status.unwrap();
        assert!(status.success(), "{status}");
    }

    #[cfg(unix)]
    fn check_written(program: &Path) -> Result<(), String> {
        for _ in 0..50 {
            match check_starts(program) {
                Err(e) if e.contains("Text file busy") => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                other => return other,
            }
        }
        check_starts(program)
    }

    #[cfg(unix)]
    #[test]
    fn the_check_runs_the_program() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("vise-server-check-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let script = |name: &str, body: &str| {
            let path = root.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        assert_eq!(check_written(&script("starts", "exit 0")), Ok(()));
        let fails = script("fails", "echo 'no luck' >&2; exit 3");
        assert_eq!(
            check_written(&fails),
            Err(format!(
                "the game ({}) can't start (exit status 3): no luck",
                fails.display()
            ))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
