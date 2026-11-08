use std::process::ExitCode;

macro_rules! probes {
    ($($name:ident,)*) => {
        fn run(name: &str) -> Option<std::process::ExitCode> {
            use std::process::Termination;
            Some(match name {
                $(stringify!($name) => $name::main().report(),)*
                _ => return None,
            })
        }

        fn main() -> std::process::ExitCode {
            dispatch::main(&[$(stringify!($name)),*], run)
        }
    };
}

pub fn args() -> std::vec::IntoIter<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        args.remove(1);
    }
    args.into_iter()
}

pub fn main(names: &[&str], run: fn(&str) -> Option<ExitCode>) -> ExitCode {
    let name = std::env::args().nth(1).unwrap_or_default();
    run(&name).unwrap_or_else(|| {
        eprintln!(
            "usage: world_probe <name> [arguments...]\nprobes: {}",
            names.join(" ")
        );
        ExitCode::from(2)
    })
}
