use crate::Result;
use vise_assets::{model::CuboidModel, resolve::ModelLibrary};
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

#[derive(Default)]
pub struct ParsedModels {
    models: BTreeMap<String, Arc<CuboidModel>>,
}
impl ParsedModels {
    pub fn load(root: &Path) -> Self {
        let Ok(namespaces) = fs::read_dir(root.join("assets")) else {
            return Self::default();
        };
        let mut files = Vec::new();
        for ns in namespaces.flatten() {
            let namespace = ns.file_name().to_string_lossy().into_owned();
            let base = ns.path().join("models");
            let mut stack = vec![base.clone()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                for e in entries.flatten() {
                    let path = e.path();
                    let is_dir = match e.file_type() {
                        Ok(t) if !t.is_symlink() => t.is_dir(),
                        _ => path.is_dir(),
                    };
                    if is_dir {
                        stack.push(path);
                    } else if path.extension().is_some_and(|x| x == "json")
                        && let Ok(rel) = path.strip_prefix(&base)
                    {
                        let rel = rel.with_extension("").to_string_lossy().replace('\\', "/");
                        files.push((format!("{namespace}:{rel}"), path));
                    }
                }
            }
        }
        let parsed = crate::par::par_map(&files, |(_, path)| {
            let text = fs::read_to_string(path).ok()?;
            CuboidModel::from_json(&text).ok().map(Arc::new)
        });
        let models = files
            .into_iter()
            .zip(parsed)
            .filter_map(|((id, _), model)| Some((id, model?)))
            .collect();
        Self { models }
    }

    pub fn block_library(&self, root: &Path, pending: Vec<String>) -> Result<ModelLibrary> {
        self.block_models(root, pending, false)
    }
    pub fn block_library_or_missing(
        &self,
        root: &Path,
        pending: Vec<String>,
    ) -> Result<ModelLibrary> {
        self.block_models(root, pending, true)
    }
    fn block_models(
        &self,
        root: &Path,
        mut pending: Vec<String>,
        or_missing: bool,
    ) -> Result<ModelLibrary> {
        let mut models = BTreeMap::new();
        let mut skipped = std::collections::BTreeSet::new();
        while let Some(name) = pending.pop() {
            if models.contains_key(&name) || skipped.contains(&name) {
                continue;
            }
            let model = match self.models.get(&name) {
                Some(model) => Arc::clone(model),
                None => {
                    let read = || -> Result<CuboidModel> {
                        let (namespace, path) =
                            name.split_once(':').ok_or("invalid model identifier")?;
                        let text = fs::read_to_string(
                            root.join(format!("assets/{namespace}/models/{path}.json")),
                        )?;
                        Ok(CuboidModel::from_json(&text)?)
                    };
                    match read() {
                        Ok(model) => Arc::new(model),
                        Err(e) if or_missing => {
                            log::warn!("Vise: missing block model {name}: {e}");
                            skipped.insert(name);
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
            if let Some(parent) = &model.parent {
                pending.push(parent.clone());
            }
            models.insert(name, model);
        }
        Ok(ModelLibrary::from_shared(models)?)
    }
    pub fn into_item_library(self) -> std::result::Result<Arc<ModelLibrary>, String> {
        ModelLibrary::with_shared_item_generator(self.models)
            .map(Arc::new)
            .map_err(|e| format!("item models: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vise_assets::resolve::{GENERATED_MODEL, MISSING_MODEL};

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn write(&self, name: &str, json: &str) {
            let (namespace, path) = name.split_once(':').unwrap();
            let path = self
                .0
                .join(format!("assets/{namespace}/models/{path}.json"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, json).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn shared_views_preserve_resolution_errors_builtins_and_reload() -> Result<()> {
        let fixture = Fixture(
            std::env::temp_dir().join(format!("vise-shared-models-{}", std::process::id())),
        );
        fixture.write("vise:parent", r##"{"textures":{"side":"#all","all":"block/stone"},"elements":[{"from":[0,0,0],"to":[16,8,16],"faces":{"up":{"texture":"#side"}}}]}"##);
        fixture.write("vise:nested/child", r#"{"parent":"vise:parent","textures":{"all":"block/glass"},"display":{"gui":{"rotation":[30,45,0]}}}"#);
        fixture.write("vise:broken", "{");
        fixture.write("vise:dangling", r#"{"parent":"vise:absent"}"#);
        fixture.write("vise:a", r#"{"parent":"vise:b"}"#);
        fixture.write("vise:b", r#"{"parent":"vise:a"}"#);
        fixture.write(
            GENERATED_MODEL,
            r#"{"elements":[{"from":[0,0,0],"to":[1,1,1],"faces":{"up":{"texture":"particle"}}}],"gui_light":"side"}"#,
        );
        fixture.write(MISSING_MODEL, r#"{"elements":[]}"#);
        let parsed = ParsedModels::load(&fixture.0);
        let direct = ParsedModels::default();
        let names = vec![
            "vise:nested/child".into(),
            GENERATED_MODEL.into(),
            MISSING_MODEL.into(),
            "vise:a".into(),
        ];
        let old = direct.block_library(&fixture.0, names.clone())?;
        let shared = parsed.block_library(&fixture.0, names)?;
        for name in ["vise:nested/child", GENERATED_MODEL, MISSING_MODEL] {
            assert_eq!(
                format!("{:?}", old.resolve(name)?),
                format!("{:?}", shared.resolve(name)?)
            );
        }
        assert!(shared.resolve("vise:a").is_err());
        for name in ["vise:broken", "vise:absent", "vise:dangling"] {
            let load = |models: &ParsedModels| {
                models
                    .block_library(&fixture.0, vec![name.into()])
                    .err()
                    .unwrap()
                    .to_string()
            };
            assert_eq!(load(&direct), load(&parsed));
            let lenient = parsed.block_library_or_missing(&fixture.0, vec![name.into()])?;
            assert_eq!(
                format!("{:?}", lenient.resolve(name)?.elements()),
                format!("{:?}", lenient.resolve(MISSING_MODEL)?.elements()),
                "{name}"
            );
        }
        let items = parsed.into_item_library()?;
        let block = shared.resolve("vise:nested/child")?;
        let item = items.resolve("vise:nested/child")?;
        assert!(std::ptr::eq(
            block.elements().as_ptr(),
            item.elements().as_ptr()
        ));
        assert_eq!(
            item.material("side")?.unwrap().sprite,
            "minecraft:block/glass"
        );
        assert_eq!(shared.resolve(GENERATED_MODEL)?.elements().len(), 1);
        assert!(items.resolve(GENERATED_MODEL)?.elements().is_empty());
        assert!(items.resolve(GENERATED_MODEL)?.uses_item_generator());
        for name in [
            "vise:broken",
            "vise:absent",
            "vise:dangling",
            MISSING_MODEL,
        ] {
            assert_eq!(items.resolve(name)?.elements().len(), 1);
        }
        assert!(items.resolve("vise:a").is_err());
        fixture.write("vise:parent", r#"{"elements":[]}"#);
        let reloaded = ParsedModels::load(&fixture.0).into_item_library()?;
        assert!(
            reloaded
                .resolve("vise:nested/child")?
                .elements()
                .is_empty()
        );
        assert_eq!(items.resolve("vise:nested/child")?.elements().len(), 1);
        fixture.write("vise:INVALID", "{}");
        let parsed = ParsedModels::load(&fixture.0);
        assert!(
            parsed
                .block_library(&fixture.0, vec!["vise:nested/child".into()])
                .is_ok()
        );
        assert!(parsed.into_item_library().is_err());
        Ok(())
    }
}
