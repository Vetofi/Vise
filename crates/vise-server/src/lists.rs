
use crate::{config, supervisor};
use vise_server_files::user_lists::{NameAndId, OpEntry, OpList, WhiteEntry, WhiteList};
use std::path::Path;

pub fn run(dir: &Path, command: &str, args: &[String]) -> Result<(), String> {
    let config = config::Config::load(dir)?;
    let Ok(_lock) = supervisor::lock(dir) else {
        if let ("whitelist", [add, name]) = (command, args)
            && add == "add"
            && !config.server.online_mode
        {
            return add_offline(dir, name);
        }
        let mut words = vec![command.to_owned()];
        words.extend_from_slice(args);
        return crate::cli::command(dir, &words);
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let ops_path = dir.join("ops.json");
    let white_path = dir.join("whitelist.json");
    let message = match (command, args.as_slice()) {
        ("op", [name]) => {
            let mut ops = load(OpList::new(ops_path))?;
            let user = account(name, config.server.online_mode)?;
            if ops.contains(&user) {
                "Nothing changed. The player already is an operator".to_owned()
            } else {
                ops.add(OpEntry {
                    user,
                    level: 4,
                    bypasses_player_limit: false,
                });
                format!("Made {name} a server operator")
            }
        }
        ("deop", [name]) => {
            let mut ops = load(OpList::new(ops_path))?;
            let user = listed(ops.entries().iter().map(|e| &e.user), name);
            if user.is_some_and(|user| ops.remove(&user)) {
                format!("Made {name} no longer a server operator")
            } else {
                "Nothing changed. The player is not an operator".to_owned()
            }
        }
        ("whitelist", ["add", name]) => {
            let mut white = load(WhiteList::new(white_path))?;
            let user = account(name, config.server.online_mode)?;
            if white.contains(&user) {
                "Player is already whitelisted".to_owned()
            } else {
                white.add(WhiteEntry { user });
                format!("Added {name} to the whitelist")
            }
        }
        ("whitelist", ["remove", name]) => {
            let mut white = load(WhiteList::new(white_path))?;
            let user = listed(white.entries().iter().map(|e| &e.user), name);
            if user.is_some_and(|user| white.remove(&user)) {
                format!("Removed {name} from the whitelist")
            } else {
                "Player is not whitelisted".to_owned()
            }
        }
        ("whitelist", [state @ ("on" | "off")]) => {
            let on = *state == "on";
            match (on, config.server.whitelist) {
                (true, true) => "Whitelist is already turned on".to_owned(),
                (false, false) => "Whitelist is already turned off".to_owned(),
                _ => {
                    config::update(dir, "server", "whitelist", toml::Value::Boolean(on))?;
                    format!("Whitelist is now turned {state}")
                }
            }
        }
        ("whitelist", ["list"]) => {
            let white = load(WhiteList::new(white_path))?;
            let names: Vec<String> = white
                .entries()
                .iter()
                .map(|e| e.user.name.clone())
                .collect();
            let state = if config.server.whitelist { "on" } else { "off" };
            if names.is_empty() {
                format!("There are no whitelisted players (the whitelist is {state})")
            } else {
                format!(
                    "There are {} whitelisted player(s): {} (the whitelist is {state})",
                    names.len(),
                    names.join(", ")
                )
            }
        }
        _ => {
            return Err(
                "use: op NAME, deop NAME, whitelist on|off|list, whitelist add|remove NAME".into(),
            );
        }
    };
    println!("{message}");
    Ok(())
}

fn add_offline(dir: &Path, name: &str) -> Result<(), String> {
    if crate::api::Client::open(dir)?.is_none() {
        return Err(format!("no server is running in {}", dir.display()));
    }
    let user = account(name, false)?;
    let mut white = load(WhiteList::new(dir.join("whitelist.json")))?;
    if white.contains(&user) {
        println!("Player is already whitelisted");
        return Ok(());
    }
    white.add(WhiteEntry { user });
    white
        .save()
        .map_err(|e| format!("{}: {e}", white.file.display()))?;
    crate::cli::command(dir, &["whitelist".to_owned(), "reload".to_owned()])?;
    println!("Added {name} to the whitelist");
    Ok(())
}

pub fn account(name: &str, online: bool) -> Result<NameAndId, String> {
    vise_server_files::profiles::resolve(name, online).ok_or_else(|| {
        if online {
            format!("That player does not exist (no Minecraft account is named {name}, or the services didn't answer)")
        } else {
            "That player does not exist".to_owned()
        }
    })
}

fn listed<'a>(mut users: impl Iterator<Item = &'a NameAndId>, name: &str) -> Option<NameAndId> {
    users.find(|u| u.name.eq_ignore_ascii_case(name)).cloned()
}

fn load<E: vise_server_files::user_lists::Entry>(
    mut list: vise_server_files::user_lists::UserList<E>,
) -> Result<vise_server_files::user_lists::UserList<E>, String> {
    if list.file.exists() {
        list.load()
            .map_err(|e| format!("{}: {e}", list.file.display()))?;
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_the_lists_while_stopped() {
        let dir = std::env::temp_dir().join(format!("vise-server-lists-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(config::FILE), config::TEMPLATE).unwrap();
        let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        run(&dir, "op", &s(&["Alex"])).unwrap();
        run(&dir, "whitelist", &s(&["add", "Steve"])).unwrap();
        run(&dir, "whitelist", &s(&["off"])).unwrap();
        assert!(
            std::fs::read_to_string(dir.join("ops.json"))
                .unwrap()
                .contains("Alex")
        );
        assert!(
            std::fs::read_to_string(dir.join("whitelist.json"))
                .unwrap()
                .contains("Steve")
        );
        assert!(!config::Config::load(&dir).unwrap().server.whitelist);
        run(&dir, "deop", &s(&["Alex"])).unwrap();
        assert!(
            !std::fs::read_to_string(dir.join("ops.json"))
                .unwrap()
                .contains("Alex")
        );
        let lock = supervisor::lock(&dir).unwrap();
        assert!(run(&dir, "op", &s(&["Alex"])).is_err());
        config::update(&dir, "server", "online_mode", toml::Value::Boolean(false)).unwrap();
        assert!(run(&dir, "whitelist", &s(&["add", "Kai"])).is_err());
        drop(lock);
        assert!(
            !std::fs::read_to_string(dir.join("ops.json"))
                .unwrap()
                .contains("Alex")
        );
        assert!(
            !std::fs::read_to_string(dir.join("whitelist.json"))
                .unwrap()
                .contains("Kai")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
