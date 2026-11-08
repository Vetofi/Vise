use super::Game;
use super::commands::Target;
use vise_client::{
    effects,
    lan::attributes::{add_modifier, generic_syncable, instance, operation_named},
    mobs::{
        Mob, Species,
        finalize::{self, Attr, Op},
    },
};
use vise_protocol::packets::play::{AttributeModifier, AttributeSnapshot};
use std::sync::Arc;

fn slot_in_group(group: &str, slot: &str) -> bool {
    group == slot
        || group == "any"
        || (group == "hand" && matches!(slot, "mainhand" | "offhand"))
        || (group == "armor" && matches!(slot, "feet" | "legs" | "chest" | "head" | "body"))
}

impl Game<'_> {
    pub(super) fn mob_knockback_resistance(&self, m: &Mob) -> f64 {
        let e = &m.extra;
        let slots = [
            ("mainhand", &e.main_hand),
            ("offhand", &e.off_hand),
            ("feet", &e.feet),
            ("legs", &e.legs),
            ("chest", &e.chest),
            ("head", &e.head),
        ];
        let mut extra = Vec::new();
        for (slot, item) in slots {
            let Some(info) = item.as_ref().and_then(|i| self.items().get(i)) else {
                continue;
            };
            for (attribute, _, amount, op, group) in &info.modifiers {
                if attribute == "minecraft:knockback_resistance" && slot_in_group(group, slot) {
                    let op = match operation_named(op) {
                        1 => Op::AddMultipliedBase,
                        2 => Op::AddMultipliedTotal,
                        _ => Op::AddValue,
                    };
                    extra.push((*amount, op));
                }
            }
        }
        finalize::value_with(m, Attr::KnockbackResistance, &extra)
    }

    fn synced_attribute(&self, entity: &str, name: &str) -> Option<i32> {
        let catalog = &self.motion.attributes;
        let id = catalog.id(name)?;
        (catalog.attribute(id).syncable && catalog.default_base(entity, id).is_some())
            .then(|| id.registry_id())
    }

    fn attribute_default(&self, entity: &str, name: &str) -> f64 {
        let catalog = &self.motion.attributes;
        catalog
            .id(name)
            .and_then(|id| catalog.default_base(entity, id))
            .unwrap_or(0.0)
    }

    fn equipment_attributes<'a>(
        &self,
        entity: &str,
        base: impl Fn(&str) -> Option<f64>,
        slots: impl IntoIterator<Item = (&'static str, Option<&'a Arc<str>>)>,
        out: &mut Vec<AttributeSnapshot>,
    ) {
        for (slot, item) in slots {
            let Some(info) = item.and_then(|i| self.items().get(i)) else {
                continue;
            };
            for (attribute, id, amount, op, group) in &info.modifiers {
                if !slot_in_group(group, slot) {
                    continue;
                }
                let Some(a) = self.synced_attribute(entity, attribute) else {
                    continue;
                };
                let start =
                    base(attribute).unwrap_or_else(|| self.attribute_default(entity, attribute));
                add_modifier(
                    out,
                    a,
                    start,
                    AttributeModifier {
                        id: id.clone(),
                        amount: *amount,
                        operation: operation_named(op),
                    },
                );
            }
        }
    }

    pub(super) fn mob_attributes(&self, m: &Mob, out: &mut Vec<AttributeSnapshot>) {
        let kind = m.kind.id.as_str();
        if let Some(a) = self.synced_attribute(kind, "minecraft:movement_speed") {
            instance(out, a, finalize::base_value(m, Attr::MovementSpeed));
        }
        if let Some(s) = m.extra.attributes.as_deref() {
            for (attr, v) in &s.bases {
                if let Some(a) = self.synced_attribute(kind, &format!("minecraft:{}", attr.name()))
                {
                    instance(out, a, *v).base = *v;
                }
            }
            for x in &s.modifiers {
                if let Some(a) =
                    self.synced_attribute(kind, &format!("minecraft:{}", x.attr.name()))
                {
                    add_modifier(
                        out,
                        a,
                        finalize::base_value(m, x.attr),
                        AttributeModifier {
                            id: x.id.to_owned(),
                            amount: x.amount,
                            operation: match x.op {
                                Op::AddValue => 0,
                                Op::AddMultipliedBase => 1,
                                Op::AddMultipliedTotal => 2,
                            },
                        },
                    );
                }
            }
        }
        let rolled = matches!(
            m.species(),
            Species::Horse | Species::SkeletonHorse | Species::ZombieHorse
        );
        let jump_base = m
            .extra
            .attributes
            .as_deref()
            .is_some_and(|s| s.bases.iter().any(|(a, _)| *a == Attr::JumpStrength));
        if rolled
            && !jump_base
            && m.extra.vector[0] > 0.0
            && let Some(a) = self.synced_attribute(kind, "minecraft:jump_strength")
        {
            instance(out, a, m.extra.vector[0]).base = m.extra.vector[0];
        }
        if let (Some(a), Some((amount, Op::AddMultipliedBase))) = (
            self.synced_attribute(kind, "minecraft:movement_speed"),
            finalize::transient(m, Attr::MovementSpeed),
        ) {
            add_modifier(
                out,
                a,
                finalize::base_value(m, Attr::MovementSpeed),
                AttributeModifier {
                    id: "minecraft:baby".into(),
                    amount,
                    operation: 1,
                },
            );
        }
        if let (true, Some(a)) = (
            m.extra.sprinting,
            self.synced_attribute(kind, "minecraft:movement_speed"),
        ) {
            add_modifier(
                out,
                a,
                finalize::base_value(m, Attr::MovementSpeed),
                AttributeModifier {
                    id: "minecraft:sprinting".into(),
                    amount: f64::from(0.3f32),
                    operation: 2,
                },
            );
        }
        let base = |name: &str| {
            Attr::ALL
                .into_iter()
                .find(|a| name.strip_prefix("minecraft:") == Some(a.name()))
                .map(|a| finalize::base_value(m, a))
        };
        for &(effect, _, amplifier) in &m.extra.effects {
            let Some((attribute, id, amount, op)) =
                effects::effect_modifier(effect.trim_start_matches("minecraft:"))
            else {
                continue;
            };
            let Some(a) = self.synced_attribute(kind, attribute) else {
                continue;
            };
            let start = base(attribute).unwrap_or_else(|| self.attribute_default(kind, attribute));
            add_modifier(
                out,
                a,
                start,
                AttributeModifier {
                    id: id.to_owned(),
                    amount: amount * f64::from(amplifier + 1),
                    operation: operation_named(op),
                },
            );
        }
        let e = &m.extra;
        self.equipment_attributes(
            kind,
            base,
            [
                ("mainhand", e.main_hand.as_ref()),
                ("offhand", e.off_hand.as_ref()),
                ("feet", e.feet.as_ref()),
                ("legs", e.legs.as_ref()),
                ("chest", e.chest.as_ref()),
                ("head", e.head.as_ref()),
            ],
            out,
        );
        self.generic_attributes_of(Target::Mob(m.id), out);
    }

    pub(super) fn stand_attributes(
        &self,
        id: u32,
        equipment: &[Option<vise_client::items::ItemStack>; 6],
    ) -> Vec<AttributeSnapshot> {
        const KIND: &str = "minecraft:armor_stand";
        let mut out = Vec::new();
        if let Some(a) = self.synced_attribute(KIND, "minecraft:movement_speed") {
            instance(
                &mut out,
                a,
                self.attribute_default(KIND, "minecraft:movement_speed"),
            );
        }
        let slots = vise_client::entities::ArmorStand::SLOTS
            .into_iter()
            .zip(equipment.iter().map(|s| s.as_ref().map(|s| &s.item)));
        self.equipment_attributes(KIND, |_| None, slots, &mut out);
        self.generic_attributes_of(Target::Stand(id), &mut out);
        out
    }

    pub(super) fn update_player_attributes(&mut self) {
        let creative = self.mode_id() == 1;
        let Some(p) = self.player.as_mut() else {
            return;
        };
        if let Some(key) = p.attributes.key("minecraft:movement_speed") {
            let _ = p.attributes.get_instance(Some(key));
        }
        for (attribute, id, amount) in [
            (
                "minecraft:block_interaction_range",
                "minecraft:creative_mode_block_range",
                0.5,
            ),
            (
                "minecraft:entity_interaction_range",
                "minecraft:creative_mode_entity_range",
                2.0,
            ),
        ] {
            let Some(key) = p.attributes.key(attribute) else {
                continue;
            };
            if !matches!(p.attributes.get_instance(Some(key.clone())), Ok(Some(_))) {
                continue;
            }
            let has = p.attributes.has_modifier(Some(key), id).unwrap_or(false);
            if has != creative {
                let _ = p.set_modifier(attribute, id, amount, "add_value", creative);
            }
        }
    }

    fn generic_attributes_of(&self, t: Target, out: &mut Vec<AttributeSnapshot>) {
        let Some(map) = self.cmds.attributes.get(&t) else {
            return;
        };
        for s in generic_syncable(map) {
            let (attribute, base) = (s.attribute, s.base);
            *instance(out, attribute, base) = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::slot_in_group;

    #[test]
    fn slot_groups_cover_their_slots() {
        assert!(slot_in_group("head", "head"));
        assert!(slot_in_group("armor", "chest"));
        assert!(!slot_in_group("armor", "mainhand"));
        assert!(slot_in_group("hand", "offhand"));
        assert!(slot_in_group("any", "feet"));
        assert!(!slot_in_group("mainhand", "offhand"));
    }
}
