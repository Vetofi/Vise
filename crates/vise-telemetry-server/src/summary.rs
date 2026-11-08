use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Value, json};

pub const FORMAT: &str = "vise-benchmark-summary";
pub const MIN_INSTALLS: usize = 3;
pub const SCENES: [&str; 4] = ["village", "forest", "ocean", "mountain"];
pub const FIELDS: [&str; 7] = [
    "avg_fps",
    "low_1pct_fps",
    "p95_ms",
    "world_ms",
    "chunk_load_ms",
    "rss_peak_mb",
    "rss_steady_mb",
];

pub struct Upload {
    pub install: Vec<u8>,
    pub version: String,
    pub report: Value,
}

fn trim(s: &str) -> String {
    s.replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn game(app: &Value) -> Option<&'static str> {
    match app.get("name").and_then(Value::as_str) {
        Some("vise-client") => Some("vise"),
        Some("minecraft") => {
            let mods = app.get("mods").and_then(Value::as_array);
            let plain = mods.is_none_or(|m| {
                m.iter()
                    .all(|m| m.get("name").and_then(Value::as_str) == Some("fabric-loader"))
            });
            plain.then_some("vanilla")
        }
        _ => None,
    }
}

pub fn rejection(report: &Value) -> Option<&'static str> {
    if report.get("format").and_then(Value::as_str) != Some("vise-benchmark-report")
        || report.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Some("not a version 1 benchmark report");
    }
    if report.get("kind").and_then(Value::as_str) != Some("game") {
        return Some("not a game run");
    }
    if game(&report["app"]).is_none() {
        return Some("not Vise or plain vanilla");
    }
    let c = &report["conditions"];
    let scenario = c.get("scenario").and_then(Value::as_str).unwrap_or("");
    if !(scenario == "standard" || scenario == "flight" || scenario.starts_with("perf_bench:")) {
        return Some("not the standard flight");
    }
    if c.get("vsync") != Some(&Value::Bool(false)) {
        return Some("VSync on or unknown");
    }
    if c.get("fps_cap").is_some_and(|v| !v.is_null()) {
        return Some("frame rate capped");
    }
    if c.get("offscreen") == Some(&Value::Bool(true)) {
        return Some("offscreen");
    }
    if !c.get("render_distance").is_some_and(Value::is_number) {
        return Some("no render distance");
    }
    let phases: BTreeSet<&str> = report["frames"]
        .as_array()
        .map(|f| f.iter().filter_map(|f| f["phase"].as_str()).collect())
        .unwrap_or_default();
    if !SCENES.iter().all(|s| phases.contains(s)) {
        return Some("not every scene");
    }
    let named = |v: &Value| v.as_str().is_some_and(|s| !trim(s).is_empty());
    if !named(&report["hardware"]["gpu"]["name"]) || !named(&report["hardware"]["cpu"]) {
        return Some("no GPU or CPU name");
    }
    None
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

pub fn median(xs: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m]
    } else {
        (v[m - 1] + v[m]) / 2.0
    })
}

pub fn round(field: &str, x: f64) -> f64 {
    if field.ends_with("_ms") && x < 10.0 {
        (x * 10.0).round() / 10.0
    } else {
        x.round()
    }
}

fn number(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

pub fn figures(report: &Value) -> [Option<f64>; 7] {
    let frames = report["frames"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let avg = |key: &str| {
        mean(
            &frames
                .iter()
                .filter_map(|f| f[key].as_f64())
                .collect::<Vec<_>>(),
        )
    };
    let metrics = report["metrics"].as_object();
    let metric = |name: &str| metrics.and_then(|m| m.get(name)?.get("mean")?.as_f64());
    let loads: Vec<f64> = metrics
        .map(|m| {
            m.iter()
                .filter(|(k, _)| {
                    k.strip_prefix("chunks.")
                        .and_then(|k| k.strip_suffix(".load_ms"))
                        .is_some_and(|scene| !scene.contains('.'))
                })
                .filter_map(|(_, v)| v.get("mean")?.as_f64())
                .collect()
        })
        .unwrap_or_default();
    [
        avg("avg_fps"),
        avg("low_1pct_fps"),
        avg("p95_ms"),
        metric("startup.world_ms"),
        mean(&loads),
        metric("memory.rss_peak_mb"),
        metric("memory.rss_steady_mb"),
    ]
}

fn newest_first(a: &str, b: &str) -> std::cmp::Ordering {
    let n = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    n(b).cmp(&n(a))
}

#[derive(Default)]
struct Group {
    installs: HashMap<Vec<u8>, Vec<[Option<f64>; 7]>>,
    classes: [BTreeMap<&'static str, BTreeSet<Vec<u8>>>; 5],
    reports: usize,
    versions: BTreeSet<String>,
}

pub fn groups(uploads: &[Upload], min_installs: usize) -> Vec<Value> {
    let mut by: BTreeMap<(String, String, i64, &str), Group> = BTreeMap::new();
    for u in uploads {
        if u.install.is_empty() || rejection(&u.report).is_some() {
            continue;
        }
        let r = &u.report;
        let key = (
            trim(r["hardware"]["gpu"]["name"].as_str().unwrap_or_default()),
            trim(r["hardware"]["cpu"].as_str().unwrap_or_default()),
            r["conditions"]["render_distance"]
                .as_f64()
                .unwrap_or_default() as i64,
            game(&r["app"]).unwrap_or_default(),
        );
        let g = by.entry(key).or_default();
        g.reports += 1;
        if !u.version.is_empty() {
            g.versions.insert(u.version.clone());
        }
        g.installs
            .entry(u.install.clone())
            .or_default()
            .push(figures(r));
        for (dim, class) in crate::classes::of_report(r).into_iter().enumerate() {
            if let Some(class) = class {
                g.classes[dim]
                    .entry(class)
                    .or_default()
                    .insert(u.install.clone());
            }
        }
    }
    let mut out: Vec<(usize, Value)> = by
        .into_iter()
        .filter(|(_, g)| g.installs.len() >= min_installs)
        .map(|((gpu, cpu, rd, game), g)| {
            let per_install: Vec<[Option<f64>; 7]> = g
                .installs
                .values()
                .map(|runs| {
                    std::array::from_fn(|i| median(&runs.iter().filter_map(|r| r[i]).collect::<Vec<_>>()))
                })
                .collect();
            let medians: serde_json::Map<String, Value> = FIELDS
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let m = median(&per_install.iter().filter_map(|r| r[i]).collect::<Vec<_>>());
                    (f.to_string(), m.map_or(Value::Null, |m| number(round(f, m))))
                })
                .collect();
            let mut versions: Vec<String> = g.versions.into_iter().collect();
            versions.sort_by(|a, b| newest_first(a, b));
            versions.truncate(3);
            let installs = g.installs.len();
            let breakdowns = breakdowns(&g.classes, min_installs);
            (
                installs,
                json!({
                    "gpu": gpu, "cpu": cpu, "render_distance": rd, "game": game, "source": "players",
                    "installs": installs, "reports": g.reports, "versions": versions, "medians": medians,
                    "breakdowns": breakdowns,
                }),
            )
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.0));
    out.into_iter().map(|(_, v)| v).collect()
}

pub fn breakdowns(
    classes: &[BTreeMap<&'static str, BTreeSet<Vec<u8>>>; 5],
    min_installs: usize,
) -> Value {
    let mut out = serde_json::Map::new();
    for (dim, by_class) in crate::classes::DIMENSIONS.iter().zip(classes) {
        let mut cells: Vec<(&str, usize)> = by_class
            .iter()
            .map(|(c, set)| (*c, set.len()))
            .filter(|(_, n)| *n >= min_installs)
            .collect();
        cells.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        if !cells.is_empty() {
            out.insert(
                dim.to_string(),
                cells
                    .into_iter()
                    .map(|(class, installs)| json!({"class": class, "installs": installs}))
                    .collect(),
            );
        }
    }
    Value::Object(out)
}

pub fn summary(uploads: &[Upload], min_installs: usize, generated: &str) -> Value {
    json!({
        "format": FORMAT, "version": 1, "generated": generated, "min_installs": min_installs,
        "groups": groups(uploads, min_installs),
    })
}
