
use crate::{
    content::{BlockDef, ItemDef, Rarity},
    loader::{ModMetadata, Side},
    registry::Registries,
};
use std::fmt::{self, Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    NotFinite(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite(id) => write!(
                f,
                "{id}: hardness, resistance and friction must be finite numbers"
            ),
        }
    }
}
impl std::error::Error for ExportError {}

pub fn content_files(
    namespace: &str,
    registries: &Registries,
) -> Result<Vec<ContentFile>, ExportError> {
    let mut files = Vec::new();
    for (id, block) in registries
        .blocks
        .iter()
        .filter(|(id, _)| id.namespace() == namespace)
    {
        files.push(ContentFile {
            path: format!("data/{namespace}/vise/block/{}.json", id.path()),
            text: block_json(&id.to_string(), block)?,
        });
    }
    for (id, item) in registries
        .items
        .iter()
        .filter(|(id, _)| id.namespace() == namespace)
    {
        files.push(ContentFile {
            path: format!("data/{namespace}/vise/item/{}.json", id.path()),
            text: item_json(item),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn finite(id: &str, v: Option<f32>) -> Result<(), ExportError> {
    match v {
        Some(v) if !v.is_finite() => Err(ExportError::NotFinite(id.to_owned())),
        _ => Ok(()),
    }
}

fn block_json(id: &str, b: &BlockDef) -> Result<String, ExportError> {
    let s = &b.settings;
    finite(id, s.hardness)?;
    finite(id, s.explosion_resistance)?;
    finite(id, s.friction)?;
    let mut props = Vec::new();
    if let Some(v) = s.hardness {
        props.push(format!("\"hardness\": {v}"));
    }
    if let Some(v) = s.explosion_resistance {
        props.push(format!("\"explosion_resistance\": {v}"));
    }
    if let Some(v) = s.light {
        props.push(format!("\"light\": {v}"));
    }
    if let Some(v) = s.requires_tool {
        props.push(format!("\"requires_tool\": {v}"));
    }
    if let Some(v) = s.friction {
        props.push(format!("\"friction\": {v}"));
    }
    if let Some(v) = &s.sounds {
        props.push(format!("\"sounds\": {}", json_str(&v.to_string())));
    }
    let mut out = String::from("{\n");
    let _ = writeln!(
        out,
        "  \"template\": {},",
        json_str(&b.template.to_string())
    );
    if !props.is_empty() {
        let _ = writeln!(out, "  \"properties\": {{ {} }},", props.join(", "));
    }
    let _ = writeln!(out, "  \"item\": {}", b.item);
    out.push_str("}\n");
    Ok(out)
}

fn item_json(i: &ItemDef) -> String {
    let mut fields = vec![format!(
        "  \"template\": {}",
        json_str(&i.template.to_string())
    )];
    if let Some(v) = i.max_stack_size {
        fields.push(format!("  \"max_stack_size\": {v}"));
    }
    let rarity = match i.rarity {
        Rarity::Common => None,
        Rarity::Uncommon => Some("uncommon"),
        Rarity::Rare => Some("rare"),
        Rarity::Epic => Some("epic"),
    };
    if let Some(r) = rarity {
        fields.push(format!("  \"rarity\": \"{r}\""));
    }
    format!("{{\n{}\n}}\n", fields.join(",\n"))
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_str(s: &str) -> String {
    let mut out = json_str(s);
    out.shrink_to_fit();
    out
}

pub fn manifest_toml(meta: &ModMetadata) -> String {
    let side = match meta.side {
        Side::Both => "both",
        Side::Client => "client",
        Side::Server => "server",
    };
    let authors: Vec<String> = meta.authors.iter().map(|a| toml_str(a)).collect();
    format!(
        "id = {}\nname = {}\nversion = {}\ndescription = {}\nauthors = [{}]\napi = \"0.2\"\nside = \"{side}\"\ncapabilities = [\"registries\"]\n",
        toml_str(&meta.id),
        toml_str(&meta.name),
        toml_str(&meta.version),
        toml_str(&meta.description),
        authors.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::BlockSettings, identifier::Identifier};

    fn regs() -> Registries {
        let mut r = Registries::new();
        let amethyst = Identifier::vanilla("amethyst_block");
        r.blocks
            .register(
                Identifier::of("ruby", "ruby_block"),
                BlockDef::new(
                    amethyst,
                    BlockSettings::new()
                        .hardness(2.0)
                        .light(7)
                        .requires_tool(true),
                ),
            )
            .unwrap();
        r.blocks
            .register(
                Identifier::of("other", "x"),
                BlockDef::new(Identifier::vanilla("stone"), BlockSettings::new()),
            )
            .unwrap();
        r.items
            .register(
                Identifier::of("ruby", "ruby"),
                ItemDef::new(Identifier::vanilla("diamond"))
                    .max_stack_size(16)
                    .rarity(Rarity::Rare),
            )
            .unwrap();
        r
    }

    #[test]
    fn exports_only_the_namespace_in_content_layout() {
        let files = content_files("ruby", &regs()).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "data/ruby/vise/block/ruby_block.json",
                "data/ruby/vise/item/ruby.json"
            ]
        );
        assert_eq!(
            files[0].text,
            "{\n  \"template\": \"minecraft:amethyst_block\",\n  \"properties\": { \"hardness\": 2, \"light\": 7, \"requires_tool\": true },\n  \"item\": true\n}\n"
        );
        assert_eq!(
            files[1].text,
            "{\n  \"template\": \"minecraft:diamond\",\n  \"max_stack_size\": 16,\n  \"rarity\": \"rare\"\n}\n"
        );
    }

    #[test]
    fn refuses_non_finite_numbers() {
        let mut r = Registries::new();
        r.blocks
            .register(
                Identifier::of("m", "b"),
                BlockDef::new(
                    Identifier::vanilla("stone"),
                    BlockSettings::new().hardness(f32::NAN),
                ),
            )
            .unwrap();
        assert_eq!(
            content_files("m", &r),
            Err(ExportError::NotFinite("m:b".into()))
        );
    }

    #[test]
    fn manifest_escapes() {
        let m = ModMetadata::new("ruby", "1.0.0")
            .name("Ru\"by")
            .author("me");
        let t = manifest_toml(&m);
        assert!(t.contains("name = \"Ru\\\"by\""));
        assert!(t.contains("authors = [\"me\"]"));
        assert!(t.contains("capabilities = [\"registries\"]"));
    }
}
