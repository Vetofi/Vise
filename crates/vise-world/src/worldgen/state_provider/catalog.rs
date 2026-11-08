use super::{BlockStateProvider, DecodeError, StateProviderBindings, identifier};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

pub struct StateProviderCatalog {
    resources: BTreeMap<String, Box<RawValue>>,
}
impl StateProviderCatalog {
    pub fn from_resources(resources: &BTreeMap<String, String>) -> Result<Self, DecodeError> {
        if resources.len() > 8192 {
            return Err(DecodeError::Field("state provider resource count"));
        }
        let mut result = BTreeMap::new();
        for (name, json) in resources {
            if result
                .insert(identifier(name)?, serde_json::from_str(json)?)
                .is_some()
            {
                return Err(DecodeError::Field("duplicate state provider resource"));
            }
        }
        Ok(Self { resources: result })
    }
    pub(super) fn lookup(&self, name: &str) -> Option<&str> {
        self.resources.get(name).map(|v| v.get())
    }
    pub fn provider(
        &self,
        json: &str,
        bindings: &StateProviderBindings<'_>,
    ) -> Result<BlockStateProvider, DecodeError> {
        BlockStateProvider::decode_holder(json, bindings, 0, &mut 65536, Some(self))
    }
    pub(crate) fn provider_with_budget(
        &self,
        json: &str,
        bindings: &StateProviderBindings<'_>,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<BlockStateProvider, DecodeError> {
        BlockStateProvider::decode_holder(json, bindings, depth, remaining, Some(self))
    }
    pub fn direct_provider(
        &self,
        json: &str,
        bindings: &StateProviderBindings<'_>,
    ) -> Result<BlockStateProvider, DecodeError> {
        BlockStateProvider::decode(json, bindings, 0, &mut 65536, Some(self))
    }
    pub fn typed_provider(
        &self,
        json: &str,
        bindings: &StateProviderBindings<'_>,
    ) -> Result<BlockStateProvider, DecodeError> {
        BlockStateProvider::decode_typed(json, bindings, 0, &mut 65536, Some(self))
    }
}
