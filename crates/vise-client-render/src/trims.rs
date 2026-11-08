use vise_assets::palette::apply_palette;
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

pub type Image = (u32, u32, Vec<u8>);

fn qualified(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn asset_path(root: &Path, folder: &str, id: &str, suffix: &str) -> std::path::PathBuf {
    let id = qualified(id);
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", &id));
    root.join("assets")
        .join(namespace)
        .join(folder)
        .join(format!("{path}{suffix}"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrimOverride {
    pub material: Option<String>,
    pub pattern: Option<String>,
    pub texture: Option<String>,
    pub palette: Option<String>,
}
impl TrimOverride {
    pub fn list(equipment: &Value) -> Vec<Self> {
        let Some(list) = equipment.get("trim_overrides").and_then(Value::as_array) else {
            return Vec::new();
        };
        let id = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(qualified);
        list.iter()
            .filter_map(|o| {
                let when = o.get("when")?;
                Some(Self {
                    material: id(when, "material"),
                    pattern: id(when, "pattern"),
                    texture: id(o, "texture"),
                    palette: id(o, "palette"),
                })
            })
            .collect()
    }
    fn matches(&self, material: &str, pattern: &str) -> bool {
        self.material.as_deref().is_none_or(|m| m == material)
            && self.pattern.as_deref().is_none_or(|p| p == pattern)
    }
}

#[derive(Clone, Debug, Default)]
pub struct TrimRegistry {
    patterns: HashMap<String, String>,
    materials: HashMap<String, String>,
}
impl TrimRegistry {
    pub fn load(root: &Path) -> Self {
        let mut out = Self::default();
        let Ok(namespaces) = fs::read_dir(root.join("data")) else {
            return out;
        };
        for ns in namespaces.flatten() {
            let namespace = ns.file_name().to_string_lossy().into_owned();
            for (folder, key, map) in [
                ("trim_pattern", "asset_id", &mut out.patterns),
                ("trim_material", "palette_id", &mut out.materials),
            ] {
                let Ok(files) = fs::read_dir(ns.path().join(folder)) else {
                    continue;
                };
                for f in files.flatten() {
                    let path = f.path();
                    if path.extension().is_none_or(|e| e != "json") {
                        continue;
                    }
                    let Some(value) = fs::read(&path)
                        .ok()
                        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    else {
                        continue;
                    };
                    if let Some(v) = value.get(key).and_then(Value::as_str) {
                        let name = path.file_stem().unwrap_or_default().to_string_lossy();
                        map.insert(format!("{namespace}:{name}"), qualified(v));
                    }
                }
            }
        }
        out
    }
    #[cfg(test)]
    fn of(patterns: &[(&str, &str)], materials: &[(&str, &str)]) -> Self {
        let map = |l: &[(&str, &str)]| {
            l.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Self {
            patterns: map(patterns),
            materials: map(materials),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TrimTexture {
    pub texture: String,
    pub palette: Option<String>,
}
impl TrimTexture {
    pub fn resolve(
        registry: &TrimRegistry,
        overrides: &[TrimOverride],
        material: &str,
        pattern: &str,
        layer: &str,
    ) -> Option<Self> {
        let (material, pattern) = (qualified(material), qualified(pattern));
        let mut texture = registry.patterns.get(&pattern)?.clone();
        let mut palette = Some(registry.materials.get(&material)?.clone());
        if let Some(o) = overrides.iter().find(|o| o.matches(&material, &pattern)) {
            if let Some(t) = &o.texture {
                texture.clone_from(t);
            }
            palette.clone_from(&o.palette);
        }
        let (namespace, path) = texture.split_once(':')?;
        Some(Self {
            texture: format!("{namespace}:trims/entity/{layer}/{path}"),
            palette,
        })
    }
    pub fn sprite(&self) -> String {
        format!(
            "vise/trim/{}/{}",
            self.texture,
            self.palette.as_deref().unwrap_or("-")
        )
    }
    pub fn generate(&self, root: &Path) -> Option<Image> {
        let path = asset_path(root, "textures", &self.texture, ".png");
        let (w, h, pixels) = crate::hud::decode(&path).ok()?;
        let Some(palette) = &self.palette else {
            return Some((w, h, pixels));
        };
        let base = fs::read(path.with_extension("png.mcmeta"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|m| {
                m.get("palette")?
                    .get("base_palette")?
                    .as_str()
                    .map(str::to_owned)
            });
        let load = |id: &str| {
            crate::hud::decode(&asset_path(root, "textures/palettes", id, ".png"))
                .ok()
                .map(|(_, _, p)| argb(&p))
        };
        let mut mapped = argb(&pixels);
        if let (Some(key), Some(target)) = (base.as_deref().and_then(load), load(palette)) {
            let _ = apply_palette(&mut mapped, &key, &target);
        }
        let out = mapped
            .iter()
            .flat_map(|p| {
                let [a, r, g, b] = p.to_be_bytes();
                [r, g, b, a]
            })
            .collect();
        Some((w, h, out))
    }
}

fn argb(rgba: &[u8]) -> Vec<u32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
        .collect()
}

pub fn slot_size(root: &Path) -> (u32, u32) {
    let mut size = (64, 32);
    let Ok(namespaces) = fs::read_dir(root.join("assets")) else {
        return size;
    };
    for ns in namespaces.flatten() {
        let Ok(layers) = fs::read_dir(ns.path().join("textures/trims/entity")) else {
            continue;
        };
        for layer in layers.flatten() {
            for f in fs::read_dir(layer.path()).into_iter().flatten().flatten() {
                let path = f.path();
                if path.extension().is_some_and(|e| e == "png")
                    && let Some((w, h)) = png_size(&path)
                {
                    size = (size.0.max(w.min(512)), size.1.max(h.min(512)));
                }
            }
        }
    }
    size
}

fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    let be = |i: usize| u32::from_be_bytes(head[i..i + 4].try_into().unwrap());
    (&head[12..16] == b"IHDR").then(|| (be(16), be(20)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> TrimRegistry {
        TrimRegistry::of(
            &[("minecraft:coast", "minecraft:coast")],
            &[
                ("minecraft:iron", "minecraft:trim/iron"),
                ("minecraft:gold", "minecraft:trim/gold"),
            ],
        )
    }

    #[test]
    fn material_palette_and_layer_folder() {
        let t = TrimTexture::resolve(
            &registry(),
            &[],
            "minecraft:gold",
            "minecraft:coast",
            "humanoid_leggings",
        )
        .unwrap();
        assert_eq!(t.texture, "minecraft:trims/entity/humanoid_leggings/coast");
        assert_eq!(t.palette.as_deref(), Some("minecraft:trim/gold"));
        assert!(
            TrimTexture::resolve(
                &registry(),
                &[],
                "minecraft:wood",
                "minecraft:coast",
                "humanoid"
            )
            .is_none()
        );
    }

    #[test]
    fn equipment_overrides_pick_the_darker_palette() {
        let iron = serde_json::json!({"trim_overrides": [
            {"palette": "minecraft:trim/iron_darker", "when": {"material": "minecraft:iron"}},
        ]});
        let overrides = TrimOverride::list(&iron);
        let resolve =
            |m| TrimTexture::resolve(&registry(), &overrides, m, "coast", "humanoid").unwrap();
        assert_eq!(
            resolve("iron").palette.as_deref(),
            Some("minecraft:trim/iron_darker")
        );
        assert_eq!(
            resolve("gold").palette.as_deref(),
            Some("minecraft:trim/gold")
        );
        let plain = TrimOverride::list(&serde_json::json!({"trim_overrides": [
            {"texture": "minecraft:other", "when": {"pattern": "minecraft:coast"}},
        ]}));
        let t = TrimTexture::resolve(&registry(), &plain, "gold", "coast", "humanoid").unwrap();
        assert_eq!(t.texture, "minecraft:trims/entity/humanoid/other");
        assert_eq!(t.palette, None);
    }

    #[test]
    fn vanilla_trims_are_recoloured() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/vise-client/26.3");
        if !root.join("assets/minecraft/textures/trims").is_dir() {
            return;
        }
        let registry = TrimRegistry::load(&root);
        let t = TrimTexture::resolve(
            &registry,
            &[],
            "minecraft:gold",
            "minecraft:coast",
            "humanoid",
        )
        .unwrap();
        let (w, h, gold) = t.generate(&root).unwrap();
        assert_eq!((w, h), (64, 32));
        let plain = TrimTexture {
            palette: None,
            ..t.clone()
        };
        let (_, _, grey) = plain.generate(&root).unwrap();
        let gold_palette = argb(
            &crate::hud::decode(&root.join("assets/minecraft/textures/palettes/trim/gold.png"))
                .unwrap()
                .2,
        );
        let mut opaque = 0;
        for (g, c) in grey.chunks(4).zip(gold.chunks(4)) {
            if g[3] != 0 {
                opaque += 1;
                let c = u32::from_be_bytes([0xFF, c[0], c[1], c[2]]);
                assert!(gold_palette.contains(&c), "{c:08x}");
            }
        }
        assert!(opaque > 0);
        assert_eq!(slot_size(&root), (64, 32));
    }
}
