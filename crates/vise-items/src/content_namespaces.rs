use std::{fs, path::Path};

pub const MARKER: &str = "vise-mod-content.json";

pub fn namespaces(root: &Path) -> Vec<String> {
    fs::read(root.join(MARKER))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn tag_namespaces(root: &Path) -> Vec<String> {
    let mods = namespaces(root);
    let mut out = vec!["minecraft".to_owned()];
    if !mods.is_empty() {
        out.push("c".to_owned());
        out.extend(mods);
    }
    out
}
