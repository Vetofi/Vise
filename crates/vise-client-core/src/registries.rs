use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct Registries {
    pub permission: u8,
    pub items: Vec<String>,
    pub item_tags: Vec<String>,
    pub blocks: BTreeMap<String, Vec<(String, Vec<String>)>>,
    pub block_tags: Vec<String>,
    pub summonable: Vec<String>,
    pub entity_types: Vec<String>,
    pub effects: Vec<String>,
    pub enchantments: Vec<String>,
    pub structures: Vec<String>,
    pub biomes: Vec<String>,
    pub dimensions: Vec<String>,
    pub clocks: Vec<String>,
    pub timelines: Vec<String>,
    pub time_markers: Vec<String>,
    pub advancements: Vec<String>,
    pub game_rules: Vec<(String, bool)>,
    pub players: Vec<String>,
    pub custom_completions: std::collections::BTreeSet<String>,
    pub objectives: Vec<String>,
    pub teams: Vec<String>,
    pub tracked: Vec<String>,
    pub functions: Vec<String>,
    pub storages: Vec<String>,
    pub bossbars: Vec<String>,
    pub predicates: Vec<String>,
    pub item_modifiers: Vec<String>,
    pub schedules: Vec<String>,
    pub stopwatches: Vec<String>,
    pub recipes: Vec<String>,
    pub sounds: Vec<String>,
    pub damage_types: Vec<String>,
    pub attributes: Vec<String>,
    pub loot_tables: Vec<String>,
    pub templates: Vec<String>,
    pub random_sequences: Vec<String>,
    pub datapacks_enabled: Vec<String>,
    pub datapacks_disabled: Vec<String>,
    pub function_tags: Vec<String>,
    pub target_block: Option<[i32; 3]>,
    pub selected_entities: Vec<String>,
    pub target_point: Option<[String; 3]>,
    pub by_registry: BTreeMap<String, RegistryIds>,
    pub block_entities: std::collections::BTreeSet<String>,
    pub block_tag_members: BTreeMap<String, Vec<String>>,
    pub persistent_components: Vec<String>,
    pub learnable_recipes: Vec<String>,
    pub advancement_criteria: BTreeMap<String, Vec<String>>,
    pub clock_time_markers: BTreeMap<String, Vec<String>>,
    pub trigger_objectives: Vec<String>,
    pub trigger_scores: BTreeMap<String, BTreeMap<String, bool>>,
}
#[derive(Clone, Debug, Default)]
pub struct RegistryIds {
    pub ids: Vec<String>,
    pub tags: Vec<String>,
}
const EMPTY: &[String] = &[];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomCompletionAction {
    Add,
    Remove,
    Set,
}
impl CustomCompletionAction {
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Remove,
            2 => Self::Set,
            _ => Self::Add,
        }
    }
}
pub fn modify_custom_completions(
    set: &mut std::collections::BTreeSet<String>,
    action: CustomCompletionAction,
    entries: &[String],
) {
    match action {
        CustomCompletionAction::Add => set.extend(entries.iter().cloned()),
        CustomCompletionAction::Remove => {
            for e in entries {
                set.remove(e);
            }
        }
        CustomCompletionAction::Set => {
            set.clear();
            set.extend(entries.iter().cloned());
        }
    }
}
impl Registries {
    pub fn custom_tab_suggestions(&self) -> std::collections::BTreeSet<&str> {
        self.players
            .iter()
            .chain(&self.custom_completions)
            .map(String::as_str)
            .collect()
    }
    pub fn fill_from_export(&mut self, r: &serde_json::Value) {
        fn strings(v: &serde_json::Value) -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        }
        let fill = |field: &mut Vec<String>, key: &str| {
            if field.is_empty() {
                *field = strings(&r[key]);
            }
        };
        fill(&mut self.effects, "effects");
        fill(&mut self.enchantments, "enchantments");
        fill(&mut self.attributes, "attributes");
        fill(&mut self.damage_types, "damage_types");
        fill(&mut self.templates, "templates");
        fill(&mut self.persistent_components, "persistent_components");
        fill(&mut self.learnable_recipes, "learnable_recipes");
        const RELOADED: [&str; 5] = [
            "minecraft:advancement",
            "minecraft:recipe",
            "minecraft:loot_table",
            "minecraft:predicate",
            "minecraft:item_modifier",
        ];
        for (name, entry) in r["by_registry"].as_object().into_iter().flatten() {
            if RELOADED.contains(&name.as_str()) || self.by_registry.contains_key(name) {
                continue;
            }
            self.by_registry.insert(
                name.clone(),
                RegistryIds {
                    ids: strings(&entry["ids"]),
                    tags: strings(&entry["tags"]),
                },
            );
        }
        if self.block_entities.is_empty() {
            self.block_entities = strings(&r["block_entities"]).into_iter().collect();
        }
        if self.block_tag_members.is_empty() {
            for (tag, members) in r["block_tag_members"].as_object().into_iter().flatten() {
                self.block_tag_members.insert(tag.clone(), strings(members));
            }
        }
        if self.clock_time_markers.is_empty() {
            for (clock, markers) in r["clock_time_markers"].as_object().into_iter().flatten() {
                self.clock_time_markers
                    .insert(clock.clone(), strings(markers));
            }
        }
        if self.advancement_criteria.is_empty() {
            for (id, criteria) in r["advancement_criteria"].as_object().into_iter().flatten() {
                self.advancement_criteria
                    .insert(id.clone(), strings(criteria));
            }
        }
    }
    pub fn ids(&self, registry: &str) -> &[String] {
        if let Some(r) = self.by_registry.get(registry) {
            return &r.ids;
        }
        match registry {
            "minecraft:item" => &self.items,
            "minecraft:entity_type" => &self.entity_types,
            "minecraft:mob_effect" => &self.effects,
            "minecraft:enchantment" => &self.enchantments,
            "minecraft:worldgen/structure" => &self.structures,
            "minecraft:worldgen/biome" => &self.biomes,
            "minecraft:world_clock" => &self.clocks,
            "minecraft:timeline" => &self.timelines,
            "minecraft:attribute" => &self.attributes,
            "minecraft:damage_type" => &self.damage_types,
            "minecraft:sound_event" => &self.sounds,
            "minecraft:predicate" => &self.predicates,
            "minecraft:item_modifier" => &self.item_modifiers,
            "minecraft:loot_table" => &self.loot_tables,
            "minecraft:recipe" => &self.recipes,
            "minecraft:advancement" => &self.advancements,
            _ => EMPTY,
        }
    }
    pub fn tags(&self, registry: &str) -> &[String] {
        if let Some(r) = self.by_registry.get(registry) {
            return &r.tags;
        }
        match registry {
            "minecraft:item" => &self.item_tags,
            "minecraft:block" => &self.block_tags,
            _ => EMPTY,
        }
    }
}
