use super::Game;
use vise_client::{
    mobs::{Species, bee, guardian},
    sound_engine::{MobSound, SoundEngine, SoundInstance, Sounds},
};

impl Game<'_> {
    pub(super) fn start_mob_sounds(&mut self) -> Vec<MobSound> {
        let game_time = self.world.game_time;
        let mobs: Vec<MobSound> = self
            .mobs
            .iter()
            .filter(|m| {
                !m.removed
                    && matches!(
                        m.species(),
                        Species::Bee | Species::Guardian | Species::ElderGuardian
                    )
            })
            .map(|m| MobSound {
                id: m.id,
                position: [m.position.x, m.position.y, m.position.z],
                speed: m.velocity.x.hypot(m.velocity.z),
                baby: m.is_baby(),
                angry: m.species() == Species::Bee
                    && bee::get(m).is_some_and(|b| b.anger_end > 0 && b.anger_end - game_time > 0),
                silent: m.silent,
                attack_scale: guardian::attack_scale(m),
            })
            .collect();
        self.guardian_sounds
            .retain(|id| mobs.iter().any(|m| m.id == *id));
        self.bee_sounds
            .retain(|id, _| mobs.iter().any(|m| m.id == *id));
        for sound in &mobs {
            let is_bee = self
                .mobs
                .iter()
                .any(|m| m.id == sound.id && m.species() == Species::Bee);
            if is_bee && self.bee_sounds.get(&sound.id) != Some(&sound.angry) {
                self.bee_sounds.insert(sound.id, sound.angry);
                let instance = SoundInstance::bee(sound, sound.angry, SoundEngine::unseeded());
                self.audio.play_instance(instance, false);
            }
        }
        mobs
    }

    pub(super) fn start_guardian_sound(&mut self, id: u32) {
        let Some(m) = self.mobs.iter().find(|m| {
            m.id == id && matches!(m.species(), Species::Guardian | Species::ElderGuardian)
        }) else {
            return;
        };
        if !self.guardian_sounds.insert(id) {
            return;
        }
        let sound = MobSound {
            id,
            position: [m.position.x, m.position.y, m.position.z],
            speed: 0.0,
            baby: false,
            angry: false,
            silent: m.silent,
            attack_scale: guardian::attack_scale(m),
        };
        let instance = SoundInstance::guardian_attack(&sound, SoundEngine::unseeded());
        self.audio.play_instance(instance, false);
    }
}
