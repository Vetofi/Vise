use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub const FILE: &str = "vise-mods.txt";
pub const DIR: &str = "vise-mods";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModList {
    pub enabled: BTreeSet<String>,
    pub disabled: BTreeSet<String>,
    pub keys: BTreeMap<String, BTreeMap<String, String>>,
    pub installed: Vec<Installed>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub id: String,
    pub title: String,
    pub description: String,
    pub problem: Option<String>,
    pub dev: bool,
    pub native: Option<NativePage>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativePage {
    pub keys: Vec<[String; 3]>,
    pub buttons: Vec<String>,
}

impl ModList {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let ids = |value: &str| -> BTreeSet<String> {
            text.lines()
                .filter_map(|l| l.split_once(':'))
                .filter(|(_, v)| v.trim() == value)
                .filter_map(|(k, _)| k.trim().strip_prefix("mod."))
                .map(str::to_owned)
                .collect()
        };
        let (enabled, disabled) = (ids("true"), ids("false"));
        let mut keys: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (k, v) in text.lines().filter_map(|l| l.split_once(':')) {
            if let Some((id, name)) = k
                .trim()
                .strip_prefix("key.")
                .and_then(|r| r.split_once('.'))
            {
                let entry = keys.entry(id.to_owned()).or_default();
                entry.insert(name.to_owned(), v.trim().to_owned());
            }
        }
        Self {
            enabled,
            disabled,
            keys,
            installed: Vec::new(),
        }
    }
    pub fn serialize(&self) -> String {
        let on = self.enabled.iter().map(|id| format!("mod.{id}:true\n"));
        let off = self.disabled.iter().map(|id| format!("mod.{id}:false\n"));
        let keys = self.keys.iter().flat_map(|(id, keys)| {
            keys.iter()
                .map(move |(name, key)| format!("key.{id}.{name}:{key}\n"))
        });
        on.chain(off).chain(keys).collect()
    }
    pub fn keys_of(&self, id: &str) -> Vec<(&str, &str)> {
        self.keys.get(id).map_or_else(Vec::new, |k| {
            k.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect()
        })
    }
    pub fn save(&self, dir: &Path) {
        let path = dir.join(FILE);
        if self.enabled.is_empty()
            && self.disabled.is_empty()
            && self.keys.is_empty()
            && !path.exists()
        {
            return;
        }
        let _ = fs::write(path, self.serialize());
    }
    pub fn is_on(&self, i: usize) -> bool {
        self.installed.get(i).is_some_and(|m| {
            m.dev
                || if m.native.is_some() {
                    !self.disabled.contains(&m.id)
                } else {
                    self.enabled.contains(&m.id)
                }
        })
    }
    pub fn toggle(&mut self, i: usize) {
        let Some(m) = self.installed.get(i).filter(|m| !m.dev) else {
            return;
        };
        if m.native.is_some() {
            if self.disabled.remove(&m.id) {
                self.enabled.insert(m.id.clone());
            } else {
                self.enabled.remove(&m.id);
                self.disabled.insert(m.id.clone());
            }
        } else if !self.enabled.remove(&m.id) {
            self.enabled.insert(m.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_ignores_the_rest() {
        let list = ModList::parse("mod.coords_hud:true\nmod.other:false\nzoom:true\n");
        assert_eq!(list.enabled.iter().collect::<Vec<_>>(), ["coords_hud"]);
        assert_eq!(ModList::parse(&list.serialize()), list);
        assert_eq!(ModList::default().serialize(), "");
    }

    #[test]
    fn keeps_the_keys_players_chose() {
        let text = "mod.zoom:true\nkey.zoom.zoom.in:key.mouse.4\nkey.zoom.out:key.keyboard.x\n";
        let list = ModList::parse(text);
        assert_eq!(
            list.keys_of("zoom"),
            [("out", "key.keyboard.x"), ("zoom.in", "key.mouse.4")]
        );
        assert!(list.keys_of("other").is_empty());
        assert_eq!(ModList::parse(&list.serialize()), list);
    }

    #[test]
    fn toggles_installed_and_native_mods_but_not_dev_ones() {
        let installed = |id: &str, dev| Installed {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            problem: None,
            dev,
            native: None,
        };
        let mut list = ModList {
            installed: vec![installed("a", false), installed("b", true)],
            ..Default::default()
        };
        assert!(!list.is_on(0) && list.is_on(1));
        list.toggle(0);
        list.toggle(1);
        assert!(list.is_on(0) && list.is_on(1));
        assert_eq!(list.serialize(), "mod.a:true\n");
        list.toggle(0);
        assert!(!list.is_on(0));
        list.installed.push(Installed {
            native: Some(NativePage::default()),
            ..installed("c", false)
        });
        assert!(list.is_on(2));
        list.toggle(2);
        assert!(!list.is_on(2));
        assert_eq!(list.serialize(), "mod.c:false\n");
        assert_eq!(ModList::parse(&list.serialize()).disabled, list.disabled);
        list.toggle(2);
        assert!(list.is_on(2));
        assert_eq!(list.serialize(), "mod.c:true\n");
    }
}
