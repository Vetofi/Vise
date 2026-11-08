use super::{Game, lan::LanHost};
use vise_client::{items::ItemStack, lan::data::component_nbt, text::Text};
use vise_protocol::packets::play::{self, Clientbound as C};
use serde_json::{Map, Value, json};
use std::{collections::HashMap, path::PathBuf};

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeServerPlayer {
    pub(crate) uuid: u128,
    pub(crate) name: String,
    pub(crate) permission: u8,
    pub(crate) dimension: String,
    pub(crate) position: [f64; 3],
    pub(crate) game_mode: i32,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeCommand {
    pub(crate) usage: String,
    pub(crate) level: u8,
}

#[allow(dead_code)]
impl NativeCommand {
    pub(crate) fn new(usage: &str) -> Self {
        Self {
            usage: usage.to_owned(),
            level: 0,
        }
    }
    pub(crate) fn op(usage: &str, level: u8) -> Self {
        Self {
            usage: usage.to_owned(),
            level: level.min(4),
        }
    }
}

fn parser(kind: &str) -> Option<(&'static str, Value)> {
    Some(match kind {
        "word" => ("brigadier:string", json!({"type": "word"})),
        "text" => ("brigadier:string", json!({"type": "greedy"})),
        "integer" => ("brigadier:integer", Value::Null),
        "number" => ("brigadier:double", Value::Null),
        "boolean" => ("brigadier:bool", Value::Null),
        "player" => (
            "minecraft:entity",
            json!({"amount": "single", "type": "players"}),
        ),
        "item" => ("minecraft:item_stack", Value::Null),
        "block_pos" => ("minecraft:block_pos", Value::Null),
        _ => return None,
    })
}

const LEVELS: [&str; 5] = ["", "moderators", "gamemasters", "admins", "owners"];

fn usage_nodes(usage: &str) -> Result<Vec<(String, Value, bool)>, String> {
    let mut out = Vec::new();
    let mut optional_seen = false;
    let mut greedy_seen = false;
    for word in usage.split_whitespace() {
        if greedy_seen {
            return Err(format!("`{usage}`: a text argument must be last"));
        }
        let (inner, optional) = if let Some(w) = word.strip_prefix('<') {
            (w.strip_suffix('>'), false)
        } else if let Some(w) = word.strip_prefix('[') {
            (w.strip_suffix(']'), true)
        } else {
            if optional_seen {
                return Err(format!("`{usage}`: `{word}` follows an optional argument"));
            }
            let literal = !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-".contains(c));
            if !literal {
                return Err(format!("`{usage}`: `{word}` is not a command word"));
            }
            out.push((word.to_owned(), json!({"type": "literal"}), false));
            continue;
        };
        let Some((name, kind)) = inner.and_then(|w| w.split_once(':')) else {
            return Err(format!("`{usage}`: `{word}` is not `<name:kind>`"));
        };
        if optional_seen && !optional {
            return Err(format!("`{usage}`: `{word}` follows an optional argument"));
        }
        let Some((parser, properties)) = parser(kind) else {
            return Err(format!("`{usage}`: unknown kind `{kind}`"));
        };
        optional_seen |= optional;
        greedy_seen |= kind == "text";
        let mut node = json!({"type": "argument", "parser": parser});
        if !properties.is_null() {
            node["properties"] = properties;
        }
        out.push((name.to_owned(), node, optional));
    }
    if out.first().is_none_or(|n| n.1["type"] != "literal") {
        return Err(format!("`{usage}`: a command starts with its name"));
    }
    Ok(out)
}

pub(super) fn command_tree(
    commands: &[NativeCommand],
    vanilla: impl Fn(&str) -> bool,
) -> (Value, HashMap<String, u8>, Vec<String>) {
    let mut root = Map::new();
    let mut levels: HashMap<String, u8> = HashMap::new();
    let mut problems = Vec::new();
    for command in commands {
        let nodes = match usage_nodes(&command.usage) {
            Ok(n) => n,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        if vanilla(&nodes[0].0) {
            problems.push(format!("`{}`: vanilla has /{}", command.usage, nodes[0].0));
            continue;
        }
        let mut children = &mut root;
        for (i, (name, node, _)) in nodes.iter().enumerate() {
            let runnable = nodes.get(i + 1).is_none_or(|n| n.2);
            let entry = children.entry(name.clone()).or_insert_with(|| {
                let mut n = node.clone();
                n["executable"] = false.into();
                n["level"] = 4.into();
                n
            });
            if entry["type"] != node["type"] || entry["parser"] != node["parser"] {
                problems.push(format!(
                    "`{}`: `{name}` is another kind in an earlier usage",
                    command.usage
                ));
                break;
            }
            if runnable {
                entry["executable"] = true.into();
            }
            let level = entry["level"]
                .as_u64()
                .unwrap_or(4)
                .min(command.level.into());
            entry["level"] = level.into();
            if !entry["children"].is_object() {
                entry["children"] = Value::Object(Map::new());
            }
            children = entry["children"].as_object_mut().expect("made above");
        }
        let level = levels.entry(nodes[0].0.clone()).or_insert(4);
        *level = (*level).min(command.level);
    }
    let mut root = Value::Object(root);
    finish(&mut root);
    let tree = if levels.is_empty() {
        Value::Null
    } else {
        json!({"type": "root", "children": root})
    };
    (tree, levels, problems)
}

fn finish(children: &mut Value) {
    let Some(map) = children.as_object_mut() else {
        return;
    };
    for node in map.values_mut() {
        let level = node["level"].as_u64().unwrap_or(0) as usize;
        let object = node.as_object_mut().expect("a node");
        object.remove("level");
        if level > 0 {
            object.insert(
                "permissions".into(),
                json!({"type": "require", "permission": {"level": LEVELS[level.min(4)]}}),
            );
        }
        if object
            .get("children")
            .is_some_and(|c| c.as_object().is_some_and(Map::is_empty))
        {
            object.remove("children");
        } else if let Some(c) = object.get_mut("children") {
            finish(c);
        }
    }
}

#[allow(dead_code)]
impl Game<'_> {
    pub(super) fn native_server_players(&self) -> Vec<NativeServerPlayer> {
        let Some(host) = &self.lan else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| g.placed)
            .map(|g| NativeServerPlayer {
                uuid: g.uuid,
                name: g.name.clone(),
                permission: g.permission,
                dimension: g.dimension.clone(),
                position: g.position,
                game_mode: g.game_mode,
            })
            .collect()
    }
    pub(super) fn native_tell(&self, uuid: u128, line: Text) -> bool {
        let Some(host) = &self.lan else {
            return false;
        };
        let Some(g) = host.hub.guests.iter().find(|g| g.placed && g.uuid == uuid) else {
            return false;
        };
        host.hub.send_to(g.conn, &system_chat(&line));
        true
    }
    pub(super) fn native_broadcast(&self, line: Text) {
        let plain = line.plain();
        if self.options.dedicated.is_some() {
            super::dedicated::log("INFO", &plain);
        } else {
            log::info!("{plain}");
        }
        let Some(host) = &self.lan else {
            return;
        };
        let packet = system_chat(&line);
        for g in host.hub.guests.iter().filter(|g| g.placed) {
            host.hub.send_to(g.conn, &packet);
        }
    }
    pub(super) fn native_feedback(&mut self, line: Text) {
        match &mut self.chat_capture {
            Some(lines) => lines.push(line),
            None => self.system_message(line),
        }
    }
    pub(super) fn native_world_dir(&self) -> Option<PathBuf> {
        (!self.world_root.as_os_str().is_empty()).then(|| self.world_root.clone())
    }
}

fn plain(stack: &ItemStack) -> bool {
    stack.damage == 0 && stack.extra.is_none()
}

fn order(selected: usize) -> impl Iterator<Item = usize> {
    std::iter::once(selected)
        .chain(std::iter::once(OFFHAND))
        .chain((0..36).filter(move |&i| i != selected))
}
const OFFHAND: usize = 36;

fn slots<'a>(
    inventory: &'a mut [Option<ItemStack>],
    offhand: &'a mut Option<ItemStack>,
) -> Vec<&'a mut Option<ItemStack>> {
    inventory
        .iter_mut()
        .take(36)
        .chain(std::iter::once(offhand))
        .collect()
}

fn count_in(slots: &[&mut Option<ItemStack>], item: &str) -> u32 {
    slots
        .iter()
        .filter_map(|s| s.as_ref())
        .filter(|s| &*s.item == item && plain(s))
        .map(|s| s.count)
        .sum()
}

fn take_from(
    mut slots: Vec<&mut Option<ItemStack>>,
    selected: usize,
    item: &str,
    count: u32,
) -> u32 {
    let mut taken = 0;
    for i in order(selected) {
        let slot = &mut *slots[i];
        if let Some(s) = slot
            && &*s.item == item
            && plain(s)
        {
            let n = s.count.min(count - taken);
            s.count -= n;
            taken += n;
            if s.count == 0 {
                *slot = None;
            }
        }
        if taken == count {
            break;
        }
    }
    taken
}

fn room_in(slots: &[&mut Option<ItemStack>], item: &str, max: u32) -> u32 {
    slots
        .iter()
        .enumerate()
        .map(|(i, s)| match s.as_ref() {
            None if i == OFFHAND => 0,
            None => max,
            Some(s) if &*s.item == item && plain(s) => max.saturating_sub(s.count),
            Some(_) => 0,
        })
        .sum()
}

fn add_to(
    mut slots: Vec<&mut Option<ItemStack>>,
    selected: usize,
    item: &str,
    max: u32,
    count: u32,
) -> u32 {
    let mut added = 0;
    for pass in 0..2 {
        for i in order(selected) {
            let slot = &mut *slots[i];
            let n = match slot {
                Some(s) if pass == 0 && &*s.item == item && plain(s) => {
                    let n = max.saturating_sub(s.count).min(count - added);
                    s.count += n;
                    n
                }
                None if pass == 1 && i != OFFHAND => {
                    let n = max.min(count - added);
                    *slot = Some(ItemStack {
                        item: item.into(),
                        count: n,
                        damage: 0,
                        extra: None,
                    });
                    n
                }
                _ => 0,
            };
            added += n;
            if added == count {
                return added;
            }
        }
    }
    added
}

#[allow(dead_code)]
impl Game<'_> {
    pub(super) fn native_held(&self) -> Option<(String, u32, bool)> {
        let s = self.inventory.get(self.selected)?.as_ref()?;
        Some((s.item.to_string(), s.count, plain(s)))
    }
    pub(super) fn native_count_items(&mut self, item: &str) -> u32 {
        count_in(&slots(&mut self.inventory, &mut self.offhand), item)
    }
    pub(super) fn native_carried_items(&self) -> Vec<(String, u32)> {
        let mut out: Vec<(String, u32)> = Vec::new();
        let held = self
            .inventory
            .iter()
            .take(36)
            .chain(std::iter::once(&self.offhand));
        for s in held.flatten().filter(|s| plain(s)) {
            match out.iter_mut().find(|(i, _)| *i == *s.item) {
                Some((_, n)) => *n += s.count,
                None => out.push((s.item.to_string(), s.count)),
            }
        }
        out
    }
    pub(super) fn native_take_items(&mut self, item: &str, count: u32) -> u32 {
        if self.player.is_none() {
            return 0;
        }
        let selected = self.selected;
        take_from(
            slots(&mut self.inventory, &mut self.offhand),
            selected,
            item,
            count,
        )
    }
    pub(super) fn native_room_for(&mut self, item: &str) -> u32 {
        let Some(max) = self.res.items().get(item).map(|i| i.max_stack) else {
            return 0;
        };
        room_in(
            &slots(&mut self.inventory, &mut self.offhand),
            item,
            max.max(1),
        )
    }
    pub(super) fn native_give_items(&mut self, item: &str, count: u32) -> u32 {
        let Some(max) = self.res.items().get(item).map(|i| i.max_stack) else {
            return 0;
        };
        if self.player.is_none() {
            return 0;
        }
        let selected = self.selected;
        add_to(
            slots(&mut self.inventory, &mut self.offhand),
            selected,
            item,
            max.max(1),
            count,
        )
    }
}

fn system_chat(line: &Text) -> C {
    C::SystemChat(play::SystemChat {
        content: component_nbt(&line.component()),
        overlay: false,
    })
}

impl Game<'_> {
    pub(super) fn register_native_commands(&mut self, host: &mut LanHost) {
        if host.native_commands.is_some() {
            return;
        }
        let mut commands = Vec::new();
        self.native_register_commands(&mut commands);
        let (tree, levels, problems) =
            command_tree(&commands, |name| host.hub.vanilla_command(name));
        for p in problems {
            log::warn!("Vise: a native mod's command was left out: {p}");
        }
        host.hub.set_mod_commands(0, tree);
        host.native_commands = Some(levels);
    }

    pub(super) fn native_command_level(host: &LanHost, line: &str) -> Option<u8> {
        let name = line.split(' ').next().unwrap_or_default();
        host.native_commands.as_ref()?.get(name).copied()
    }

    pub(super) fn perform_native_command(&mut self, conn: u32, permission: u8, line: &str) -> bool {
        let Some(host) = &self.lan else {
            return false;
        };
        if Self::native_command_level(host, line).is_none_or(|level| level > permission) {
            return false;
        }
        let Some(g) = host.hub.guest(conn) else {
            return false;
        };
        let player = NativeServerPlayer {
            uuid: g.uuid,
            name: g.name.clone(),
            permission,
            dimension: g.dimension.clone(),
            position: g.position,
            game_mode: g.game_mode,
        };
        let mut ran = false;
        self.native_perform_command(&player, line, &mut ran);
        if !ran {
            self.native_feedback(
                Text::translate("command.unknown.command", &[])
                    .with_color(vise_client::text::RED),
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(commands: &[NativeCommand]) -> (Value, HashMap<String, u8>, Vec<String>) {
        command_tree(commands, |name| name == "give")
    }

    fn stack(item: &str, count: u32) -> Option<ItemStack> {
        Some(ItemStack {
            item: item.into(),
            count,
            damage: 0,
            extra: None,
        })
    }

    #[test]
    fn items_are_taken_from_the_hand_first_and_only_plain() {
        let mut inv = vec![None; 36];
        inv[0] = stack("minecraft:diamond", 10);
        inv[4] = stack("minecraft:diamond", 5);
        inv[8] = stack("minecraft:diamond", 7);
        inv[8].as_mut().unwrap().damage = 3;
        let mut off = stack("minecraft:diamond", 2);
        assert_eq!(
            count_in(&slots(&mut inv, &mut off), "minecraft:diamond"),
            17
        );
        assert_eq!(
            take_from(slots(&mut inv, &mut off), 4, "minecraft:diamond", 6),
            6
        );
        assert!(inv[4].is_none(), "the selected slot first");
        assert_eq!(off.as_ref().unwrap().count, 1, "then the off hand");
        assert_eq!(
            take_from(slots(&mut inv, &mut off), 4, "minecraft:diamond", 100),
            11
        );
        assert_eq!(inv[8].as_ref().unwrap().count, 7, "a damaged stack stays");
        assert!(inv[0].is_none() && off.is_none());
    }

    #[test]
    fn items_are_given_only_where_they_fit() {
        let mut inv = vec![stack("minecraft:dirt", 64); 36];
        inv[3] = stack("minecraft:diamond", 60);
        inv[9] = None;
        let mut off = None;
        assert_eq!(
            room_in(&slots(&mut inv, &mut off), "minecraft:diamond", 64),
            4 + 64
        );
        assert_eq!(
            add_to(slots(&mut inv, &mut off), 0, "minecraft:diamond", 64, 200),
            68
        );
        assert_eq!(inv[3].as_ref().unwrap().count, 64, "onto its stack first");
        assert!(off.is_none(), "never into an empty off hand (getFreeSlot)");
        assert_eq!(inv[9].as_ref().unwrap().count, 64);
        assert_eq!(
            room_in(&slots(&mut inv, &mut off), "minecraft:diamond", 64),
            0
        );
        assert_eq!(
            add_to(slots(&mut inv, &mut off), 0, "minecraft:diamond", 64, 1),
            0
        );
    }

    #[test]
    fn usages_merge_into_one_tree_with_runnable_nodes() {
        let (t, levels, problems) = tree(&[
            NativeCommand::new("balance [player:player]"),
            NativeCommand::new("pay <player:player> <amount:integer>"),
            NativeCommand::op("eco give <player:player> <amount:integer>", 3),
            NativeCommand::op("eco take <player:player> <amount:integer>", 3),
        ]);
        assert!(problems.is_empty(), "{problems:?}");
        let c = &t["children"];
        assert_eq!(c["balance"]["executable"], true);
        assert_eq!(c["balance"]["children"]["player"]["executable"], true);
        assert_eq!(
            c["balance"]["children"]["player"]["parser"],
            "minecraft:entity"
        );
        assert_eq!(c["pay"]["executable"], false);
        assert_eq!(c["pay"]["children"]["player"]["executable"], false);
        let amount = &c["pay"]["children"]["player"]["children"]["amount"];
        assert_eq!(amount["executable"], true);
        assert_eq!(amount["parser"], "brigadier:integer");
        assert!(amount.get("children").is_none());
        assert_eq!(c["eco"]["permissions"]["permission"]["level"], "admins");
        assert!(c["eco"]["children"]["give"].is_object());
        assert!(c["eco"]["children"]["take"].is_object());
        assert!(c["pay"].get("permissions").is_none());
        assert_eq!(levels["eco"], 3);
        assert_eq!(levels["pay"], 0);
    }

    #[test]
    fn a_command_takes_its_lowest_level() {
        let (t, levels, _) = tree(&[
            NativeCommand::new("warp [name:word]"),
            NativeCommand::op("warp set <name:word>", 2),
        ]);
        assert_eq!(levels["warp"], 0);
        assert!(t["children"]["warp"].get("permissions").is_none());
        let set = &t["children"]["warp"]["children"]["set"];
        assert_eq!(set["permissions"]["permission"]["level"], "gamemasters");
    }

    #[test]
    fn bad_usages_and_vanilla_names_are_left_out() {
        let (t, levels, problems) = tree(&[
            NativeCommand::new("give <x:integer>"),
            NativeCommand::new("a [x:integer] <y:integer>"),
            NativeCommand::new("b <x:text> <y:word>"),
            NativeCommand::new("c <x:colour>"),
            NativeCommand::new("<x:integer>"),
            NativeCommand::new("Bad"),
            NativeCommand::new("d <x:integer>"),
            NativeCommand::new("d <x:word>"),
        ]);
        assert_eq!(problems.len(), 7, "{problems:?}");
        assert_eq!(levels.keys().collect::<Vec<_>>(), ["d"]);
        assert_eq!(
            t["children"]["d"]["children"]["x"]["parser"],
            "brigadier:integer"
        );
        let (none, levels, _) = tree(&[]);
        assert!(none.is_null() && levels.is_empty());
    }
}
