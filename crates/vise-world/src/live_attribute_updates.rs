use crate::{
    attribute_map::ValueKind,
    attribute_updates::{self, UpdateWorld, UpdatedAttribute},
    attributes::live::{Error, Handle, Key, LiveAttributes},
    object_open_set::CursorError,
};

pub trait LiveAttributeWorld: UpdateWorld<Key> {
    type Identity: FnMut() -> i32;
    fn get_attributes(&mut self) -> Result<Self::Attributes, Self::Error>;
    fn live_attributes(&self, map: Self::Attributes) -> &LiveAttributes<Self::Identity>;
    fn live_attributes_mut(&mut self, map: Self::Attributes)
    -> &mut LiveAttributes<Self::Identity>;
    fn instance_attribute(&self, instance: Option<Handle>) -> Result<Key, Self::Error>;
    fn attribute_error(error: Error) -> Self::Error;
    fn cursor_error(error: CursorError) -> Self::Error;
    fn dispatch_attribute_updated(&mut self, attribute: Key) -> Result<(), Self::Error>
    where
        Self: Sized,
    {
        attribute_updates::updated(self, attribute)
    }
}

pub fn kind(attribute: &Key) -> UpdatedAttribute {
    match attribute.name() {
        "minecraft:max_health" => UpdatedAttribute::MaxHealth,
        "minecraft:max_absorption" => UpdatedAttribute::MaxAbsorption,
        "minecraft:scale" => UpdatedAttribute::Scale,
        "minecraft:waypoint_transmit_range" => UpdatedAttribute::WaypointRange,
        _ => UpdatedAttribute::Other,
    }
}

pub fn value<W: LiveAttributeWorld>(
    world: &mut W,
    attribute: Option<Key>,
    kind: ValueKind,
) -> Result<f64, W::Error> {
    let map = world.get_attributes()?;
    value_in(world, map, attribute, kind)
}

pub fn value_in<W: LiveAttributeWorld>(
    world: &mut W,
    map: W::Attributes,
    attribute: Option<Key>,
    kind: ValueKind,
) -> Result<f64, W::Error> {
    world
        .live_attributes_mut(map)
        .value(attribute, kind)
        .map_err(W::attribute_error)
}

pub fn refresh<W: LiveAttributeWorld>(world: &mut W) -> Result<(), W::Error> {
    let map = world.get_attributes()?;
    let mut cursor = world.live_attributes(map).updates().cursor();
    while cursor.has_next() {
        let instance = world
            .live_attributes(map)
            .updates()
            .next(&mut cursor)
            .map_err(W::cursor_error)?;
        let attribute = world.instance_attribute(instance)?;
        world.dispatch_attribute_updated(attribute)?;
    }
    world.live_attributes_mut(map).updates_mut().clear();
    Ok(())
}
