
use crate::TimerConfig;

#[derive(Clone, Debug, Default)]
pub struct Balance {
    pub ms: f64,
}

impl Balance {
    pub fn elapse(&mut self, elapsed_ms: f64, config: &TimerConfig) {
        self.ms = self.ms.min(config.max_lag_ms) + elapsed_ms;
    }

    pub fn tick_end(&mut self, tick_rate: f32, config: &TimerConfig) -> Option<f64> {
        self.ms -= 1000.0 / f64::from(tick_rate.max(1.0));
        if self.ms < -config.tolerance_ms {
            let ahead = -self.ms;
            self.ms = 0.0;
            return Some(ahead);
        }
        None
    }

    pub fn reset(&mut self) {
        self.ms = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(b: &mut Balance, server_ms: f64, per_tick: f64, seconds: f64) -> usize {
        let c = TimerConfig::default();
        let mut owed = 0.0;
        let mut flags = 0;
        for _ in 0..(seconds * 1000.0 / server_ms) as usize {
            b.elapse(server_ms, &c);
            owed += per_tick;
            while owed >= 1.0 {
                owed -= 1.0;
                flags += usize::from(b.tick_end(20.0, &c).is_some());
            }
        }
        flags
    }

    #[test]
    fn a_vanilla_client_is_never_flagged() {
        let mut b = Balance::default();
        assert_eq!(run(&mut b, 50.0, 1.0, 600.0), 0);
        assert_eq!(run(&mut b, 100.0, 2.0, 600.0), 0);
    }

    #[test]
    fn a_slow_server_tick_with_its_backlog_is_not_flagged() {
        let (mut b, c) = (Balance::default(), TimerConfig::default());
        b.elapse(3000.0, &c);
        for _ in 0..60 {
            assert_eq!(b.tick_end(20.0, &c), None);
        }
    }

    #[test]
    fn a_short_network_stall_is_not_flagged() {
        let (mut b, c) = (Balance::default(), TimerConfig::default());
        for _ in 0..16 {
            b.elapse(50.0, &c);
        }
        b.elapse(50.0, &c);
        for _ in 0..17 {
            assert_eq!(b.tick_end(20.0, &c), None);
        }
    }

    #[test]
    fn a_sped_up_client_is_flagged_again_and_again() {
        let mut b = Balance::default();
        let flags = run(&mut b, 50.0, 1.1, 60.0);
        assert!((15..=20).contains(&flags), "{flags}");
        assert!(run(&mut Balance::default(), 50.0, 2.0, 10.0) >= 25);
    }

    #[test]
    fn a_pause_cant_be_saved_for_a_burst() {
        let (mut b, c) = (Balance::default(), TimerConfig::default());
        for _ in 0..200 {
            b.elapse(50.0, &c);
        }
        let flagged = (0..60).any(|_| b.tick_end(20.0, &c).is_some());
        assert!(flagged);
    }
}
