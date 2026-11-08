
use crate::{loader::ModMetadata, registry::Registries};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Environment {
    Client,
    Server,
}

pub struct ModContext<'a> {
    pub metadata: &'a ModMetadata,
    pub environment: Environment,
    pub registries: &'a mut Registries,
}

impl ModContext<'_> {
    pub fn mod_id(&self) -> &str {
        &self.metadata.id
    }

    #[track_caller]
    pub fn id(&self, path: &str) -> crate::Identifier {
        crate::Identifier::of(&self.metadata.id, path)
    }
}

pub trait ModInitializer: Send {
    fn on_initialize(&mut self, ctx: &mut ModContext<'_>);
}

pub trait ClientModInitializer: Send {
    fn on_initialize_client(&mut self, ctx: &mut ModContext<'_>);
}

pub trait ServerModInitializer: Send {
    fn on_initialize_server(&mut self, ctx: &mut ModContext<'_>);
}
