mod block_state_data;
mod chunk_flatten;
mod chunk_leaves;
pub mod dynamic;
pub mod files;
mod fixes_a;
mod fixes_b;
mod fixes_c;
mod fixes_cliffs;
mod fixes_flat;
mod fixes_legacy;
mod fixes_mid;
mod fixes_old;
mod flat_tables;
mod items;
pub mod json;
pub mod schema;
mod text;

use crate::nbt::Tag;
use dynamic::{as_str, string};
use schema::{Key, Ty, key, walk};

pub const CURRENT: i32 = crate::DATA_VERSION;
pub const OLDEST_SUPPORTED: i32 = -1;

pub fn identifier(id: &str) -> Option<(String, String)> {
    let (ns, path) = match id.split_once(':') {
        Some((ns, path)) => (if ns.is_empty() { "minecraft" } else { ns }, path),
        None => ("minecraft", id),
    };
    let ns_ok = ns
        .bytes()
        .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'));
    let path_ok = path
        .bytes()
        .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'));
    (ns_ok && path_ok).then(|| (ns.to_owned(), path.to_owned()))
}

pub struct Cx<'a> {
    root: Ty,
    data: &'a mut Tag,
    k: Key,
}

impl Cx<'_> {
    pub fn each(&mut self, ty: Ty, mut f: impl FnMut(&mut Tag)) {
        walk(self.root, self.data, self.k - 1, ty, &mut f);
    }
    pub fn each_out(&mut self, ty: Ty, mut f: impl FnMut(&mut Tag)) {
        walk(self.root, self.data, self.k, ty, &mut f);
    }
    pub fn entity(&mut self, id: &str, mut f: impl FnMut(&mut Tag)) {
        self.each(Ty::Entity, |e| {
            if id_is(e, id) {
                f(e);
            }
        });
    }
    pub fn block_entity(&mut self, id: &str, mut f: impl FnMut(&mut Tag)) {
        self.each(Ty::BlockEntity, |e| {
            if id_is(e, id) {
                f(e);
            }
        });
    }
    pub fn item(&mut self, filter: impl Fn(&str) -> bool, mut f: impl FnMut(&mut Tag)) {
        self.each(Ty::ItemStack, |s| {
            if let Some(id) = as_str(s.get("id"))
                && filter(&dynamic::namespaced(&id))
            {
                f(s);
            }
        });
    }
    pub fn rename(&mut self, ty: Ty, rename: impl Fn(&str) -> Option<&'static str>) {
        self.each(ty, |v| {
            if let Some(name) = as_str(Some(v))
                && let Some(new) = rename(&name)
            {
                *v = string(new);
            }
        });
    }
}

pub(crate) const LAST_READ: &str = "\u{0}last_read";

pub fn mark_read(e: &mut Tag) {
    if let Tag::Compound(_) = e {
        let mut raw = e.clone();
        strip_last_read(&mut raw);
        dynamic::set(e, LAST_READ, raw);
    }
}

pub fn reread(k: Key, e: &mut Tag) {
    reread_as(Ty::Entity, k, e);
}

pub fn reread_as(ty: Ty, k: Key, v: &mut Tag) {
    schema::flat::strip_untyped(v);
    schema::flat::mark_untyped(ty, v, k);
    walk(ty, v, k, Ty::BlockEntity, &mut mark_read);
    walk(ty, v, k, Ty::Entity, &mut mark_read);
}

pub fn drop_typed(e: &mut Tag, field: &str) {
    match e.get(LAST_READ).and_then(|raw| raw.get(field)).cloned() {
        Some(raw) => dynamic::set(e, field, raw),
        None => {
            dynamic::remove(e, field);
        }
    }
}

fn strip_last_read(v: &mut Tag) {
    match v {
        Tag::Compound(c) => {
            c.remove(&dynamic::key(LAST_READ));
            c.remove(&dynamic::key(schema::flat::UNTYPED));
            for value in c.values_mut() {
                strip_last_read(value);
            }
        }
        Tag::List(items) => items.iter_mut().for_each(strip_last_read),
        _ => {}
    }
}

pub fn id_is(v: &Tag, id: &str) -> bool {
    as_str(v.get("id")).is_some_and(|s| dynamic::namespaced(&s) == id)
}

pub fn renamer<'a>(
    table: &'a [(&'a str, &'static str)],
) -> impl Fn(&str) -> Option<&'static str> + 'a {
    move |id: &str| {
        let id = dynamic::namespaced(id);
        table
            .iter()
            .find(|(from, _)| *from == id)
            .map(|(_, to)| *to)
    }
}

type FixFn = fn(&mut Cx);

fn fixes() -> Vec<(Key, FixFn)> {
    let mut out = Vec::new();
    fixes_legacy::register(&mut out);
    fixes_flat::register(&mut out);
    fixes_mid::register(&mut out);
    fixes_cliffs::register(&mut out);
    fixes_old::register(&mut out);
    fixes_a::register(&mut out);
    fixes_b::register(&mut out);
    fixes_c::register(&mut out);
    out.sort_by_key(|(k, _)| *k);
    out
}

pub fn update(root: Ty, data: &mut Tag, from: i32, to: i32) {
    if from >= to {
        return;
    }
    let from_key = key(from.max(0) as u32, 0);
    let to_version = to as u32;
    walk(root, data, from_key, Ty::BlockEntity, &mut mark_read);
    walk(root, data, from_key, Ty::Entity, &mut mark_read);
    schema::flat::mark_untyped(root, data, from_key);
    read_hooks(root, data, from_key);
    let mut refreshed = from_key >= schema::flat::END;
    for (k, fix) in fixes() {
        if k > from_key && k / 10 <= to_version {
            if !refreshed && k >= schema::flat::END {
                refreshed = true;
                walk(
                    root,
                    data,
                    schema::flat::END - 1,
                    Ty::BlockEntity,
                    &mut mark_read,
                );
                walk(
                    root,
                    data,
                    schema::flat::END - 1,
                    Ty::Entity,
                    &mut mark_read,
                );
            }
            fix(&mut Cx { root, data, k });
        }
    }
    write_hooks(root, data, key(to_version, 9));
    strip_last_read(data);
}

fn read_hooks(root: Ty, data: &mut Tag, k: Key) {
    walk(root, data, k, Ty::ItemStack, &mut |stack| {
        if k < key(3818, 5) {
            items::add_names_at(stack, k);
        } else if let Tag::String(_) = stack {
            *stack = Tag::compound([("id", stack.clone()), ("count", Tag::Int(1))]);
        }
    });
    if k >= key(1451, 6) {
        walk(root, data, k, Ty::Objective, &mut fixes_a::unpack_objective);
    }
    walk(root, data, k, Ty::BlockName, &mut |name| {
        if let Some(n) = as_str(Some(name)) {
            *name = string(&dynamic::namespaced(&n));
        } else if k < key(1451, 4)
            && let Some(n) = dynamic::as_number(name)
        {
            *name = Tag::Int(n.int_value());
        }
    });
    for ty in [Ty::Biome, Ty::EntityName, Ty::ItemName] {
        walk(root, data, k, ty, &mut |name| {
            if let Some(n) = as_str(Some(name)) {
                *name = string(&dynamic::namespaced(&n));
            }
        });
    }
}

fn write_hooks(root: Ty, data: &mut Tag, k: Key) {
    walk(root, data, k, Ty::Objective, &mut fixes_a::repack_objective);
}

pub fn update_chunk(tag: &mut Tag, dimension: &str, generator: Option<&str>) {
    update_chunk_to(tag, dimension, generator, CURRENT);
}

pub fn update_chunk_to(tag: &mut Tag, dimension: &str, generator: Option<&str>, to: i32) {
    let version = dynamic::as_int(tag.get("DataVersion"), -1);
    if version >= to {
        return;
    }
    let mut context = Tag::compound([("dimension", string(dimension))]);
    if let Some(g) = generator {
        dynamic::set(&mut context, "generator", string(g));
    }
    dynamic::set(tag, "__context", context);
    update(Ty::Chunk, tag, version, to);
    dynamic::remove(tag, "__context");
    dynamic::set(tag, "DataVersion", Tag::Int(to));
}

pub fn update_region_tag(root: Ty, tag: &mut Tag) {
    update_region_tag_to(root, tag, CURRENT);
}

pub fn update_region_tag_to(root: Ty, tag: &mut Tag, to: i32) {
    let default = if root == Ty::PoiChunk { 1945 } else { -1 };
    let version = dynamic::as_int(tag.get("DataVersion"), default);
    if version >= to {
        return;
    }
    update(root, tag, version, to);
    dynamic::set(tag, "DataVersion", Tag::Int(to));
}

pub fn update_player(tag: &mut Tag) {
    let version = dynamic::as_int(tag.get("DataVersion"), -1);
    update(Ty::Player, tag, version, CURRENT);
}

pub fn update_player_to(tag: &mut Tag, to: i32) {
    let version = dynamic::as_int(tag.get("DataVersion"), -1);
    update(Ty::Player, tag, version, to);
    dynamic::set(tag, "DataVersion", Tag::Int(to));
}

pub fn update_hotbar(tag: &mut Tag) {
    let version = dynamic::as_int(tag.get("DataVersion"), 1343);
    update(Ty::Hotbar, tag, version, CURRENT);
}

pub fn saved_data_type(relative: &str) -> Option<Ty> {
    let name = relative.strip_suffix(".dat").unwrap_or(relative);
    if name.ends_with("/command_storage") {
        return Some(Ty::SavedCommandStorage);
    }
    if let Some(n) = name.strip_prefix("minecraft/maps/")
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(Ty::SavedMapData);
    }
    Some(match name {
        "minecraft/custom_boss_events" => Ty::SavedCustomBossEvents,
        "minecraft/scoreboard" => Ty::SavedScoreboard,
        "minecraft/chunk_tickets" => Ty::SavedTickets,
        "minecraft/world_border" => Ty::SavedWorldBorder,
        "minecraft/game_rules" => Ty::SavedGameRules,
        "minecraft/ender_dragon_fight" => Ty::SavedEnderDragonFight,
        "minecraft/wandering_trader" => Ty::SavedWanderingTrader,
        "minecraft/maps/last_id" => Ty::SavedMapIndex,
        "minecraft/weather" => Ty::SavedWeather,
        "minecraft/scheduled_events" => Ty::SavedScheduledEvents,
        "minecraft/world_gen_settings" => Ty::SavedWorldGenSettings,
        "minecraft/random_sequences" => Ty::SavedRandomSequences,
        "minecraft/stopwatches" => Ty::SavedStopwatches,
        "minecraft/raids" => Ty::SavedRaids,
        "minecraft/world_clocks" => Ty::SavedWorldClocks,
        _ => return None,
    })
}

pub fn update_saved_data(ty: Ty, tag: &mut Tag) {
    let version = dynamic::as_int(tag.get("DataVersion"), 1343);
    update(ty, tag, version, CURRENT);
}

#[cfg(test)]
mod tests;
