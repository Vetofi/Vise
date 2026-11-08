use crate::{
    Error, Result,
    packets::configuration::{RegistryData, UpdateTags},
};
use vise_world::nbt::{JavaString, Tag};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Debug, Default)]
pub struct Ids {
    pub names: Vec<String>,
    pub ids: HashMap<String, i32>,
}
impl Ids {
    pub fn from_names(names: Vec<String>) -> Self {
        let ids = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as i32))
            .collect();
        Self { names, ids }
    }
    pub fn name(&self, id: i32) -> Option<&str> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.names.get(i))
            .map(String::as_str)
    }
    pub fn id(&self, name: &str) -> Option<i32> {
        self.ids.get(name).copied()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Builtin {
    pub registries: HashMap<String, Ids>,
}
impl Builtin {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self> {
        let json: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| Error::new(format!("registries report: {e}")))?;
        let mut registries = HashMap::new();
        for (name, registry) in json
            .as_object()
            .ok_or_else(|| Error::new("registries report is not an object"))?
        {
            let Some(entries) = registry["entries"].as_object() else {
                continue;
            };
            let mut names = vec![String::new(); entries.len()];
            for (entry, value) in entries {
                let id = value["protocol_id"]
                    .as_u64()
                    .ok_or_else(|| Error::new(format!("{entry}: no protocol_id")))?
                    as usize;
                if id >= names.len() {
                    names.resize(id + 1, String::new());
                }
                names[id] = entry.clone();
            }
            registries.insert(name.clone(), Ids::from_names(names));
        }
        Ok(Self { registries })
    }
    pub fn get(&self, registry: &str) -> Option<&Ids> {
        self.registries.get(registry)
    }
    pub fn find(resources: &Path) -> Option<std::path::PathBuf> {
        let candidates = [
            resources.join("registries.json"),
            resources.join("../../datagen/out/reports/registries.json"),
            resources.join("../datagen/out/reports/registries.json"),
        ];
        candidates.into_iter().find(|p| p.is_file())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Dynamic {
    pub registries: HashMap<String, Vec<(String, Option<Tag>)>>,
    pub dialog_tags: HashMap<String, Vec<i32>>,
}
impl Dynamic {
    pub fn add_tags(&mut self, packet: &UpdateTags) {
        for r in packet
            .registries
            .iter()
            .filter(|r| r.registry == "minecraft:dialog")
        {
            self.dialog_tags = r
                .tags
                .iter()
                .map(|t| (t.name.clone(), t.ids.clone()))
                .collect();
        }
    }
    pub fn add(&mut self, packet: RegistryData) {
        let entries = packet.entries.into_iter().map(|e| (e.id, e.data)).collect();
        self.registries.insert(packet.registry, entries);
    }
    pub fn name(&self, registry: &str, id: i32) -> Option<&str> {
        let i = usize::try_from(id).ok()?;
        self.registries
            .get(registry)?
            .get(i)
            .map(|(n, _)| n.as_str())
    }
    pub fn id(&self, registry: &str, name: &str) -> Option<i32> {
        let list = self.registries.get(registry)?;
        list.iter().position(|(n, _)| n == name).map(|i| i as i32)
    }
    pub fn data(&self, registry: &str, id: i32) -> Option<&Tag> {
        let i = usize::try_from(id).ok()?;
        self.registries.get(registry)?.get(i)?.1.as_ref()
    }
    pub fn len(&self, registry: &str) -> usize {
        self.registries.get(registry).map_or(0, Vec::len)
    }
    pub fn is_empty(&self) -> bool {
        self.registries.is_empty()
    }
    pub fn dimension(&self, id: i32) -> DimensionType {
        let name = self
            .name("minecraft:dimension_type", id)
            .unwrap_or("minecraft:overworld");
        let mut d = DimensionType::vanilla(name);
        if let Some(Tag::Compound(map)) = self.data("minecraft:dimension_type", id) {
            let int = |k: &str| match map.get(&JavaString::from(k)) {
                Some(Tag::Int(v)) => Some(*v),
                _ => None,
            };
            if let Some(v) = int("min_y") {
                d.min_y = v;
            }
            if let Some(v) = int("height") {
                d.height = v;
            }
            if let Some(v) = int("logical_height") {
                d.logical_height = v;
            }
            if let Some(Tag::Byte(v)) = map.get(&JavaString::from("has_skylight")) {
                d.has_skylight = *v != 0;
            }
        }
        d
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionType {
    pub name: String,
    pub min_y: i32,
    pub height: i32,
    pub logical_height: i32,
    pub has_skylight: bool,
}
impl DimensionType {
    pub fn vanilla(name: &str) -> Self {
        let (min_y, height, logical_height, has_skylight) = match name {
            "minecraft:the_nether" => (0, 256, 128, false),
            "minecraft:the_end" => (0, 256, 256, true),
            _ => (-64, 384, 384, true),
        };
        Self {
            name: name.to_owned(),
            min_y,
            height,
            logical_height,
            has_skylight,
        }
    }
    pub fn sections(&self) -> usize {
        (self.height / 16) as usize
    }
}

pub fn ceil_log2(n: usize) -> u8 {
    if n <= 1 {
        0
    } else {
        (usize::BITS - (n - 1).leading_zeros()) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::configuration::RegistryEntry;

    #[test]
    fn parses_registry_reports() {
        let report = r#"{"minecraft:item":{"default":"minecraft:air","entries":{"minecraft:stone":{"protocol_id":1},"minecraft:air":{"protocol_id":0}},"protocol_id":5}}"#;
        let b = Builtin::parse(report).unwrap();
        let items = b.get("minecraft:item").unwrap();
        assert_eq!(items.name(1), Some("minecraft:stone"));
        assert_eq!(items.id("minecraft:air"), Some(0));
        assert_eq!(items.name(2), None);
    }

    #[test]
    fn dynamic_registries_follow_arrival_order() {
        let mut d = Dynamic::default();
        d.add(RegistryData {
            registry: "minecraft:dimension_type".into(),
            entries: vec![
                RegistryEntry {
                    id: "minecraft:overworld".into(),
                    data: None,
                },
                RegistryEntry {
                    id: "minecraft:the_nether".into(),
                    data: Some(Tag::compound([
                        ("min_y", Tag::Int(0)),
                        ("height", Tag::Int(256)),
                        ("has_skylight", Tag::Byte(0)),
                    ])),
                },
            ],
        });
        assert_eq!(
            d.dimension(0),
            DimensionType::vanilla("minecraft:overworld")
        );
        assert_eq!(d.dimension(1).sections(), 16);
        assert!(!d.dimension(1).has_skylight);
        assert_eq!(ceil_log2(35723), 16);
        assert_eq!(ceil_log2(65), 7);
        assert_eq!(ceil_log2(64), 6);
    }
}
