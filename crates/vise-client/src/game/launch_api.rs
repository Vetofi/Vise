
pub const VERSION: &str = "0.1.0";

pub struct LaunchOption {
    pub name: &'static str,
    pub value: Option<&'static str>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub what: &'static str,
}

const fn option(
    name: &'static str,
    value: Option<&'static str>,
    what: &'static str,
) -> LaunchOption {
    LaunchOption { name, value, what }
}

pub const OPTIONS: &[LaunchOption] = &[
    option(
        "--resources",
        Some("DIR"),
        "The prepared resources (vise-prepare's output). Default: `resources` beside the executable, else `.local/vise-client/26.3`.",
    ),
    option(
        "--game-dir",
        Some("DIR"),
        "The game folder: `saves/`, `options.txt`, `resourcepacks/`, `vise-mods/`, `screenshots/`. Created when missing. Default: the working directory. `--gameDir` is the same.",
    ),
    option(
        "--home",
        Some("DIR"),
        "Logs (`logs/latest.log`) and crash reports (`crash-reports/`). Default: `VISE_HOME`, else `vise` in the platform data folder.",
    ),
    option(
        "--width",
        Some("N"),
        "The window's starting width (default 1280).",
    ),
    option(
        "--height",
        Some("N"),
        "The window's starting height (default 720).",
    ),
    option(
        "--fullscreen",
        None,
        "Opens the window fullscreen, whatever `options.txt` says; `options.txt` is not changed.",
    ),
    option(
        "--session-file",
        Some("PATH"),
        "The signed-in account, as the session JSON below. Read once, then deleted.",
    ),
    option(
        "--session-stdin",
        None,
        "The same JSON on standard input, so it never touches the disk.",
    ),
    option(
        "--quickPlaySingleplayer",
        Some("WORLD"),
        "Opens this world of `saves/` at start, only if it exists (else the title screen). WORLD is one folder name: a path is refused.",
    ),
    option(
        "--quickPlayMultiplayer",
        Some("HOST[:PORT]"),
        "Joins this server at start.",
    ),
    option(
        "--launch-api",
        None,
        "Prints the reply below and exits 0, before anything else runs.",
    ),
];

pub mod exit_code {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const PANIC: i32 = 101;
}

const EXIT_CODES: &[(i32, &str)] = &[
    (
        exit_code::OK,
        "The player quit, or `--help` or `--launch-api` printed.",
    ),
    (
        exit_code::ERROR,
        "The game stopped on an error. A crash report was saved.",
    ),
    (
        exit_code::USAGE,
        "The command line was refused: an unknown option, a missing or bad value. No crash report.",
    ),
    (
        exit_code::PANIC,
        "The game panicked. A crash report was saved.",
    ),
];

#[derive(Debug)]
pub struct UsageError(pub String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

pub fn reply(vise: &str) -> String {
    let options: Vec<_> = OPTIONS
        .iter()
        .map(|o| serde_json::json!({ "name": o.name, "value": o.value }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "launch_api": VERSION,
        "vise": vise,
        "minecraft": vise_core::VANILLA_VERSION,
        "options": options,
        "exit_codes": EXIT_CODES.iter().map(|(code, _)| code).collect::<Vec<_>>(),
        "crash_report_line": vise_client::crash::SAVED_LINE,
    }))
    .expect("a JSON value")
}

#[cfg(test)]
pub fn markdown() -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let w = &mut out;
    let _ = writeln!(
        w,
        "<!-- Generated from crates/vise-client/src/game/launch_api.rs; a test fails when this file is out of date. Update it with: VISE_WRITE_DOCS=1 cargo test -p vise-client --bin vise-client launch_api -->"
    );
    let _ = writeln!(w, "# Launch API reference\n");
    let _ = writeln!(
        w,
        "What `vise-client` promises a launcher that starts it, Launch API {VERSION}. The guide: \
         [launch.md](launch.md); stability and versions: [stability.md](stability.md).\n"
    );
    let _ = writeln!(w, "## Options\n");
    let _ = writeln!(
        w,
        "Any other option is refused with exit code 2, so ask `--launch-api` before passing one \
         a build may not know.\n"
    );
    let _ = writeln!(w, "| Option | What it does |\n| --- | --- |");
    for o in OPTIONS {
        let usage = match o.value {
            Some(value) => format!("`{} {value}`", o.name),
            None => format!("`{}`", o.name),
        };
        let _ = writeln!(w, "| {usage} | {} |", o.what);
    }
    let _ = writeln!(w, "\n## `--launch-api`\n");
    let _ = writeln!(
        w,
        "Prints one JSON object on stdout and exits 0. `vise` is the build's version; readers \
         ignore fields they don't know.\n"
    );
    let _ = writeln!(w, "```json\n{}\n```\n", reply("<version>"));
    let _ = writeln!(w, "## Exit codes\n\n| Code | Meaning |\n| --- | --- |");
    for (code, what) in EXIT_CODES {
        let _ = writeln!(w, "| {code} | {what} |");
    }
    let _ = writeln!(
        w,
        "\nWhen a crash report is saved, the last line on stderr is `{}` followed by the \
         report's path.",
        vise_client::crash::SAVED_LINE.trim_end()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> vise_client::Result<Option<super::super::Options>> {
        super::super::Options::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn launch_api_options_are_accepted() {
        for o in OPTIONS {
            let args: Vec<&str> = std::iter::once(o.name)
                .chain(o.value.map(|v| if v == "N" { "100" } else { "x" }))
                .collect();
            assert!(parse(&args).is_ok(), "{} is refused", o.name);
        }
    }

    #[test]
    fn launch_api_flags_are_read() {
        let o = parse(&["--fullscreen", "--game-dir", "g", "--session-stdin"])
            .unwrap()
            .unwrap();
        assert!(o.fullscreen && o.session_stdin);
        assert_eq!(o.game_dir.as_deref(), Some(std::path::Path::new("g")));
        assert!(parse(&["--launch-api"]).unwrap().is_none());
        assert!(parse(&["--no-such-option"]).is_err());
    }

    #[test]
    fn launch_api_bad_value_names_its_option() {
        let error = |args: &[&str]| parse(args).err().unwrap().to_string();
        assert_eq!(
            error(&["--width", "wide"]),
            "--width: invalid digit found in string"
        );
        assert_eq!(error(&["--bogus", "x"]), "--bogus: unknown option");
    }

    #[test]
    fn launch_api_quick_play_is_one_folder_of_saves() {
        for name in ["../x", "a/b", "/abs", ".", ".."] {
            assert!(
                parse(&["--quickPlaySingleplayer", name]).is_err(),
                "{name} is accepted"
            );
        }
        assert!(parse(&["--quickPlaySingleplayer", "E2E World"]).is_ok());
    }

    #[test]
    fn the_launch_reference_lists_every_option() {
        let text = markdown();
        for o in OPTIONS {
            assert!(text.contains(o.name), "{}", o.name);
        }
    }
}
