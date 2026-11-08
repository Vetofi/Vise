use vise_port_pack::{fetch, inventory::Record, pipeline, upgrade};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "usage: vise-port-pack import <pack> <work> | inventory <work> [--offline] | fetch <work> [--from <folder>] [<mod id>...] | carry <work> [--resources <prepared>] [<mod id>...] | report <work> | check <jar> <out> | port <pack> <work> [--from <folder>] [--resources <prepared>] | upgrade-datapack <pack> (<out> | --world <world>)";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| {
        let from = option(&mut args, "--from")?;
        let resources = option(&mut args, "--resources")?;
        let world = option(&mut args, "--world")?;
        let offline = take_flag(&mut args, "--offline");
        let path = |i: usize| args.get(i).map(Path::new).ok_or(USAGE.to_owned());
        let rest = || args.get(2..).unwrap_or_default().to_vec();
        let log = &|line: String| println!("{line}");
        match args.first().map(String::as_str) {
            Some("import") => pipeline::import(path(1)?, path(2)?, log).map(drop),
            Some("inventory") => pipeline::inventory(path(1)?, offline, log).map(drop),
            Some("fetch") => {
                let work = path(1)?;
                let records: Vec<Record> = pipeline::read_json(&work.join("inventory.json"))?;
                let only = rest();
                let want = |r: &Record| {
                    if only.is_empty() {
                        pipeline::portable(r)
                    } else {
                        only.contains(&r.key)
                    }
                };
                fetch::fetch(work, &records, from.as_deref(), &want, log).map(drop)
            }
            Some("carry") => {
                pipeline::carry(path(1)?, resources.as_deref(), &rest(), log).map(drop)
            }
            Some("report") => {
                let work = path(1)?;
                pipeline::report(work, log)?;
                println!("{}", work.join("coverage.md").display());
                Ok(())
            }
            Some("check") => {
                let c = vise_port_pack::check::check(path(1)?, path(2)?)?;
                println!("{}: {}. {}", c.mod_id, c.verdict.label(), c.summary);
                for p in &c.ports {
                    println!("  ports: {p}");
                }
                for s in &c.skipped {
                    println!("  skipped: {s}");
                }
                Ok(())
            }
            Some("port") => {
                let work = path(2)?;
                pipeline::import(path(1)?, work, log)?;
                pipeline::inventory(work, offline, log)?;
                pipeline::port(work, resources.as_deref(), from.as_deref(), log)?;
                println!("{}", work.join("coverage.md").display());
                Ok(())
            }
            Some("upgrade-datapack") => {
                let pack = path(1)?;
                let out = match &world {
                    Some(world) => {
                        let name = pack.file_stem().unwrap_or_default().to_string_lossy();
                        world.join("datapacks").join(format!("{name}-26.3"))
                    }
                    None => path(2)?.to_path_buf(),
                };
                let report = upgrade::upgrade_into(pack, &out)?;
                for line in report.lines() {
                    println!("{line}");
                }
                println!("{}", out.display());
                Ok(())
            }
            _ => Err(USAGE.to_owned()),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn option(args: &mut Vec<String>, name: &str) -> Result<Option<PathBuf>, String> {
    match args.iter().position(|a| a == name) {
        Some(i) if i + 1 < args.len() => {
            let value = args.remove(i + 1);
            args.remove(i);
            Ok(Some(PathBuf::from(value)))
        }
        Some(_) => Err(USAGE.to_owned()),
        None => Ok(None),
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != name);
    args.len() != before
}
