use vise_client::{
    creative::{Creative, Names},
    item_data::canon_stack,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn main() -> vise_client::Result<()> {
    let args: Vec<String> = crate::args().collect();
    let usage =
        "usage: creative_search_fixture RESOURCES CREATIVE_JSON VANILLA_JSON OUTPUT [permissions]";
    let names = Names::load(Path::new(args.get(1).ok_or(usage)?));
    let mut creative = Creative::from_json(&serde_json::from_slice(&fs::read(
        args.get(2).ok_or(usage)?,
    )?)?)?;
    creative.set_permissions(args.get(5).is_some_and(|a| a == "permissions"), &names);
    let vanilla: Value = serde_json::from_slice(&fs::read(args.get(3).ok_or(usage)?)?)?;
    let tabs: Vec<Value> = creative
        .tabs
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "icon": t.icon,
                "row": if t.top { "TOP" } else { "BOTTOM" },
                "column": t.column,
                "items": t.items.iter().map(|s| canon_stack(Some(s))).collect::<Vec<_>>(),
            })
        })
        .collect();
    let searches: Vec<Value> = vanilla["searches"]
        .as_array()
        .ok_or("searches")?
        .iter()
        .map(|s| {
            let query = s["query"].as_str().unwrap_or_default();
            json!({"query": query, "results": creative.search_results(query, &names)})
        })
        .collect();
    let search: Vec<String> = creative
        .search_items()
        .iter()
        .map(|s| canon_stack(Some(s)))
        .collect();
    let out = json!({"tabs": tabs, "search": search, "searches": searches});
    fs::write(args.get(4).ok_or(usage)?, out.to_string())?;
    Ok(())
}
