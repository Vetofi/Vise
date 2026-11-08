use crate::items::ItemStack;
use vise_world::nbt::Tag;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct UseCooldown {
    pub seconds: f32,
    pub group: Option<Arc<str>>,
}
impl UseCooldown {
    pub fn ticks(&self) -> i32 {
        (self.seconds * 20.0) as i32
    }
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let seconds = v.get("seconds")?.as_f64()? as f32;
        Some(Self {
            seconds,
            group: v
                .get("cooldown_group")
                .and_then(|g| g.as_str())
                .map(Arc::from),
        })
    }
    pub fn from_tag(t: &Tag) -> Option<Self> {
        let Tag::Compound(c) = t else {
            return None;
        };
        let text = |k: &str| match c.get(&vise_world::nbt::JavaString::from(k)) {
            Some(Tag::String(s)) => Some(s.to_string_lossy()),
            _ => None,
        };
        let seconds = match c.get(&vise_world::nbt::JavaString::from("seconds")) {
            Some(Tag::Float(bits)) => f32::from_bits(*bits),
            Some(Tag::Double(bits)) => f64::from_bits(*bits) as f32,
            _ => return None,
        };
        Some(Self {
            seconds,
            group: text("cooldown_group").map(Arc::from),
        })
    }
}

pub fn component(stack: &ItemStack, default: Option<&UseCooldown>) -> Option<UseCooldown> {
    stack
        .data()
        .and_then(|d| d.other("minecraft:use_cooldown"))
        .and_then(UseCooldown::from_tag)
        .or_else(|| default.cloned())
}

pub fn group(stack: &ItemStack, default: Option<&UseCooldown>) -> Arc<str> {
    component(stack, default)
        .and_then(|c| c.group)
        .unwrap_or_else(|| Arc::clone(&stack.item))
}

#[derive(Clone, Debug, Default)]
pub struct Cooldowns {
    tick_count: i32,
    entries: HashMap<Arc<str>, (i32, i32)>,
}
impl Cooldowns {
    pub fn percent(&self, group: &str, partial_tick: f32) -> f32 {
        let Some((start, end)) = self.entries.get(group) else {
            return 0.0;
        };
        let duration = end.wrapping_sub(*start) as f32;
        let remaining = *end as f32 - (self.tick_count as f32 + partial_tick);
        (remaining / duration).clamp(0.0, 1.0)
    }
    pub fn is_on_cooldown(&self, group: &str) -> bool {
        self.percent(group, 0.0) > 0.0
    }
    pub fn add(&mut self, group: Arc<str>, ticks: i32) {
        self.entries.insert(
            group,
            (self.tick_count, self.tick_count.wrapping_add(ticks)),
        );
    }
    pub fn tick(&mut self) -> Vec<Arc<str>> {
        self.tick_count = self.tick_count.wrapping_add(1);
        let done: Vec<Arc<str>> = self
            .entries
            .iter()
            .filter(|(_, (_, end))| *end <= self.tick_count)
            .map(|(group, _)| Arc::clone(group))
            .collect();
        self.entries.retain(|_, (_, end)| *end > self.tick_count);
        done
    }
    pub fn remove(&mut self, group: &str) {
        self.entries.remove(group);
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stack(item: &str) -> ItemStack {
        ItemStack::new(item, 1)
    }

    #[test]
    fn the_percent_falls_from_one_to_zero_over_the_entry() {
        let mut c = Cooldowns::default();
        c.add(Arc::from("minecraft:ender_pearl"), 20);
        assert!(c.is_on_cooldown("minecraft:ender_pearl"));
        assert_eq!(c.percent("minecraft:ender_pearl", 0.0), 1.0);
        for _ in 0..10 {
            assert!(c.tick().is_empty());
        }
        assert_eq!(c.percent("minecraft:ender_pearl", 0.0), 0.5);
        assert_eq!(c.percent("minecraft:ender_pearl", 0.5), 0.475);
        for _ in 0..9 {
            c.tick();
        }
        assert!(c.is_on_cooldown("minecraft:ender_pearl"));
        assert_eq!(c.tick(), [Arc::<str>::from("minecraft:ender_pearl")]);
        assert!(!c.is_on_cooldown("minecraft:ender_pearl"));
        assert!(c.is_empty());
        assert_eq!(c.percent("minecraft:ender_pearl", 0.0), 0.0);
    }

    #[test]
    fn the_group_comes_from_the_stack_then_the_item() {
        let pearl = stack("minecraft:ender_pearl");
        let default = UseCooldown::from_json(&json!({"seconds": 1.0})).expect("the component");
        assert_eq!(&*group(&pearl, Some(&default)), "minecraft:ender_pearl");
        let mut grouped = stack("minecraft:ender_pearl");
        grouped.data_mut().set_other(
            "minecraft:use_cooldown",
            Some(Tag::compound([
                ("seconds", Tag::Float(2.0f32.to_bits())),
                ("cooldown_group", Tag::String("minecraft:pearls".into())),
            ])),
        );
        let c = component(&grouped, Some(&default)).expect("the stack's component");
        assert_eq!(c.ticks(), 40);
        assert_eq!(&*group(&grouped, Some(&default)), "minecraft:pearls");
        assert!(component(&stack("minecraft:dirt"), None).is_none());
    }
}
