
use crate::{
    entrypoint::{
        ClientModInitializer, Environment, ModContext, ModInitializer, ServerModInitializer,
    },
    registry::Registries,
};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Both,
    Client,
    Server,
}

impl Side {
    pub fn runs_on(self, env: Environment) -> bool {
        matches!(
            (self, env),
            (Side::Both, _)
                | (Side::Client, Environment::Client)
                | (Side::Server, Environment::Server)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub side: Side,
    pub depends: Vec<String>,
    pub after: Vec<String>,
}

impl ModMetadata {
    pub fn new(id: &str, version: &str) -> Self {
        Self {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            description: String::new(),
            authors: Vec::new(),
            side: Side::Both,
            depends: Vec::new(),
            after: Vec::new(),
        }
    }
    pub fn name(mut self, v: &str) -> Self {
        self.name = v.into();
        self
    }
    pub fn description(mut self, v: &str) -> Self {
        self.description = v.into();
        self
    }
    pub fn author(mut self, v: &str) -> Self {
        self.authors.push(v.into());
        self
    }
    pub fn side(mut self, v: Side) -> Self {
        self.side = v;
        self
    }
    pub fn depends_on(mut self, id: &str) -> Self {
        self.depends.push(id.into());
        self
    }
    pub fn load_after(mut self, id: &str) -> Self {
        self.after.push(id.into());
        self
    }
}

pub struct Mod {
    pub metadata: ModMetadata,
    main: Option<Box<dyn ModInitializer>>,
    client: Option<Box<dyn ClientModInitializer>>,
    server: Option<Box<dyn ServerModInitializer>>,
}

impl Mod {
    pub fn new(metadata: ModMetadata) -> Self {
        Self {
            metadata,
            main: None,
            client: None,
            server: None,
        }
    }
    pub fn main(mut self, e: impl ModInitializer + 'static) -> Self {
        self.main = Some(Box::new(e));
        self
    }
    pub fn client(mut self, e: impl ClientModInitializer + 'static) -> Self {
        self.client = Some(Box::new(e));
        self
    }
    pub fn server(mut self, e: impl ServerModInitializer + 'static) -> Self {
        self.server = Some(Box::new(e));
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    DuplicateMod(String),
    MissingDependency {
        mod_id: String,
        needs: String,
    },
    Cycle(Vec<String>),
    TooLate(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateMod(id) => write!(f, "mod {id} is added twice"),
            Self::MissingDependency { mod_id, needs } => {
                write!(f, "mod {mod_id} needs mod {needs}, which is not installed")
            }
            Self::TooLate(id) => write!(f, "mod {id} was added after mods were loaded"),
            Self::Cycle(ids) => write!(
                f,
                "mods depend on each other in a loop: {}",
                ids.join(" -> ")
            ),
        }
    }
}
impl std::error::Error for LoadError {}

pub struct ModLoader {
    environment: Environment,
    mods: Vec<Mod>,
    order: Vec<String>,
    registries: Registries,
}

impl ModLoader {
    pub fn new(environment: Environment) -> Self {
        Self {
            environment,
            mods: Vec::new(),
            order: Vec::new(),
            registries: Registries::new(),
        }
    }

    pub fn add(&mut self, m: Mod) -> Result<(), LoadError> {
        if self.mods.iter().any(|o| o.metadata.id == m.metadata.id) {
            return Err(LoadError::DuplicateMod(m.metadata.id));
        }
        self.mods.push(m);
        Ok(())
    }

    pub fn load(&mut self) -> Result<&Registries, LoadError> {
        let mods = std::mem::take(&mut self.mods);
        let mut mods: Vec<Mod> = mods
            .into_iter()
            .filter(|m| m.metadata.side.runs_on(self.environment))
            .collect();
        let order = sort(&mods)?;
        let mut slots: Vec<Option<Mod>> = mods.drain(..).map(Some).collect();
        let mut ordered: Vec<Mod> = order
            .into_iter()
            .map(|i| slots[i].take().expect("each mod once"))
            .collect();

        for m in &mut ordered {
            if let Some(e) = m.main.as_mut() {
                let mut ctx = ModContext {
                    metadata: &m.metadata,
                    environment: self.environment,
                    registries: &mut self.registries,
                };
                e.on_initialize(&mut ctx);
            }
        }
        for m in &mut ordered {
            let mut ctx = ModContext {
                metadata: &m.metadata,
                environment: self.environment,
                registries: &mut self.registries,
            };
            match self.environment {
                Environment::Client => {
                    if let Some(e) = m.client.as_mut() {
                        e.on_initialize_client(&mut ctx);
                    }
                }
                Environment::Server => {
                    if let Some(e) = m.server.as_mut() {
                        e.on_initialize_server(&mut ctx);
                    }
                }
            }
        }
        self.registries.freeze();
        self.order = ordered.iter().map(|m| m.metadata.id.clone()).collect();
        self.mods = ordered;
        Ok(&self.registries)
    }

    pub fn loaded(&self) -> &[String] {
        &self.order
    }

    pub fn registries(&self) -> &Registries {
        &self.registries
    }

    pub fn into_registries(self) -> Registries {
        self.registries
    }
}

fn sort(mods: &[Mod]) -> Result<Vec<usize>, LoadError> {
    let index: HashMap<&str, usize> = mods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.metadata.id.as_str(), i))
        .collect();
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); mods.len()];
    for (i, m) in mods.iter().enumerate() {
        for dep in &m.metadata.depends {
            let Some(&d) = index.get(dep.as_str()) else {
                return Err(LoadError::MissingDependency {
                    mod_id: m.metadata.id.clone(),
                    needs: dep.clone(),
                });
            };
            before[i].push(d);
        }
        for dep in &m.metadata.after {
            if let Some(&d) = index.get(dep.as_str()) {
                before[i].push(d);
            }
        }
    }
    let mut out = Vec::with_capacity(mods.len());
    let mut state = vec![0u8; mods.len()];
    for i in 0..mods.len() {
        visit(i, mods, &before, &mut state, &mut out, &mut Vec::new())?;
    }
    Ok(out)
}

fn visit(
    i: usize,
    mods: &[Mod],
    before: &[Vec<usize>],
    state: &mut [u8],
    out: &mut Vec<usize>,
    stack: &mut Vec<usize>,
) -> Result<(), LoadError> {
    match state[i] {
        2 => return Ok(()),
        1 => {
            let from = stack.iter().position(|&s| s == i).unwrap_or(0);
            let mut ids: Vec<String> = stack[from..]
                .iter()
                .map(|&s| mods[s].metadata.id.clone())
                .collect();
            ids.push(mods[i].metadata.id.clone());
            return Err(LoadError::Cycle(ids));
        }
        _ => {}
    }
    state[i] = 1;
    stack.push(i);
    for &d in &before[i] {
        visit(d, mods, before, state, out, stack)?;
    }
    stack.pop();
    state[i] = 2;
    out.push(i);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Log(Arc<Mutex<Vec<String>>>, &'static str);
    impl ModInitializer for Log {
        fn on_initialize(&mut self, ctx: &mut ModContext<'_>) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{}:{}", self.1, ctx.mod_id()));
        }
    }
    impl ClientModInitializer for Log {
        fn on_initialize_client(&mut self, ctx: &mut ModContext<'_>) {
            self.0
                .lock()
                .unwrap()
                .push(format!("client:{}", ctx.mod_id()));
        }
    }
    impl ServerModInitializer for Log {
        fn on_initialize_server(&mut self, ctx: &mut ModContext<'_>) {
            self.0
                .lock()
                .unwrap()
                .push(format!("server:{}", ctx.mod_id()));
        }
    }

    fn m(id: &str, log: &Arc<Mutex<Vec<String>>>) -> Mod {
        Mod::new(ModMetadata::new(id, "1.0.0"))
            .main(Log(log.clone(), "main"))
            .client(Log(log.clone(), ""))
            .server(Log(log.clone(), ""))
    }

    #[test]
    fn dependencies_load_first_then_side_entrypoints() {
        let log = Arc::default();
        let mut l = ModLoader::new(Environment::Client);
        let b = Mod::new(ModMetadata::new("b", "1").depends_on("a"))
            .main(Log(Arc::clone(&log), "main"));
        l.add(b).unwrap();
        l.add(m("a", &log)).unwrap();
        l.load().unwrap();
        assert_eq!(l.loaded(), ["a", "b"]);
        assert_eq!(*log.lock().unwrap(), ["main:a", "main:b", "client:a"]);
    }

    #[test]
    fn server_side_only_runs_server_entrypoints() {
        let log = Arc::default();
        let mut l = ModLoader::new(Environment::Server);
        l.add(m("a", &log)).unwrap();
        l.add(
            Mod::new(ModMetadata::new("c", "1").side(Side::Client))
                .main(Log(Arc::clone(&log), "main")),
        )
        .unwrap();
        l.load().unwrap();
        assert_eq!(l.loaded(), ["a"]);
        assert_eq!(*log.lock().unwrap(), ["main:a", "server:a"]);
    }

    #[test]
    fn reports_missing_duplicate_and_cycles() {
        let log = Arc::default();
        let mut l = ModLoader::new(Environment::Client);
        l.add(Mod::new(ModMetadata::new("a", "1").depends_on("zzz")))
            .unwrap();
        assert_eq!(
            l.add(m("a", &log)),
            Err(LoadError::DuplicateMod("a".into()))
        );
        assert!(matches!(l.load(), Err(LoadError::MissingDependency { .. })));

        let mut l = ModLoader::new(Environment::Client);
        l.add(Mod::new(ModMetadata::new("a", "1").depends_on("b")))
            .unwrap();
        l.add(Mod::new(ModMetadata::new("b", "1").depends_on("a")))
            .unwrap();
        assert_eq!(
            l.load().err(),
            Some(LoadError::Cycle(vec!["a".into(), "b".into(), "a".into()]))
        );
    }

    #[test]
    fn load_after_is_soft() {
        let mut l = ModLoader::new(Environment::Client);
        l.add(Mod::new(ModMetadata::new("x", "1").load_after("y")))
            .unwrap();
        l.add(Mod::new(ModMetadata::new("y", "1"))).unwrap();
        l.load().unwrap();
        assert_eq!(l.loaded(), ["y", "x"]);
        let mut l = ModLoader::new(Environment::Client);
        l.add(Mod::new(ModMetadata::new("x", "1").load_after("absent")))
            .unwrap();
        l.load().unwrap();
    }
}
