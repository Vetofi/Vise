#![forbid(unsafe_code)]
mod game;

use vise_client::{crash, logging};

#[global_allocator]
static ALLOCATOR: vise_threads::Allocator = vise_threads::Allocator;

static MOD_API_LEVEL: [u8; 24] = vise_mods::api_level::MARK;

fn main() {
    vise_threads::configure_allocator();
    std::hint::black_box(&MOD_API_LEVEL);
    vise_client::telemetry::mark_start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let game_args = args.iter().take_while(|a| *a != "--dedicated");
    let help = game_args
        .clone()
        .any(|a| a == "--help" || a == "-h" || a == "--launch-api");
    if let Some(dir) = game_args.clone().skip_while(|a| *a != "--home").nth(1) {
        logging::set_game_home(std::path::absolute(dir).unwrap_or_else(|_| dir.into()));
    }
    let (home, side, thread) = if args.iter().any(|a| a == "--dedicated") {
        let folder = game_args
            .clone()
            .skip_while(|a| *a != "--game-dir" && *a != "--gameDir")
            .nth(1)
            .and_then(|dir| std::path::absolute(dir).ok())
            .unwrap_or_else(|| ".".into());
        (folder, "server", "Server thread")
    } else {
        (logging::game_home(), "client", "Render thread")
    };
    if !help {
        logging::init(&home, thread);
        log::info!("Vise {}", crash::version());
        crash::install(home.clone(), side);
    }
    raise_open_file_limit();
    if let Err(error) = game::run() {
        let usage = error.is::<game::UsageError>();
        log::error!("Vise: {error}");
        logging::flush();
        if !help && !usage {
            crash::report_error(&home, side, &*error);
        }
        #[cfg(windows)]
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!("Press Enter to close.");
            let _ = std::io::stdin().read_line(&mut String::new());
        }
        std::process::exit(if usage {
            game::exit_code::USAGE
        } else {
            game::exit_code::ERROR
        });
    }
    logging::flush();
}

fn raise_open_file_limit() {
    #[cfg(unix)]
    {
        use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
        let limit = getrlimit(Resource::Nofile);
        let hard = if cfg!(target_os = "macos") {
            Some(limit.maximum.map_or(10_240, |h| h.min(10_240)))
        } else {
            limit.maximum
        };
        let raised = match (limit.current, hard) {
            (Some(soft), Some(hard)) => hard > soft,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if raised {
            let to = Rlimit {
                current: hard,
                maximum: limit.maximum,
            };
            match setrlimit(Resource::Nofile, to) {
                Ok(()) => log::info!(
                    "Open file limit raised from {:?} to {hard:?}",
                    limit.current
                ),
                Err(e) => log::warn!("Open file limit stays at {:?}: {e}", limit.current),
            }
        }
    }
}
