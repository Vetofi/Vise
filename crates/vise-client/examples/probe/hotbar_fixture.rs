use vise_client::{hotbar, item_data::canon_stack, items::Items};
use vise_world::loot::PackOverlay;
use vise_world::nbt::{Tag, parse_snbt};
use serde_json::Value;
use std::path::Path;
use std::{fmt::Write as _, fs};

pub fn main() -> vise_client::Result<()> {
    let args: Vec<String> = crate::args().collect();
    let usage = "usage: hotbar_fixture RESOURCES CASES OUTPUT";
    let items = Items::catalog(
        Path::new(args.first().and(args.get(1)).ok_or(usage)?),
        &PackOverlay::NONE,
    )?;
    let known = |id: &str| items.get(id).is_some();
    let cases = fs::read_to_string(args.get(2).ok_or(usage)?)?;
    let mut out = String::new();
    for line in cases.lines() {
        let case: Value = serde_json::from_str(line)?;
        let rows = match case["snbt"].as_str() {
            Some(text) => match parse_snbt(text) {
                Ok((tag @ Tag::Compound(_), _)) => hotbar::from_tag(tag, &known),
                _ => return Err("case snbt".into()),
            },
            None => (0..hotbar::GROUPS)
                .map(|_| vec![None; hotbar::SIZE])
                .collect(),
        };
        for (i, row) in rows.iter().enumerate() {
            let _ = write!(out, "{i}:");
            for stack in row {
                let _ = write!(out, " {}", canon_stack(stack.as_ref()));
            }
            out.push_str(" | ");
        }
        out.push('\n');
    }
    fs::write(args.get(3).ok_or(usage)?, out)?;
    Ok(())
}
