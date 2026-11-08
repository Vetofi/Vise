use super::{
    super::{Game, Screen},
    Pads,
};
use vise_client::gamepad::{self, Button, GlyphStyle, PadState, Rumble, TriggerFeel};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Devices {
    gilrs: Option<gilrs::Gilrs>,
    started: bool,
    active: Option<gilrs::GamepadId>,
    rumbles: Vec<(gilrs::ff::Effect, Instant)>,
    dualsense: bool,
    output: Option<vise_dualsense::Link>,
    haptics: Option<vise_dualsense::haptics::Haptics>,
    haptics_tried: bool,
    shown: Option<vise_dualsense::device::State>,
}

fn gilrs_button(button: Button) -> gilrs::Button {
    use gilrs::Button as G;
    match button {
        Button::South => G::South,
        Button::East => G::East,
        Button::West => G::West,
        Button::North => G::North,
        Button::LeftBumper => G::LeftTrigger,
        Button::RightBumper => G::RightTrigger,
        Button::LeftTrigger => G::LeftTrigger2,
        Button::RightTrigger => G::RightTrigger2,
        Button::Select => G::Select,
        Button::Start => G::Start,
        Button::LeftStick => G::LeftThumb,
        Button::RightStick => G::RightThumb,
        Button::DPadUp => G::DPadUp,
        Button::DPadDown => G::DPadDown,
        Button::DPadLeft => G::DPadLeft,
        Button::DPadRight => G::DPadRight,
    }
}

fn is_dualsense(pad: &gilrs::Gamepad) -> bool {
    matches!((pad.vendor_id(), pad.product_id()),
        (Some(vendor), Some(product)) if vise_dualsense::is_dualsense(vendor, product))
}

impl Pads {
    fn start(&mut self) {
        if self.devices.started {
            return;
        }
        self.devices.started = true;
        self.devices.gilrs = match gilrs::Gilrs::new() {
            Ok(g) => {
                for (_, pad) in g.gamepads() {
                    log::info!("Controller connected: {}", pad.name());
                }
                Some(g)
            }
            Err(gilrs::Error::NotImplemented(_)) => None,
            Err(e) => {
                log::warn!("Vise: controllers are unavailable: {e}");
                None
            }
        };
    }

    pub(super) fn read(&mut self) -> PadState {
        self.start();
        let Some(gilrs) = self.devices.gilrs.as_mut() else {
            return PadState::default();
        };
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
            use gilrs::EventType as E;
            match event {
                E::Connected => {
                    log::info!("Controller connected: {}", gilrs.gamepad(id).name());
                    if let Some(output) = &self.devices.output {
                        output.scan();
                    }
                }
                E::Disconnected => {
                    log::info!("Controller disconnected: {}", gilrs.gamepad(id).name());
                    if self.devices.active == Some(id) {
                        self.devices.active = None;
                    }
                    if let Some(output) = &self.devices.output {
                        output.scan();
                    }
                }
                E::ButtonPressed(..) => self.devices.active = Some(id),
                E::AxisChanged(_, value, _) | E::ButtonChanged(_, value, _)
                    if value.abs() > 0.5 =>
                {
                    self.devices.active = Some(id)
                }
                _ => {}
            }
        }
        let Some(pad) = self
            .devices
            .active
            .and_then(|id| gilrs.connected_gamepad(id))
        else {
            self.devices.dualsense = gilrs.gamepads().any(|(_, pad)| is_dualsense(&pad));
            return PadState::default();
        };
        self.style = GlyphStyle::of_vendor(pad.vendor_id());
        self.devices.dualsense = is_dualsense(&pad);
        let mut state = PadState::default();
        for button in Button::ALL {
            if !matches!(button, Button::LeftTrigger | Button::RightTrigger) {
                state.set(button, pad.is_pressed(gilrs_button(button)));
            }
        }
        let axis = |a| pad.value(a);
        use gilrs::Axis as A;
        state.left = [axis(A::LeftStickX), axis(A::LeftStickY)];
        state.right = [axis(A::RightStickX), axis(A::RightStickY)];
        let trigger = |b: gilrs::Button| {
            let value = pad.button_data(b).map_or(0.0, |d| d.value());
            if pad.is_pressed(b) {
                value.max(1.0)
            } else {
                value
            }
        };
        state.triggers = [
            trigger(gilrs::Button::LeftTrigger2),
            trigger(gilrs::Button::RightTrigger2),
        ];
        state
    }
}

impl Game<'_> {
    pub(super) fn pad_output_off(&mut self) {
        self.pad.devices.output = None;
        self.pad.devices.shown = None;
    }

    pub(super) fn pad_output_frame(&mut self) {
        if self.pad.devices.dualsense
            && self.pad.in_use
            && self.focused
            && self.pad.devices.output.is_none()
        {
            self.pad.devices.output = Some(vise_dualsense::Link::start());
        }
        self.pad_haptics();
        self.pad_show();
        let now = Instant::now();
        self.pad.devices.rumbles.retain(|(_, end)| *end > now);
    }

    fn pad_output(&self) -> Option<&vise_dualsense::Link> {
        let output = self.pad.devices.output.as_ref()?;
        (self.pad.devices.dualsense && self.focused && output.open().is_some()).then_some(output)
    }

    fn pad_show(&mut self) {
        if self.pad.devices.output.is_none() {
            return;
        }
        let playing = self.pad.devices.dualsense
            && self.focused
            && self.title.is_none()
            && self.screen == Screen::Playing
            && !self.pause;
        let (using, attacking) = if playing {
            self.trigger_feels()
        } else {
            (TriggerFeel::Free, TriggerFeel::Free)
        };
        let in_world = self.title.is_none();
        let lightbar = match self.player.as_mut().filter(|_| in_world) {
            Some(p) => {
                let max = p.attribute("minecraft:max_health").unwrap_or(20.0) as f32;
                let health = p.health.health() / max.max(1.0);
                self.settings
                    .controller
                    .lightbar
                    .colour(health, self.dim.id())
            }
            None => self.pad.devices.shown.and_then(|s| s.lightbar),
        };
        let lights = self.settings.controller.lightbar != gamepad::Lightbar::Off;
        let state = vise_dualsense::device::State {
            triggers: self.settings.controller.trigger_effects(using, attacking),
            lightbar: lightbar.filter(|_| lights),
            player_leds: lights.then_some(vise_dualsense::PLAYER_1),
        };
        if self.pad.devices.shown != Some(state)
            && let Some(output) = &self.pad.devices.output
        {
            output.set(state);
            self.pad.devices.shown = Some(state);
        }
    }

    fn trigger_feels(&self) -> (TriggerFeel, TriggerFeel) {
        let using = match self.using {
            Some((slot, ticks)) => match self.stack_in(slot) {
                Some(s) if &*s.item == "minecraft:bow" => {
                    TriggerFeel::Draw(gamepad::bow_power(ticks))
                }
                Some(s) if &*s.item == "minecraft:trident" => {
                    TriggerFeel::Draw(ticks as f32 / 10.0)
                }
                Some(s) if &*s.item == "minecraft:crossbow" => {
                    TriggerFeel::Draw(ticks as f32 / Self::charge_ticks(s).max(1) as f32)
                }
                Some(s) if self.blocks_attacks(s).is_some() => TriggerFeel::Brace,
                _ => TriggerFeel::Free,
            },
            None => self.idle_use_feel(),
        };
        let attacking = match &self.mining {
            Some(m) if !self.creative => {
                let state = self.world.block(m.pos[0], m.pos[1], m.pos[2]);
                TriggerFeel::Texture(self.res.facts().get(state).hardness)
            }
            _ => TriggerFeel::Free,
        };
        (using, attacking)
    }

    fn idle_use_feel(&self) -> TriggerFeel {
        let gliding = self.player.as_ref().is_some_and(|p| p.fall_flying);
        let mount = self
            .riding
            .and_then(|id| self.mobs.iter().find(|m| m.id == id));
        let mount = mount.map_or("", |m| m.kind.id.as_str());
        let feel = |stack: Option<&vise_client::items::ItemStack>| match stack
            .map(|s| (s, &*s.item))
        {
            Some((s, "minecraft:crossbow")) if s.data().is_some_and(|d| !d.charged.is_empty()) => {
                TriggerFeel::Click
            }
            Some((_, "minecraft:firework_rocket")) if gliding => TriggerFeel::Boost,
            Some((_, "minecraft:carrot_on_a_stick")) if mount == "minecraft:pig" => {
                TriggerFeel::Boost
            }
            Some((_, "minecraft:warped_fungus_on_a_stick")) if mount == "minecraft:strider" => {
                TriggerFeel::Boost
            }
            Some((s, _)) if self.blocks_attacks(s).is_some() => match self.shield_cooldown {
                0 => TriggerFeel::Brace,
                _ => TriggerFeel::Locked,
            },
            _ => TriggerFeel::Free,
        };
        match feel(self.stack_in(self.selected)) {
            TriggerFeel::Free => feel(self.offhand.as_ref()),
            main => main,
        }
    }

    fn pad_haptics(&mut self) {
        let usb = self.pad.devices.output.as_ref().and_then(|o| o.open())
            == Some(vise_dualsense::Transport::Usb);
        if !usb {
            self.pad.devices.haptics = None;
            self.pad.devices.haptics_tried = false;
        } else if !self.pad.devices.haptics_tried {
            self.pad.devices.haptics_tried = true;
            if std::env::var_os("VISE_DUALSENSE_HAPTICS").is_some_and(|v| v == "1")
                && std::env::var_os("VISE_NO_AUDIO").is_none()
            {
                self.pad.devices.haptics = vise_dualsense::haptics::Haptics::open();
            }
        }
    }

    pub(in crate::game) fn pad_close_output(&mut self) {
        self.pad.devices.haptics = None;
        self.pad.devices.output = None;
    }

    pub(super) fn pad_vibrate(&mut self, rumble: Rumble, pulse: gamepad::Pulse) {
        let level = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        if let Some(haptics) = self
            .pad
            .devices
            .haptics
            .as_ref()
            .filter(|_| self.pad_output().is_some())
        {
            let steps = rumble.shape().into_iter().map(|step| {
                (
                    step.strong,
                    step.weak,
                    Duration::from_millis(u64::from(step.millis)),
                )
            });
            haptics.play(&steps.collect::<Vec<_>>());
            return;
        }
        if let Some(output) = self.pad_output() {
            let steps = rumble.shape().into_iter().map(|step| {
                let length = Duration::from_millis(u64::from(step.millis));
                ([level(step.strong), level(step.weak)], length)
            });
            output.rumble(steps.collect());
            return;
        }
        let (Some(gilrs), Some(id)) = (self.pad.devices.gilrs.as_mut(), self.pad.devices.active)
        else {
            return;
        };
        if !gilrs
            .connected_gamepad(id)
            .is_some_and(|g| g.is_ff_supported())
        {
            return;
        }
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
        let length = Ticks::from_ms(pulse.millis);
        let motor = |kind| BaseEffect {
            kind,
            scheduling: Replay {
                play_for: length,
                ..Default::default()
            },
            envelope: Default::default(),
        };
        let magnitude = |v: f32| (v.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
        let effect = EffectBuilder::new()
            .add_effect(motor(BaseEffectType::Strong {
                magnitude: magnitude(pulse.strong),
            }))
            .add_effect(motor(BaseEffectType::Weak {
                magnitude: magnitude(pulse.weak),
            }))
            .repeat(Repeat::For(length))
            .gamepads(&[id])
            .finish(gilrs);
        match effect.and_then(|e| e.play().map(|()| e)) {
            Ok(effect) => {
                let end = Instant::now() + Duration::from_millis(u64::from(pulse.millis) + 50);
                self.pad.devices.rumbles.push((effect, end));
            }
            Err(e) => log::debug!("Vise: controller vibration: {e}"),
        }
    }

    pub(super) fn pad_devices_status(&self) -> Value {
        let devices = &self.pad.devices;
        let pads: Vec<Value> = devices.gilrs.as_ref().map_or(Vec::new(), |g| {
            g.gamepads()
                .map(|(id, p)| {
                    json!({
                        "name": p.name(),
                        "vendor": p.vendor_id(),
                        "product": p.product_id(),
                        "active": devices.active == Some(id),
                    })
                })
                .collect()
        });
        json!({
            "backend": devices.gilrs.is_some(),
            "controllers": pads,
            "dualsense_output": devices.output.as_ref().and_then(|o| o.open()).map(|t| format!("{t:?}")),
            "trigger_effects": devices.shown.map(|s| s.triggers),
        })
    }
}
