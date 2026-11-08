use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Perf,
    Crash,
    Bug,
    Benchmark,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Perf, Kind::Crash, Kind::Bug, Kind::Benchmark];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Perf => "perf",
            Kind::Crash => "crash",
            Kind::Bug => "bug",
            Kind::Benchmark => "benchmark",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    pub fn max_bytes(self) -> usize {
        match self {
            Kind::Perf => 4 * 1024,
            Kind::Crash => 32 * 1024,
            Kind::Bug => 80 * 1024,
            Kind::Benchmark => 128 * 1024,
        }
    }

    pub fn daily_cap(self) -> u32 {
        match self {
            Kind::Perf => 12,
            Kind::Crash => 5,
            Kind::Bug => 5,
            Kind::Benchmark => 10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Launcher,
    Game,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub day: u32,
    pub source: Source,
    pub version: String,
    #[serde(flatten)]
    pub payload: Payload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Payload {
    Perf(Perf),
    Crash(Crash),
    Bug(Bug),
    Benchmark(serde_json::Value),
}

impl Item {
    pub fn new(source: Source, version: impl Into<String>, payload: Payload) -> Self {
        Self {
            day: crate::today(),
            source,
            version: version.into(),
            payload,
        }
    }

    pub fn kind(&self) -> Kind {
        match self.payload {
            Payload::Perf(_) => Kind::Perf,
            Payload::Crash(_) => Kind::Crash,
            Payload::Bug(_) => Kind::Bug,
            Payload::Benchmark(_) => Kind::Benchmark,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let size = serde_json::to_vec(self).map_or(usize::MAX, |j| j.len());
        let kind = self.kind();
        if size > kind.max_bytes() {
            return Err(format!(
                "a {} item of {size} bytes is over its {} bytes",
                kind.name(),
                kind.max_bytes()
            ));
        }
        if self.version.len() > 64 {
            return Err("the version is too long".into());
        }
        match &self.payload {
            Payload::Bug(bug) if bug.text.trim().is_empty() => {
                Err("a bug report needs text".into())
            }
            Payload::Bug(bug) if bug.text.chars().count() > Bug::MAX_TEXT => Err(format!(
                "a bug report is at most {} characters",
                Bug::MAX_TEXT
            )),
            Payload::Benchmark(report) if !report.is_object() => {
                Err("a benchmark report is a JSON object".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct System {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub cpu: String,
    pub threads: u16,
    pub cores: u16,
    pub cpu_base_mhz: u32,
    pub cpu_boost_mhz: u32,
    pub ram_gb: u16,
    pub gpu: String,
    pub gpu_vendor: String,
    pub gpu_kind: String,
    pub vram_mb: u32,
    pub driver: String,
    pub backend: String,
    pub storage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
    pub power: String,
    pub form: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub scale: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub render_distance: u8,
    pub simulation_distance: u8,
    pub max_fps: u16,
    pub vsync: bool,
    pub fullscreen: bool,
    pub graphics: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Perf {
    pub system: System,
    pub settings: Settings,
    pub seconds: u32,
    pub frames: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<Summary>,
    pub peak_ram_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_title_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_core_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_rest_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_world_ms: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crash {
    pub system: System,
    pub error: String,
    pub stack: String,
    pub thread: String,
    pub details: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bug {
    pub system: System,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

impl Bug {
    pub const MAX_TEXT: usize = 4000;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub count: u64,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
    pub buckets: Vec<u32>,
}

pub const BUCKET_EDGES_MS: [f32; 13] = [
    4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.7, 20.0, 25.0, 33.3, 50.0, 100.0, 250.0,
];

#[derive(Clone, Debug)]
pub struct Timings {
    tenths: Vec<u32>,
    count: u64,
    max_ms: f32,
}

impl Default for Timings {
    fn default() -> Self {
        Self::new()
    }
}

impl Timings {
    pub const FINE_MS: usize = 500;

    pub fn new() -> Self {
        Self {
            tenths: vec![0; Self::FINE_MS * 10 + 1],
            count: 0,
            max_ms: 0.0,
        }
    }

    pub fn record(&mut self, ms: f32) {
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        let bin = ((ms * 10.0) as usize).min(self.tenths.len() - 1);
        self.tenths[bin] = self.tenths[bin].saturating_add(1);
        self.count += 1;
        self.max_ms = self.max_ms.max(ms);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    fn percentile(&self, p: f64) -> f32 {
        let rank = ((self.count as f64 * p).ceil() as u64).max(1);
        let mut seen = 0u64;
        for (bin, &n) in self.tenths.iter().enumerate() {
            seen += u64::from(n);
            if seen >= rank {
                return if bin + 1 == self.tenths.len() {
                    self.max_ms
                } else {
                    ((bin + 1) as f32 / 10.0).min(self.max_ms)
                };
            }
        }
        self.max_ms
    }

    pub fn summary(&self) -> Summary {
        let mut buckets = vec![0u32; BUCKET_EDGES_MS.len() + 1];
        for (bin, &n) in self.tenths.iter().enumerate() {
            let ms = bin as f32 / 10.0;
            let bucket = BUCKET_EDGES_MS
                .iter()
                .position(|&edge| ms < edge - 0.01)
                .unwrap_or(BUCKET_EDGES_MS.len());
            buckets[bucket] = buckets[bucket].saturating_add(n);
        }
        let round = |ms: f32| (ms * 10.0).round() / 10.0;
        Summary {
            count: self.count,
            p50_ms: round(self.percentile(0.50)),
            p95_ms: round(self.percentile(0.95)),
            p99_ms: round(self.percentile(0.99)),
            max_ms: round(self.max_ms),
            buckets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_tagged_by_kind_and_read_back() {
        let item = Item::new(
            Source::Game,
            "v0.4.2",
            Payload::Bug(Bug {
                text: "Water flickers".into(),
                ..Bug::default()
            }),
        );
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["kind"], "bug");
        assert_eq!(json["source"], "game");
        assert_eq!(json["data"]["text"], "Water flickers");
        assert_eq!(serde_json::from_value::<Item>(json).unwrap(), item);
    }

    #[test]
    fn a_benchmark_report_is_carried_as_it_is() {
        let report = serde_json::json!({"schema": "vise-benchmark-report/1", "scenes": []});
        let item = Item::new(
            Source::Launcher,
            "0.1.5",
            Payload::Benchmark(report.clone()),
        );
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["data"], report);
        assert!(item.validate().is_ok());
        let not_object = Item::new(Source::Launcher, "0.1.5", Payload::Benchmark(1.into()));
        assert!(not_object.validate().is_err());
    }

    #[test]
    fn oversized_items_are_refused() {
        let bug = |text: String| {
            Item::new(
                Source::Game,
                "v1",
                Payload::Bug(Bug {
                    text,
                    ..Bug::default()
                }),
            )
        };
        assert!(bug("x".repeat(Bug::MAX_TEXT)).validate().is_ok());
        assert!(bug("x".repeat(Bug::MAX_TEXT + 1)).validate().is_err());
        assert!(bug("  ".into()).validate().is_err());
        let huge = Item::new(
            Source::Game,
            "v1",
            Payload::Crash(Crash {
                stack: "frame\n".repeat(10_000),
                ..Crash::default()
            }),
        );
        assert!(huge.validate().unwrap_err().contains("over its"));
    }

    #[test]
    fn timings_give_percentiles_and_buckets() {
        let mut t = Timings::new();
        (0..90).for_each(|_| t.record(16.6));
        (0..9).for_each(|_| t.record(30.0));
        t.record(400.0);
        t.record(f32::NAN);
        let s = t.summary();
        assert_eq!(s.count, 100);
        assert_eq!(s.p50_ms, 16.7);
        assert_eq!(s.p95_ms, 30.1);
        assert_eq!(s.p99_ms, 30.1);
        assert_eq!(s.max_ms, 400.0);
        assert_eq!(s.buckets.len(), 14);
        assert_eq!(s.buckets[6], 90, "14–16.7 ms");
        assert_eq!(s.buckets[9], 9, "25–33.3 ms");
        assert_eq!(s.buckets[13], 1, "250 ms and more");
        assert_eq!(s.buckets.iter().sum::<u32>(), 100);
    }

    #[test]
    fn hitches_beyond_the_fine_range_keep_their_maximum() {
        let mut t = Timings::new();
        t.record(2_000.0);
        let s = t.summary();
        assert_eq!((s.p50_ms, s.max_ms), (2_000.0, 2_000.0));
        assert_eq!(s.buckets[13], 1);
    }

    #[test]
    fn a_session_summary_is_small() {
        let mut t = Timings::new();
        (0..200_000).for_each(|i| t.record(5.0 + (i % 300) as f32 / 10.0));
        let perf = Item::new(
            Source::Game,
            "v0.4.2",
            Payload::Perf(Perf {
                frames: t.summary(),
                ticks: Some(t.summary()),
                ..Perf::default()
            }),
        );
        let size = serde_json::to_vec(&perf).unwrap().len();
        assert!(size < 1024, "{size} bytes");
        assert!(perf.validate().is_ok());
    }
}
