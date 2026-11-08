
pub struct Haptics;

impl Haptics {
    pub fn open() -> Option<Haptics> {
        None
    }

    pub fn play(&self, _steps: &[(f32, f32, std::time::Duration)]) {}

    pub fn stop(&self) {}
}
