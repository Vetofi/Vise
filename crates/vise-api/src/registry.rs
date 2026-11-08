
use crate::identifier::Identifier;
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RawId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    Duplicate {
        registry: Identifier,
        id: Identifier,
    },
    Frozen {
        registry: Identifier,
        id: Identifier,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { registry, id } => write!(f, "{id} is already in registry {registry}"),
            Self::Frozen { registry, id } => {
                write!(
                    f,
                    "cannot add {id}: registry {registry} is frozen (register in ModInitializer::on_initialize)"
                )
            }
        }
    }
}
impl std::error::Error for RegistryError {}

pub struct Registry<T> {
    name: Identifier,
    entries: Vec<(Identifier, T)>,
    by_id: HashMap<Identifier, RawId>,
    frozen: bool,
}

impl<T> Registry<T> {
    pub fn new(name: Identifier) -> Self {
        Self {
            name,
            entries: Vec::new(),
            by_id: HashMap::new(),
            frozen: false,
        }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn register(&mut self, id: Identifier, value: T) -> Result<RawId, RegistryError> {
        if self.frozen {
            return Err(RegistryError::Frozen {
                registry: self.name.clone(),
                id,
            });
        }
        if self.by_id.contains_key(&id) {
            return Err(RegistryError::Duplicate {
                registry: self.name.clone(),
                id,
            });
        }
        let raw = RawId(self.entries.len() as u32);
        self.by_id.insert(id.clone(), raw);
        self.entries.push((id, value));
        Ok(raw)
    }

    pub fn get(&self, id: &Identifier) -> Option<&T> {
        self.by_id.get(id).map(|r| &self.entries[r.0 as usize].1)
    }

    pub fn get_raw(&self, raw: RawId) -> Option<&T> {
        self.entries.get(raw.0 as usize).map(|e| &e.1)
    }

    pub fn raw_id(&self, id: &Identifier) -> Option<RawId> {
        self.by_id.get(id).copied()
    }

    pub fn id_of(&self, raw: RawId) -> Option<&Identifier> {
        self.entries.get(raw.0 as usize).map(|e| &e.0)
    }

    pub fn contains(&self, id: &Identifier) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Identifier, &T)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }
}

pub struct Registries {
    pub blocks: Registry<crate::content::BlockDef>,
    pub items: Registry<crate::content::ItemDef>,
}

impl Registries {
    pub fn new() -> Self {
        Self {
            blocks: Registry::new(Identifier::of("minecraft", "block")),
            items: Registry::new(Identifier::of("minecraft", "item")),
        }
    }

    pub fn freeze(&mut self) {
        self.blocks.freeze();
        self.items.freeze();
    }
}

impl Default for Registries {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry<&'static str> {
        Registry::new(Identifier::of("vise", "test"))
    }

    #[test]
    fn registers_and_looks_up() {
        let mut r = reg();
        let ruby = Identifier::of("m", "ruby");
        assert_eq!(r.register(ruby.clone(), "Ruby"), Ok(RawId(0)));
        assert_eq!(
            r.register(Identifier::of("m", "opal"), "Opal"),
            Ok(RawId(1))
        );
        assert_eq!(r.get(&ruby), Some(&"Ruby"));
        assert_eq!(
            r.id_of(RawId(1)).map(|i| i.to_string()),
            Some("m:opal".into())
        );
        assert_eq!(r.raw_id(&ruby), Some(RawId(0)));
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn refuses_duplicates_and_frozen() {
        let mut r = reg();
        let a = Identifier::of("m", "a");
        r.register(a.clone(), "A").unwrap();
        assert!(matches!(
            r.register(a, "A2"),
            Err(RegistryError::Duplicate { .. })
        ));
        r.freeze();
        assert!(matches!(
            r.register(Identifier::of("m", "b"), "B"),
            Err(RegistryError::Frozen { .. })
        ));
    }
}
