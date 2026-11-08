mod facts;
mod json;
use crate::{DATA_VERSION, chunk::BlockState, compression::invalid, nbt::Tag};
pub use json::BlockStateJsonCodec;
pub(crate) use json::java_integer;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    io,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockStateId(pub u32);

pub struct BlockRegistry {
    states: Vec<BlockState>,
    ids: HashMap<u64, u32>,
    collided: BTreeMap<BlockState, BlockStateId>,
    defaults: BTreeMap<String, BlockStateId>,
}
fn state_key(state: &BlockState) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.name.hash(&mut hasher);
    for (key, value) in &state.properties {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    hasher.finish()
}
impl BlockRegistry {
    pub fn from_nbt(tag: &Tag) -> io::Result<Self> {
        if tag.get("DataVersion") != Some(&Tag::Int(DATA_VERSION)) {
            return Err(invalid("block registry DataVersion mismatch"));
        }
        let Some(Tag::List(entries)) = tag.get("states") else {
            return Err(invalid("missing registry states"));
        };
        let Some(Tag::Compound(defaults)) = tag.get("defaults") else {
            return Err(invalid("missing registry defaults"));
        };
        let states = entries
            .iter()
            .map(BlockState::from_nbt)
            .collect::<io::Result<_>>()?;
        let defaults = defaults
            .iter()
            .map(|(name, id)| {
                let name = name.to_string().map_err(|e| invalid(e.to_string()))?;
                let Tag::Int(id) = id else {
                    return Err(invalid("default state ID is not an int"));
                };
                Ok((name, i64::from(*id)))
            })
            .collect::<io::Result<_>>()?;
        Self::from_states(states, defaults)
    }

    pub fn from_facts_json(reader: impl io::Read) -> io::Result<Self> {
        Ok(Self::from_facts_json_with_flags(reader)?.0)
    }

    pub fn from_facts_json_with_flags(
        reader: impl io::Read,
    ) -> io::Result<(Self, Vec<(bool, bool)>)> {
        let facts: facts::Facts = serde_json::from_reader(io::BufReader::new(reader))
            .map_err(|e| invalid(e.to_string()))?;
        let flags = facts.states.iter().map(|r| (r.air, r.fluid)).collect();
        let states = facts.states.into_iter().map(|r| r.state).collect();
        Ok((Self::from_states(states, facts.defaults)?, flags))
    }

    fn from_states(states: Vec<BlockState>, defaults: BTreeMap<String, i64>) -> io::Result<Self> {
        if states.is_empty() || states.len() > i32::MAX as usize {
            return Err(invalid("invalid registry size"));
        }
        let mut result = Self {
            states: Vec::with_capacity(states.len()),
            ids: HashMap::with_capacity(states.len()),
            collided: BTreeMap::new(),
            defaults: BTreeMap::new(),
        };
        for (i, state) in states.into_iter().enumerate() {
            if result.id(&state).is_some() {
                return Err(invalid("duplicate block registry state"));
            }
            match result.ids.entry(state_key(&state)) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(i as u32);
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    result
                        .collided
                        .insert(state.clone(), BlockStateId(i as u32));
                }
            }
            result.states.push(state);
        }
        for (name, id) in defaults {
            if id < 0
                || id > i64::from(i32::MAX)
                || result
                    .states
                    .get(id as usize)
                    .is_none_or(|s| s.name != name)
            {
                return Err(invalid("invalid default block state ID"));
            }
            result.defaults.insert(name, BlockStateId(id as u32));
        }
        let names: BTreeSet<_> = result.states.iter().map(|s| &s.name).collect();
        if names.len() != result.defaults.len()
            || names
                .iter()
                .any(|name| !result.defaults.contains_key(*name))
        {
            return Err(invalid("registry is missing block defaults"));
        }
        Ok(result)
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
    pub fn state(&self, id: BlockStateId) -> Option<&BlockState> {
        self.states.get(id.0 as usize)
    }
    pub fn id(&self, state: &BlockState) -> Option<BlockStateId> {
        match self.ids.get(&state_key(state)) {
            Some(&id) if self.states[id as usize] == *state => Some(BlockStateId(id)),
            _ => self.collided.get(state).copied(),
        }
    }
    pub fn default_state(&self, name: &str) -> Option<BlockStateId> {
        self.defaults.get(name).copied()
    }

    pub fn property_values(&self, block: &str, property: &str) -> Vec<&str> {
        let mut values: Vec<&str> = Vec::new();
        for state in self
            .states
            .iter()
            .filter(|state| state.name.as_str() == block)
        {
            if let Some(value) = state.properties.get(property) {
                let value = value.as_str();
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        values
    }

    pub fn resolve(&self, descriptor: &BlockState) -> io::Result<BlockStateId> {
        let default = self
            .default_state(&descriptor.name)
            .ok_or_else(|| invalid("unknown block name"))?;
        let mut state = self.state(default).unwrap().clone();
        for (key, value) in &descriptor.properties {
            if !state.properties.contains_key(key) {
                return Err(invalid("unknown block property"));
            }
            state.properties.insert(key.clone(), value.clone());
        }
        self.id(&state)
            .ok_or_else(|| invalid("invalid block property value"))
    }
}

#[derive(Clone, Copy)]
pub struct BlockTable<'t, 'r> {
    tag: &'t Tag,
    registry: &'r BlockRegistry,
    read_checked: u8,
}
impl<'t, 'r> BlockTable<'t, 'r> {
    pub fn check(tag: &'t Tag, registry: &'r BlockRegistry, what: &str) -> io::Result<Self> {
        Self {
            tag,
            registry,
            read_checked: 0,
        }
        .checked(what)
    }
    fn checked(self, what: &str) -> io::Result<Self> {
        let Some(Tag::List(states)) = self.tag.get("states") else {
            return Err(invalid(format!("missing {what} state identities")));
        };
        if self.read_checked == 0 {
            if states.len() != self.registry.len() {
                return Err(invalid(format!("{what} registry size mismatch")));
            }
            for (id, state) in states.iter().enumerate() {
                if self.registry.state(BlockStateId(id as u32))
                    != Some(&BlockState::from_nbt(state)?)
                {
                    return Err(invalid(format!("{what} state identities differ")));
                }
            }
        }
        Ok(self)
    }
    pub fn nested(&self, key: &str, what: &str) -> io::Result<Self> {
        Self {
            tag: self
                .tag
                .get(key)
                .ok_or_else(|| invalid(format!("missing {what}")))?,
            registry: self.registry,
            read_checked: self.read_checked.saturating_sub(1),
        }
        .checked(what)
    }
    pub fn tag(&self) -> &'t Tag {
        self.tag
    }
    pub fn get(&self, key: &str) -> Option<&'t Tag> {
        self.tag.get(key)
    }
    pub fn registry(&self) -> &'r BlockRegistry {
        self.registry
    }
}

pub struct BlockTableFile<'r> {
    tag: Tag,
    registry: &'r BlockRegistry,
}
impl<'r> BlockTableFile<'r> {
    pub fn open(path: &std::path::Path, registry: &'r BlockRegistry) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::read(&bytes, registry).map_err(|e| invalid(format!("{}: {e}", path.display())))
    }
    pub fn read(bytes: &[u8], registry: &'r BlockRegistry) -> io::Result<Self> {
        let mut visit = |states: &mut crate::nbt::ListElements<'_, '_, '_>| {
            if states.len() != registry.len() {
                return Err(states.error("table registry size mismatch"));
            }
            let mut id = 0u32;
            while let Some(tag) = states.next_element()? {
                let state =
                    BlockState::from_nbt(&tag).map_err(|_| states.error("invalid table state"))?;
                if registry.state(BlockStateId(id)) != Some(&state) {
                    return Err(states.error("table state identities differ"));
                }
                id += 1;
            }
            Ok(())
        };
        let (named, _) = crate::nbt::read_named_streaming(
            bytes,
            crate::nbt::Limits::tables(),
            "states",
            &mut visit,
        )
        .map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            tag: named.tag,
            registry,
        })
    }
    pub fn table(&self, what: &str) -> io::Result<BlockTable<'_, 'r>> {
        BlockTable {
            tag: &self.tag,
            registry: self.registry,
            read_checked: 2,
        }
        .checked(what)
    }
}
