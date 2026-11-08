use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct PalettedPermutations {
    pub textures: Vec<String>,
    pub palette_key: String,
    pub permutations: BTreeMap<String, String>,
    pub separator: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permutation<'a> {
    pub base: &'a str,
    pub palette_key: &'a str,
    pub palette: &'a str,
}

fn qualified(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

impl PalettedPermutations {
    pub fn from_atlas(atlas: &str) -> Vec<Self> {
        let Ok(json) = serde_json::from_str::<Value>(atlas) else {
            return Vec::new();
        };
        let sources = json
            .get("sources")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        sources
            .iter()
            .filter(|s| {
                s.get("type")
                    .and_then(Value::as_str)
                    .map(qualified)
                    .as_deref()
                    == Some("minecraft:paletted_permutations")
            })
            .filter_map(|s| {
                Some(Self {
                    textures: s
                        .get("textures")?
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(qualified)
                        .collect(),
                    palette_key: qualified(s.get("palette_key")?.as_str()?),
                    permutations: s
                        .get("permutations")?
                        .as_object()?
                        .iter()
                        .filter_map(|(k, v)| Some((k.clone(), qualified(v.as_str()?))))
                        .collect(),
                    separator: s
                        .get("separator")
                        .and_then(Value::as_str)
                        .unwrap_or("_")
                        .to_owned(),
                })
            })
            .collect()
    }
    pub fn find(&self, sprite: &str) -> Option<Permutation<'_>> {
        let sprite = qualified(sprite);
        self.textures.iter().find_map(|base| {
            let suffix = sprite
                .strip_prefix(base.as_str())?
                .strip_prefix(self.separator.as_str())?;
            Some(Permutation {
                base,
                palette_key: &self.palette_key,
                palette: self.permutations.get(suffix)?,
            })
        })
    }
}

pub fn apply_palette(pixels: &mut [u32], key: &[u32], target: &[u32]) -> Option<()> {
    if key.len() != target.len() {
        return None;
    }
    let mut map = BTreeMap::new();
    for (&k, &t) in key.iter().zip(target) {
        if k >> 24 != 0 {
            map.insert(k | 0xFF00_0000, t);
        }
    }
    for pixel in pixels {
        let alpha = *pixel >> 24;
        if alpha == 0 {
            continue;
        }
        let rgb = *pixel | 0xFF00_0000;
        let target = map.get(&rgb).copied().unwrap_or(rgb);
        *pixel = (alpha * (target >> 24) / 255) << 24 | target & 0x00FF_FFFF;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_trim_permutations_and_maps_colours() {
        let atlas = r#"{"sources":[{"type":"minecraft:paletted_permutations","palette_key":"minecraft:trim_base",
            "permutations":{"quartz":"minecraft:trim/quartz"},"textures":["minecraft:trims/items/boots_trim"]}]}"#;
        let sources = PalettedPermutations::from_atlas(atlas);
        let p = sources[0]
            .find("minecraft:trims/items/boots_trim_quartz")
            .unwrap();
        assert_eq!(
            (p.base, p.palette),
            ("minecraft:trims/items/boots_trim", "minecraft:trim/quartz")
        );
        assert!(
            sources[0]
                .find("minecraft:trims/items/boots_trim_gold")
                .is_none()
        );
        let mut pixels = [0xFF11_2233, 0x8011_2233, 0x0011_2233, 0xFF44_5566];
        apply_palette(&mut pixels, &[0xFF11_2233], &[0xFFAA_BBCC]).unwrap();
        assert_eq!(pixels, [0xFFAA_BBCC, 0x80AA_BBCC, 0x0011_2233, 0xFF44_5566]);
    }
}
