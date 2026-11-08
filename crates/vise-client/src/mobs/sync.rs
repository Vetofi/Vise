use super::{
    Mob, Species, allay, armadillo, axolotl, breeze, camel, copper_golem, creaking, equine, frog,
    goat, golem, guardian, piglin, ravager, shulker, sniffer, tame, undead, villager, warden,
};

pub const STANDING: u8 = 0;
pub const SLEEPING: u8 = 2;
pub const CROUCHING: u8 = 5;
pub const LONG_JUMPING: u8 = 6;
pub const CROAKING: u8 = 8;
pub const USING_TONGUE: u8 = 9;
pub const SITTING: u8 = 10;
pub const ROARING: u8 = 11;
pub const SNIFFING: u8 = 12;
pub const EMERGING: u8 = 13;
pub const DIGGING: u8 = 14;
pub const SLIDING: u8 = 15;
pub const SHOOTING: u8 = 16;
pub const INHALING: u8 = 17;

pub fn pose_id(m: &Mob) -> u8 {
    match m.species() {
        Species::Warden => warden::pose_id(m),
        Species::Breeze => breeze::pose_id(m),
        Species::Camel | Species::CamelHusk if camel::sitting(m) => SITTING,
        Species::Frog | Species::Cat | Species::Ocelot | Species::Goat => m.extra.pose,
        Species::Villager if villager::sleeping(m) => SLEEPING,
        _ => STANDING,
    }
}

pub fn set_pose(m: &mut Mob, id: u8) {
    let old = std::mem::replace(&mut m.extra.pose, id);
    match m.species() {
        Species::Frog if old != id => frog::pose_changed(m),
        Species::Warden => warden::set_pose_id(m, id),
        Species::Breeze => breeze::set_pose_id(m, id),
        _ => {}
    }
}

pub fn client_tick(m: &mut Mob, game_time: i64) {
    match m.species() {
        Species::Warden => warden::client_tick(m),
        Species::Sniffer => sniffer::animate(m),
        Species::Breeze => breeze::ai_step(m),
        Species::Camel | Species::CamelHusk => camel::client_tick(m, game_time),
        Species::Shulker => shulker::client_tick(m),
        Species::Ravager => ravager::client_tick(m),
        Species::Hoglin | Species::Zoglin => piglin::client_tick(m),
        Species::IronGolem => golem::client_tick(m),
        Species::Creaking => creaking::animate(m),
        Species::Allay => {
            m.anim.vo = m.anim.v;
            allay::animate(m);
        }
        Species::Axolotl => axolotl::tick_animation(m),
        Species::Guardian | Species::ElderGuardian => {
            m.anim.vo = m.anim.v;
            guardian::tail_and_spikes(m);
        }
        Species::CopperGolem if !m.no_ai => {
            copper_golem::animate(m);
        }
        Species::Armadillo => armadillo::tick(m),
        Species::Frog => frog::tick(m),
        Species::Goat => goat::tick(m),
        Species::Sheep if m.extra.anim_ticks > 0 => m.extra.anim_ticks -= 1,
        s if equine::grazes(s) => equine::client_tick(m),
        Species::Drowned => undead::drowned_client_tick(m),
        Species::Wolf if m.alive() => tame::interest_tick(m),
        _ => {}
    }
}

pub fn entity_event(m: &mut Mob, event: u8) {
    if m.species() == Species::Armadillo {
        armadillo::entity_event(m, event);
    }
    if m.species() == Species::Goat {
        goat::entity_event(m, event);
    }
    let age = m.age as f32;
    match m.species() {
        Species::Warden => match event {
            4 => {
                m.anim.states[warden::ROAR].stop();
                m.anim.states[warden::ATTACK].start(age);
            }
            61 => m.anim.v[warden::TENDRIL] = 10.0,
            62 => m.anim.states[warden::SONIC_BOOM].start(age),
            _ => {}
        },
        Species::Ravager => ravager::entity_event(m, event),
        Species::Hoglin | Species::Zoglin => piglin::client_event(m, event),
        Species::IronGolem => golem::client_event(m, event),
        Species::Creaking => creaking::entity_event(m, event),
        Species::Wolf => tame::entity_event(m, event),
        Species::Sheep if event == 10 => m.extra.anim_ticks = 40,
        _ => {}
    }
}
