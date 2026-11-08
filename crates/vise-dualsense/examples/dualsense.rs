use vise_dualsense::device::{DualSense, Link, State};
use vise_dualsense::{Output, TRIGGER_OFF};
use std::time::{Duration, Instant};

const GYRO: usize = 0x0f;
const ACCEL: usize = 0x15;
const RIGHT_TRIGGER_STATUS: usize = 0x29;
const LEFT_TRIGGER_STATUS: usize = 0x2a;
const TRIGGER_EFFECTS: usize = 0x2f;
const POWER: usize = 0x34;

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let api = hidapi::HidApi::new().expect("hidapi");
    let mut pad = match DualSense::open(&api) {
        Some(Ok(pad)) => pad,
        Some(Err(e)) => panic!("the DualSense is there but can't be opened: {e}"),
        None => panic!("no DualSense found"),
    };
    println!("DualSense over {:?}", pad.transport());
    let all = what == "all";
    if all || what == "status" {
        status(&pad);
    }
    if all || what == "rumble" {
        rumble(&mut pad);
    }
    if all || what == "lightbar" {
        lightbar(&mut pad);
    }
    if all || what == "leds" {
        leds(&mut pad);
    }
    if all || what == "triggers" {
        triggers(&mut pad);
    }
    if what == "watch" {
        for _ in 0..150 {
            let (accel, gyro) = shake(&pad, Duration::from_millis(200));
            if accel > 100.0 {
                println!(
                    "  {:?} shaking: accel {accel:7.1} gyro {gyro:7.1}",
                    std::time::SystemTime::now()
                );
            }
        }
    }
    drop(pad);
    if all || what == "link" {
        link();
    }
}

fn input(pad: &DualSense) -> Vec<u8> {
    let mut buf = [0; 80];
    while pad.read(&mut buf, 0).is_ok_and(|n| n > 0) {}
    loop {
        let n = pad.read(&mut buf, 1000).expect("read");
        match buf[0] {
            0x01 if n >= 64 => return buf[1..n].to_vec(),
            0x31 if n >= 78 => return buf[2..n].to_vec(),
            _ => {}
        }
    }
}

fn status(pad: &DualSense) {
    let d = input(pad);
    println!(
        "  sticks {:?} triggers {:?} battery {}%",
        &d[0..4],
        &d[4..6],
        (d[POWER] & 0x0f).min(10) * 10
    );
    trigger_status(pad);
}

fn trigger_status(pad: &DualSense) {
    let d = input(pad);
    println!(
        "  triggers report: right {:02x} left {:02x} effects {:02x}",
        d[RIGHT_TRIGGER_STATUS], d[LEFT_TRIGGER_STATUS], d[TRIGGER_EFFECTS]
    );
}

fn shake(pad: &DualSense, time: Duration) -> (f64, f64) {
    let end = Instant::now() + time;
    let mut samples = Vec::new();
    while Instant::now() < end {
        let d = input(pad);
        let axis =
            |at: usize, i: usize| f64::from(i16::from_le_bytes([d[at + 2 * i], d[at + 2 * i + 1]]));
        samples.push([0, 1, 2, 3, 4, 5].map(|i| {
            if i < 3 {
                axis(ACCEL, i)
            } else {
                axis(GYRO, i - 3)
            }
        }));
    }
    let deviation = |range: std::ops::Range<usize>| -> f64 {
        range
            .map(|i| {
                let n = samples.len() as f64;
                let mean = samples.iter().map(|s| s[i]).sum::<f64>() / n;
                (samples.iter().map(|s| (s[i] - mean).powi(2)).sum::<f64>() / n).sqrt()
            })
            .sum()
    };
    (deviation(0..3), deviation(3..6))
}

fn write(pad: &mut DualSense, out: Output) {
    pad.write(&out).expect("write");
}

fn rumble(pad: &mut DualSense) {
    let still = shake(pad, Duration::from_millis(400));
    println!(
        "  still:            accel {:7.1} gyro {:7.1}",
        still.0, still.1
    );
    for (name, motors) in [
        ("left (heavy) 50%", [128, 0]),
        ("left (heavy) 100%", [255, 0]),
        ("right (light) 50%", [0, 128]),
        ("right (light) 100%", [0, 255]),
        ("both 100%", [255, 255]),
    ] {
        write(
            pad,
            Output {
                rumble: motors,
                ..Output::default()
            },
        );
        let s = shake(pad, Duration::from_millis(400));
        println!("  {name:18} accel {:7.1} gyro {:7.1}", s.0, s.1);
        write(
            pad,
            Output {
                rumble: [0, 0],
                ..Output::default()
            },
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    for i in 1..=3 {
        let after = shake(pad, Duration::from_millis(300));
        println!(
            "  stopped ({i}):      accel {:7.1} gyro {:7.1}",
            after.0, after.1
        );
    }
}

fn lightbar(pad: &mut DualSense) {
    write(
        pad,
        Output {
            release_leds: true,
            ..Output::default()
        },
    );
    for (name, rgb) in [
        ("red", [255, 0, 0]),
        ("green", [0, 255, 0]),
        ("blue", [0, 0, 255]),
        ("Vise violet", [0x8b, 0x5c, 0xf6]),
    ] {
        println!("  lightbar {name}");
        write(
            pad,
            Output {
                lightbar: Some(rgb),
                ..Output::default()
            },
        );
        std::thread::sleep(Duration::from_millis(800));
    }
}

fn leds(pad: &mut DualSense) {
    for (name, leds) in [
        ("player 1", 0b00100),
        ("player 2", 0b01010),
        ("player 3", 0b10101),
        ("player 4", 0b11011),
        ("all", 0b11111),
        ("none", 0),
    ] {
        println!("  player LEDs {name}");
        write(
            pad,
            Output {
                player_leds: Some(leds | vise_dualsense::PLAYER_LEDS_INSTANT),
                ..Output::default()
            },
        );
        std::thread::sleep(Duration::from_millis(600));
    }
    write(
        pad,
        Output {
            player_leds: Some(0b00100),
            ..Output::default()
        },
    );
}

fn triggers(pad: &mut DualSense) {
    use vise_dualsense::trigger;
    for (name, effect) in [
        ("bow drawn halfway", trigger::feedback(1, 4)),
        ("bow drawn fully", trigger::feedback(1, 8)),
        ("crossbow click", trigger::weapon(3, 6, 8)),
        ("mining stone", trigger::vibration(1, 3, 30)),
        ("mining obsidian", trigger::vibration(1, 5, 30)),
        ("boost wall", trigger::feedback(4, 6)),
        ("off", TRIGGER_OFF),
    ] {
        write(
            pad,
            Output {
                left_trigger: Some(effect),
                right_trigger: Some(effect),
                ..Output::default()
            },
        );
        std::thread::sleep(Duration::from_millis(150));
        print!("  {name:18}");
        trigger_status(pad);
        if effect != TRIGGER_OFF {
            std::thread::sleep(Duration::from_millis(1850));
        }
    }
}

fn link() {
    println!("link:");
    let link = Link::start();
    let start = Instant::now();
    while link.open().is_none() && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("  open over {:?} after {:?}", link.open(), start.elapsed());
    link.set(State {
        triggers: [TRIGGER_OFF, vise_dualsense::trigger::weapon(2, 6, 6)],
        lightbar: Some([0x8b, 0x5c, 0xf6]),
        player_leds: Some(0b00100),
    });
    link.rumble(vec![
        ([200, 0], Duration::from_millis(120)),
        ([0, 160], Duration::from_millis(200)),
    ]);
    std::thread::sleep(Duration::from_millis(600));
    drop(link);
    println!("  dropped: triggers released");
}
