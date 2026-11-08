use std::{fs, io, path::Path};

pub const FILE: &str = "vise-extras.txt";
pub const KEY: &str = "noAi";
pub const REFUSAL: &str = "AI features are off: turn off No AI in the launcher's settings";

pub fn parse(text: &str) -> bool {
    text.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(k, v)| k.trim() == KEY && v.trim() == "true")
    })
}

pub fn is_on(home: &Path) -> bool {
    fs::read_to_string(home.join(FILE)).is_ok_and(|text| parse(&text))
}

pub fn with(text: &str, on: bool) -> String {
    let mut out: String = text
        .lines()
        .filter(|line| line.split_once(':').is_none_or(|(k, _)| k.trim() != KEY))
        .flat_map(|line| [line, "\n"])
        .collect();
    out.push_str(&format!("{KEY}:{on}\n"));
    out
}

pub fn set(home: &Path, on: bool) -> io::Result<()> {
    let path = home.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if !on {
                return Ok(());
            }
            String::new()
        }
        Err(e) => return Err(e),
    };
    fs::create_dir_all(home)?;
    fs::write(path, with(&text, on))
}

pub fn check(home: &Path) -> Result<(), &'static str> {
    if is_on(home) { Err(REFUSAL) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_extras_line_keeping_the_others() {
        assert!(!parse(""));
        assert!(!parse("noAi:false\nzoom:true\n"));
        assert!(parse("zoom:true\nnoAi: true\n"));
        assert_eq!(
            with("zoom:true\nnoAi:false\nfps:true", true),
            "zoom:true\nfps:true\nnoAi:true\n"
        );
        assert_eq!(with("", false), "noAi:false\n");
    }

    #[test]
    fn set_writes_only_when_needed() {
        let home = std::env::temp_dir().join(format!("vise-no-ai-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        set(&home, false).unwrap();
        assert!(!home.join(FILE).exists(), "off needs no file");
        assert!(check(&home).is_ok());
        set(&home, true).unwrap();
        assert!(is_on(&home));
        assert_eq!(check(&home), Err(REFUSAL));
        fs::write(home.join(FILE), "fps:true\nnoAi:true\n").unwrap();
        set(&home, false).unwrap();
        assert_eq!(
            fs::read_to_string(home.join(FILE)).unwrap(),
            "fps:true\nnoAi:false\n"
        );
        let _ = fs::remove_dir_all(&home);
    }
}
