use super::Game;
use vise_assets::item_model::DisplayContext;
use vise_client::{
    block_entity_render::vertex_light,
    effects::EffectInstance,
    items::ItemStack,
    mesher::Vertex,
    particles::{ParticleOptions, ParticleType},
};

const TOTEM: &str = "minecraft:totem_of_undying";

pub(super) const TOTEM_EVENT: i8 = 35;

impl Game<'_> {
    pub(super) fn update_totem_ready(&mut self) {
        let ready = self.held().is_some_and(|s| &*s.item == TOTEM)
            || self.offhand.as_ref().is_some_and(|s| &*s.item == TOTEM);
        if let Some(p) = self.player.as_mut() {
            p.health.totem_ready = ready;
        }
    }
    pub(super) fn apply_totem(&mut self) -> Option<Body> {
        let p = self.player.as_mut()?;
        if !p.health.totem_used {
            return None;
        }
        p.health.totem_used = false;
        let _ = p.set_health(1.0, false);
        let body = Body {
            at: [p.position.x, p.position.y, p.position.z],
            width: f64::from(p.dimensions.width),
            height: f64::from(p.dimensions.height),
        };
        let slot = if self.held().is_some_and(|s| &*s.item == TOTEM) {
            Some(self.selected)
        } else {
            None
        };
        let cell = match slot {
            Some(i) => &mut self.inventory[i],
            None => &mut self.offhand,
        };
        if let Some(s) = cell.as_mut() {
            s.count -= 1;
            if s.count == 0 {
                *cell = None;
            }
        }
        self.stats.add("used", TOTEM, 1);
        self.trigger_item(
            "minecraft:used_totem",
            &vise_client::items::ItemStack::new(TOTEM, 1),
        );
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:item_interact_finish", body.at, ctx);
        self.potions.effects.clear();
        for (id, ticks, amp) in [
            ("regeneration", 900, 1),
            ("absorption", 100, 1),
            ("fire_resistance", 800, 0),
        ] {
            if let Some(e) = vise_client::brewing::effect(id) {
                self.add_player_effect(EffectInstance::new(e.id, ticks, amp));
            }
        }
        Some(body)
    }
    pub(super) fn tick_totem(&mut self) {
        let Some(body) = self.apply_totem() else {
            return;
        };
        if let Some(host) = &self.lan {
            host.hub
                .entity_event(host.host_entity, TOTEM_EVENT, body.at);
        }
        self.totem_event(body, true);
    }
    pub(super) fn remote_totem(&mut self, id: i32) {
        let own = self.remote.as_ref().is_some_and(|r| r.entity_id == id);
        let body = if own {
            self.player.as_ref().map(|p| Body {
                at: [p.position.x, p.position.y, p.position.z],
                width: f64::from(p.dimensions.width),
                height: f64::from(p.dimensions.height),
            })
        } else {
            self.mobs.iter().find(|m| m.id == id as u32).map(|m| Body {
                at: [m.position.x, m.position.y, m.position.z],
                width: f64::from(m.width()),
                height: f64::from(m.height()),
            })
        };
        if let Some(body) = body {
            self.totem_event(body, own);
        }
    }
    pub(super) fn totem_event(&mut self, body: Body, own: bool) {
        self.particles.tracking_emitter_for(
            ParticleOptions::simple(ParticleType::TotemOfUndying),
            body.at,
            body.width,
            body.height,
            30,
        );
        self.audio
            .play_unshared("minecraft:item.totem.use", Some(body.at), 1.0, 1.0);
        if own {
            self.pad_rumble(vise_client::gamepad::Rumble::Totem);
            let totem = self.find_totem();
            self.item_activation
                .activate(totem, self.particles.random());
        }
    }
    fn find_totem(&self) -> ItemStack {
        let models = &self.pack.item_renderer.models;
        [self.held(), self.offhand.as_ref()]
            .into_iter()
            .flatten()
            .find(|s| {
                models
                    .components(&vise_client::item_render::stack_view(s))
                    .get("minecraft:death_protection")
                    .is_some()
            })
            .cloned()
            .unwrap_or_else(|| ItemStack::new(TOTEM, 1))
    }
    pub(super) fn tick_item_activation(&mut self) {
        if self.player.is_some() {
            self.item_activation.tick();
        }
    }
    pub(super) fn item_activation_geometry(
        &self,
        partial: f32,
        view: glam::Mat4,
        aspect: f32,
        out: &mut Vec<Vertex>,
    ) {
        if self.ui.hide_gui {
            return;
        }
        let (Some(stack), Some(pose)) = (
            self.item_activation.active(),
            self.item_activation.pose(partial, aspect),
        ) else {
            return;
        };
        let state = self.resolve_item(stack, DisplayContext::Fixed, None, 0);
        let to_world = view.inverse();
        let (a, b) = vise_client::item_render::gui::items_3d_lights();
        let lights = (
            to_world.transform_vector3(a).normalize(),
            to_world.transform_vector3(b).normalize(),
        );
        self.submit_item_lit(&state, to_world * pose, vertex_light(0xFF), lights, out);
    }
}

#[derive(Clone, Copy)]
pub(super) struct Body {
    pub at: [f64; 3],
    width: f64,
    height: f64,
}
