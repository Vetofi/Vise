use vise_client::block_entity_item::place;
use vise_client::block_entity_nbt::canonical;
use vise_client::containers::{default_name_key, display_name, double_chest_name};
use vise_client::save::text;
use vise_world::nbt::{Tag, parse_snbt};
use serde_json::Value;
use std::fs;

fn canon(tag: &Tag, out: &mut String) {
    match tag {
        Tag::Compound(map) => {
            let mut keys: Vec<_> = map.iter().map(|(k, v)| (k.to_string_lossy(), v)).collect();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (k, v)) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&k);
                out.push(':');
                canon(v, out);
            }
            out.push('}');
        }
        Tag::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&vise_client::item_data::canon_tag(other)),
    }
}

fn snbt(value: &Value) -> Option<Tag> {
    parse_snbt(value.as_str()?).ok().map(|(tag, _)| tag)
}

fn custom_name(value: &Value) -> Option<Option<String>> {
    let saved = canonical(&snbt(value)?)?;
    Some(text(saved.get("CustomName")))
}

fn case(line: &str) -> Option<String> {
    let case: Value = serde_json::from_str(line).ok()?;
    let title = if let Some(first) = case.get("first") {
        let first = custom_name(first)?;
        let second = custom_name(&case["second"])?;
        double_chest_name(first.as_deref(), second.as_deref())
    } else {
        let saved = canonical(&snbt(&case["nbt"])?)?;
        let placed = match case.get("patch") {
            Some(p) => match snbt(p)? {
                Tag::Compound(patch) => place(&saved, &patch, true),
                _ => return None,
            },
            None => saved,
        };
        let key = default_name_key(case["block"].as_str()?)?;
        display_name(text(placed.get("CustomName")).as_deref(), key)
    };
    let mut out = String::new();
    canon(&title, &mut out);
    Some(out)
}

pub fn main() -> vise_client::Result<()> {
    let args: Vec<String> = crate::args().collect();
    let input = fs::read_to_string(
        args.get(1)
            .ok_or("usage: container_title_fixture INPUT OUTPUT")?,
    )?;
    let lines: Vec<String> = input
        .lines()
        .map(|line| case(line).unwrap_or_else(|| "error".to_owned()))
        .collect();
    fs::write(
        args.get(2).ok_or("missing OUTPUT")?,
        lines.join("\n") + "\n",
    )?;
    Ok(())
}
