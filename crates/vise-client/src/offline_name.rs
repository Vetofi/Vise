use std::{fs, path::Path};

pub const FILE: &str = "vise-player.txt";
const KEY: &str = "name:";

pub fn is_valid(name: &str) -> bool {
    crate::lan::conn::valid_name(name)
}

pub fn generate(seed: u64) -> String {
    format!("Player{:05}", seed % 100_000)
}

pub fn load_or_create(dir: &Path) -> String {
    let path = dir.join(FILE);
    if let Some(name) = fs::read_to_string(&path).ok().and_then(|text| parse(&text)) {
        return name;
    }
    let name = generate(seed());
    if let Err(e) = fs::write(&path, format!("{KEY}{name}\n")) {
        log::warn!(
            "Vise: could not keep the player name in {}: {e}",
            path.display()
        );
    }
    name
}

fn parse(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim_end_matches('\r').strip_prefix(KEY))
        .filter(|name| is_valid(name))
        .map(str::to_owned)
}

fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut x = nanos ^ u64::from(std::process::id()).rotate_left(32);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^ (x >> 33)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "vise-offline-name-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn generated_names_are_valid_and_never_the_hosts_profile() {
        for seed in [0, 7, 99_999, 100_000, u64::MAX] {
            let name = generate(seed);
            assert!(is_valid(&name), "{name}");
            assert_ne!(name, "Player");
            assert_eq!(name.len(), 11);
        }
        assert_eq!(generate(42), "Player00042");
    }

    #[test]
    fn the_name_is_kept_and_reused() {
        let d = dir("kept");
        let first = load_or_create(&d);
        assert_eq!(
            fs::read_to_string(d.join(FILE)).unwrap(),
            format!("name:{first}\n")
        );
        assert_eq!(load_or_create(&d), first);
        fs::write(d.join(FILE), "name:Alex_2\r\n").unwrap();
        assert_eq!(load_or_create(&d), "Alex_2");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn invalid_names_are_replaced() {
        for text in [
            "",
            "name:",
            "name:has space",
            "name:ThisNameIsTooLong17",
            "name:été",
        ] {
            let d = dir("invalid");
            fs::write(d.join(FILE), text).unwrap();
            let name = load_or_create(&d);
            assert!(
                name.starts_with("Player") && is_valid(&name),
                "{text:?} -> {name}"
            );
            assert_eq!(load_or_create(&d), name);
            let _ = fs::remove_dir_all(&d);
        }
    }
}
