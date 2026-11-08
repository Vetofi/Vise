use super::Game;
use vise_client::{
    mod_list::{self, Installed},
    text::Text,
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ModsState {
    pub(super) dev: Vec<PathBuf>,
    stopped: Vec<String>,
}

pub(super) fn content_root(
    root: &std::path::Path,
    enabled: &std::collections::BTreeSet<String>,
    dev: &[std::path::PathBuf],
) -> std::path::PathBuf {
    use vise_client::mod_content::{self, ContentMod};
    use vise_mods::{Capability, Content, Package};
    let here = std::env::current_dir().unwrap_or_default();
    let mut paths = vise_mods::find(&here.join(mod_list::DIR));
    paths.retain(|p| !dev.contains(p));
    paths.extend(dev.iter().cloned());
    let mut mods: Vec<ContentMod> = Vec::new();
    for path in &paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        let on = enabled.contains(&m.id) || dev.contains(path);
        if !on
            || !m.capabilities.contains(&Capability::Registries)
            || mods.iter().any(|c| c.id == m.id)
        {
            continue;
        }
        let content = Content::read(path, m).and_then(|content| {
            let own = (m.id.as_str(), content.namespaces.as_slice());
            match mods
                .iter()
                .find_map(|c| vise_mods::content::clash((&c.id, &c.content.namespaces), own))
            {
                Some(e) => Err(e),
                None => Ok(content),
            }
        });
        match content {
            Ok(content) => mods.push(ContentMod {
                id: m.id.clone(),
                content,
            }),
            Err(e) => log::warn!("Vise: mod {} content not loaded: {e}", m.id),
        }
    }
    match mod_content::resource_root(root, &mods, &here.join(".vise-cache")) {
        Ok(out) => {
            if out != root {
                log::info!(
                    "Vise: content from {} mod(s) in {}",
                    mods.len(),
                    out.display()
                );
            }
            out
        }
        Err(e) => {
            log::warn!("Vise: content mods not loaded: {e}");
            root.to_owned()
        }
    }
}

const STOPPED: &str = "Doesn't run any more: needs a native port";
const NOTICE_SEEN: &str = ".native-notice";

impl Game<'_> {
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let mut paths = vise_mods::find(&dir);
        paths.retain(|p| !self.mods.dev.contains(p));
        paths.extend(self.mods.dev.iter().cloned());
        self.settings.mods.installed = paths
            .iter()
            .map(|path| {
                let file = path.file_name().unwrap_or_default().to_string_lossy();
                let package = match vise_mods::Package::read(path) {
                    Ok(package) => package,
                    Err(e) => {
                        return Installed {
                            id: String::new(),
                            title: file.into_owned(),
                            description: String::new(),
                            problem: Some(format!("Can't be read: {e}")),
                            dev: false,
                            native: None,
                        };
                    }
                };
                let m = &package.manifest;
                let problem = package.wasm.is_some().then(|| STOPPED.to_owned());
                Installed {
                    id: m.id.clone(),
                    title: format!("{} {}", m.name, m.version),
                    description: m.description.clone(),
                    problem,
                    dev: self.mods.dev.contains(path),
                    native: None,
                }
            })
            .collect();
        self.list_native_mods();
        let installed = &self.settings.mods.installed;
        self.mods.stopped = installed
            .iter()
            .filter(|row| row.problem.as_deref() == Some(STOPPED))
            .map(|row| row.title.clone())
            .collect();
    }
    pub(super) fn tell_native_now(&mut self) {
        if self.mods.stopped.is_empty() || self.player.is_none() {
            return;
        }
        let names = std::mem::take(&mut self.mods.stopped).join(", ");
        let seen = Self::options_path()
            .with_file_name(mod_list::DIR)
            .join(NOTICE_SEEN);
        if seen.exists() {
            return;
        }
        self.system_message(Text::literal(format!(
            "Vise mods are native now: {names} stopped running until they have a native version. The launcher's Mods page has the native ones."
        )));
        let _ = std::fs::write(&seen, "");
    }
}
