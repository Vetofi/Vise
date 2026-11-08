
use std::path::Path;

pub const DIR: &str = "shaderpacks";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Vise,
    Classic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub file: String,
    pub title: String,
    pub kind: Kind,
}

pub fn scan(dir: &Path) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut packs: Vec<Entry> = entries.flatten().filter_map(|e| entry(&e.path())).collect();
    packs.sort_by(|a, b| {
        (a.kind == Kind::Classic, a.title.to_lowercase())
            .cmp(&(b.kind == Kind::Classic, b.title.to_lowercase()))
    });
    packs
}

fn entry(path: &Path) -> Option<Entry> {
    let file = path.file_name()?.to_str()?.to_owned();
    if file.starts_with('.') {
        return None;
    }
    if vise_shader_api::package::is_pack(path) {
        let manifest = if path.is_dir() {
            std::fs::read_to_string(path.join(vise_shader_api::MANIFEST))
                .ok()
                .and_then(|t| vise_shader_api::Manifest::parse(&t).ok())
        } else {
            vise_shader_api::Pack::read(path)
                .ok()
                .map(|p| p.manifest)
        };
        let title = manifest.map_or_else(|| file.clone(), |m| m.name);
        return Some(Entry {
            file,
            title,
            kind: Kind::Vise,
        });
    }
    let zip = path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    (zip || path.join("shaders").is_dir()).then(|| Entry {
        title: file.clone(),
        file,
        kind: Kind::Classic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_both_kinds_vise_first_and_skips_the_rest() {
        let dir = std::env::temp_dir().join(format!("vise-shader-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.as_path();
        let manifest = "schema = 1\nid = \"warm\"\nname = \"Warm Light\"\nversion = \"1\"\n\
            api = \"0.1\"\n[[post]]\nfile = \"p.wgsl\"\n";
        std::fs::create_dir(d.join("warm")).unwrap();
        std::fs::write(d.join("warm").join(vise_shader_api::MANIFEST), manifest).unwrap();
        std::fs::create_dir_all(d.join("Classic Folder/shaders")).unwrap();
        std::fs::write(d.join("archive.zip"), b"").unwrap();
        std::fs::write(d.join("notes.txt"), b"").unwrap();
        std::fs::create_dir(d.join("empty")).unwrap();
        std::fs::create_dir_all(d.join(".hidden/shaders")).unwrap();
        let packs = scan(d);
        let listed: Vec<(&str, &str, Kind)> = packs
            .iter()
            .map(|e| (e.file.as_str(), e.title.as_str(), e.kind))
            .collect();
        assert_eq!(
            listed,
            [
                ("warm", "Warm Light", Kind::Vise),
                ("archive.zip", "archive.zip", Kind::Classic),
                ("Classic Folder", "Classic Folder", Kind::Classic),
            ]
        );
        assert!(scan(&d.join("missing")).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }
}
