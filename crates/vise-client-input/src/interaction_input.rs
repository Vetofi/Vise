#[derive(Default)]
pub struct InteractionInput {
    clicks: [u32; 3],
}

#[derive(Clone, Copy)]
pub enum Button {
    Attack,
    Use,
    Pick,
}

pub trait InteractionWorld {
    fn pick_hit_result(&mut self);
    fn release_hit_result(&mut self);
    fn using_item(&self) -> bool;
    fn release_using(&mut self);
    fn start_attack(&mut self) -> bool;
    fn start_use(&mut self);
    fn pick(&mut self);
    fn use_delay(&self) -> u32;
    fn continue_attack(&mut self, down: bool);
}

pub trait TargetUseWorld {
    fn use_entity(&mut self) -> bool;
    fn use_block(&mut self) -> bool;
    fn use_item(&mut self);
}

pub fn use_on_target(world: &mut impl TargetUseWorld, entity: bool) {
    let consumed = if entity {
        world.use_entity()
    } else {
        world.use_block()
    };
    if !consumed {
        world.use_item();
    }
}

impl InteractionInput {
    pub fn press(&mut self, button: Button) {
        self.clicks[button as usize] += 1;
    }

    pub fn tick(&mut self, world: &mut impl InteractionWorld, attack_down: bool, use_down: bool) {
        let [attack, use_item, pick] = std::mem::take(&mut self.clicks);
        world.pick_hit_result();
        let mut instant = false;
        if world.using_item() {
            if !use_down {
                world.release_using();
            }
        } else {
            for _ in 0..attack {
                instant |= world.start_attack();
            }
            for _ in 0..use_item {
                world.start_use();
            }
            for _ in 0..pick {
                world.pick();
            }
        }
        if use_down && world.use_delay() == 0 && !world.using_item() {
            world.start_use();
        }
        world.continue_attack(!instant && attack_down);
        world.release_hit_result();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct World {
        using: bool,
        destroying: bool,
        delay: u32,
        begin_using: bool,
        events: Vec<&'static str>,
    }
    impl InteractionWorld for World {
        fn pick_hit_result(&mut self) {}
        fn release_hit_result(&mut self) {}
        fn using_item(&self) -> bool {
            self.using
        }
        fn release_using(&mut self) {
            self.events.push("release");
            self.using = false;
        }
        fn start_attack(&mut self) -> bool {
            self.events.push("attack");
            false
        }
        fn start_use(&mut self) {
            if !self.destroying {
                self.events.push("use");
                self.delay = 4;
                self.using = self.begin_using;
            }
        }
        fn pick(&mut self) {
            self.events.push("pick");
        }
        fn use_delay(&self) -> u32 {
            self.delay
        }
        fn continue_attack(&mut self, _: bool) {
            self.events.push("continue");
        }
    }

    #[test]
    fn quick_clicks_wait_for_a_tick_and_follow_keybind_order() {
        let mut input = InteractionInput::default();
        let mut world = World::default();
        input.press(Button::Use);
        input.press(Button::Pick);
        input.press(Button::Attack);
        assert!(world.events.is_empty());
        input.tick(&mut world, false, false);
        assert_eq!(world.events, ["attack", "use", "pick", "continue"]);
        world.events.clear();
        input.tick(&mut world, false, false);
        assert_eq!(
            world.events,
            ["continue"],
            "no spurious release or repeated click"
        );
    }

    #[test]
    fn releasing_actual_item_use_discards_clicks_for_that_tick() {
        let mut input = InteractionInput::default();
        let mut world = World {
            using: true,
            ..Default::default()
        };
        for button in [Button::Attack, Button::Use, Button::Pick] {
            input.press(button);
        }
        input.tick(&mut world, true, false);
        assert_eq!(world.events, ["release", "continue"]);
    }

    #[test]
    fn holding_use_respects_the_delay_and_active_digging() {
        let mut input = InteractionInput::default();
        let mut world = World {
            destroying: true,
            ..Default::default()
        };
        input.press(Button::Use);
        input.tick(&mut world, true, true);
        assert_eq!(world.events, ["continue"]);
        world.destroying = false;
        world.events.clear();
        input.tick(&mut world, false, true);
        assert_eq!(world.events, ["use", "continue"]);
        world.events.clear();
        input.tick(&mut world, false, true);
        assert_eq!(world.events, ["continue"]);
    }

    #[test]
    fn a_quick_use_click_releases_on_the_following_tick() {
        let mut input = InteractionInput::default();
        let mut world = World {
            begin_using: true,
            ..Default::default()
        };
        input.press(Button::Use);
        input.tick(&mut world, false, false);
        assert_eq!(world.events, ["use", "continue"]);
        assert!(world.using);
        world.events.clear();
        input.tick(&mut world, false, false);
        assert_eq!(world.events, ["release", "continue"]);
    }

    #[derive(Default)]
    struct Column {
        block: bool,
        hit: Option<Option<()>>,
        placed: bool,
    }
    impl Column {
        fn target(&self) -> Option<()> {
            self.hit.unwrap_or(self.block.then_some(()))
        }
    }
    impl InteractionWorld for Column {
        fn pick_hit_result(&mut self) {
            self.hit = Some(self.target());
        }
        fn release_hit_result(&mut self) {
            self.hit = None;
        }
        fn using_item(&self) -> bool {
            false
        }
        fn release_using(&mut self) {}
        fn start_attack(&mut self) -> bool {
            if self.target().is_some() {
                self.block = false;
            }
            !self.block
        }
        fn start_use(&mut self) {
            if self.target().is_some() && !self.block {
                self.block = true;
                self.placed = true;
            }
        }
        fn pick(&mut self) {}
        fn use_delay(&self) -> u32 {
            0
        }
        fn continue_attack(&mut self, _: bool) {}
    }

    #[test]
    fn attack_and_use_in_one_tick_share_the_hit_result() {
        let mut input = InteractionInput::default();
        let mut world = Column {
            block: true,
            ..Default::default()
        };
        input.press(Button::Attack);
        input.press(Button::Use);
        input.tick(&mut world, false, false);
        assert!(world.placed, "the use acts on the block the attack broke");
        assert!(world.block);
        assert!(world.hit.is_none(), "the next pick follows the crosshair");
    }
}
