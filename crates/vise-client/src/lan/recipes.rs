use super::data::SyncData;
use vise_protocol::buf::Writer;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const PROPERTY_SETS: &[(&str, &str, &[&str])] = &[
    ("smithing_addition", "smithing", &["addition"]),
    ("smithing_base", "smithing", &["base"]),
    ("smithing_template", "smithing", &["template"]),
    ("furnace_input", "minecraft:smelting", &["ingredient"]),
    ("blast_furnace_input", "minecraft:blasting", &["ingredient"]),
    ("smoker_input", "minecraft:smoking", &["ingredient"]),
    (
        "campfire_input",
        "minecraft:campfire_cooking",
        &["ingredient"],
    ),
    ("brewing_input", "minecraft:brewing", &["input"]),
    ("brewing_reagent", "minecraft:brewing", &["reagent"]),
];

enum Ingredient {
    Tag(String),
    Items(Vec<String>),
}

fn ingredient(v: &Value) -> Option<Ingredient> {
    match v {
        Value::String(s) => Some(match s.strip_prefix('#') {
            Some(tag) => Ingredient::Tag(tag.to_owned()),
            None => Ingredient::Items(vec![s.clone()]),
        }),
        Value::Array(a) => Some(Ingredient::Items(
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect(),
        )),
        Value::Object(o) => ingredient(o.get("item")?),
        _ => None,
    }
}

pub(super) fn recipes(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Value>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            recipes(&path, &format!("{prefix}{name}/"), out);
        } else if let Some(stem) = name.strip_suffix(".json")
            && let Ok(v) = serde_json::from_slice(&std::fs::read(&path).unwrap_or_default())
        {
            out.insert(format!("minecraft:{prefix}{stem}"), v);
        }
    }
}

pub(super) fn overlay_recipes(
    overlay: &vise_world::loot::PackOverlay,
    out: &mut BTreeMap<String, Value>,
) {
    for (id, files) in overlay.kind("recipe") {
        if let Ok(v) = serde_json::from_slice::<Value>(vise_world::loot::PackOverlay::top(files))
        {
            out.insert(id.to_owned(), v);
        }
    }
}

pub fn update_recipes(
    resources: &Path,
    sync: &SyncData,
    overlay: &vise_world::loot::PackOverlay,
) -> Vec<u8> {
    let mut all = BTreeMap::new();
    recipes(&resources.join("data/minecraft/recipe"), "", &mut all);
    overlay_recipes(overlay, &mut all);
    let tags: BTreeMap<&str, &[i32]> = sync
        .tags
        .registries
        .iter()
        .filter(|r| r.registry == "minecraft:item")
        .flat_map(|r| r.tags.iter())
        .map(|t| (t.name.trim_start_matches("minecraft:"), t.ids.as_slice()))
        .collect();
    let item = |name: &str| sync.id("minecraft:item", name).unwrap_or(0);
    let items = |i: &Ingredient| -> Vec<i32> {
        match i {
            Ingredient::Tag(t) => tags
                .get(t.trim_start_matches("minecraft:"))
                .map(|ids| ids.to_vec())
                .unwrap_or_default(),
            Ingredient::Items(list) => list.iter().map(|n| item(n)).collect(),
        }
    };
    let mut w = Writer::new();
    w.varint(PROPERTY_SETS.len() as i32);
    for (key, kind, fields) in PROPERTY_SETS {
        let mut set = BTreeSet::new();
        for recipe in all.values() {
            let t = recipe["type"].as_str().unwrap_or("");
            let matches = if *kind == "smithing" {
                t.starts_with("minecraft:smithing_")
            } else {
                t == *kind
            };
            if matches {
                for field in *fields {
                    if let Some(i) = ingredient(&recipe[*field]) {
                        set.extend(items(&i));
                    }
                }
            }
        }
        w.string(&format!("minecraft:{key}"));
        w.varint(set.len() as i32);
        for id in set {
            w.varint(id);
        }
    }
    let stonecutting: Vec<&Value> = all
        .values()
        .filter(|r| r["type"] == "minecraft:stonecutting")
        .collect();
    let item_stack = sync
        .id("minecraft:slot_display", "minecraft:item_stack")
        .unwrap_or(0);
    w.varint(stonecutting.len() as i32);
    for r in stonecutting {
        match ingredient(&r["ingredient"]) {
            Some(Ingredient::Tag(t)) => {
                w.varint(0);
                w.string(&t);
            }
            Some(i) => {
                let ids = items(&i);
                w.varint(ids.len() as i32 + 1);
                for id in ids {
                    w.varint(id);
                }
            }
            None => w.varint(1),
        }
        w.varint(item_stack);
        w.varint(item(r["result"]["id"].as_str().unwrap_or("minecraft:air")));
        w.varint(r["result"]["count"].as_i64().unwrap_or(1) as i32);
        w.varint(0);
        w.varint(0);
    }
    w.data
}
