#[cfg(not(target_os = "ios"))]
use crate::{DEFAULT_LIGHTBAR, Output, PLAYER_1, Vibration, is_dualsense};
use crate::{TRIGGER_OFF, TriggerEffect};
#[cfg(not(target_os = "ios"))]
use hidapi::{BusType, HidApi, HidDevice, HidResult};
use std::sync::Arc;
#[cfg(not(target_os = "ios"))]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Sender;
#[cfg(not(target_os = "ios"))]
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;
#[cfg(not(target_os = "ios"))]
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Bluetooth,
}

#[cfg(not(target_os = "ios"))]
pub struct DualSense {
    device: HidDevice,
    transport: Transport,
    vibration: Vibration,
    seq: u8,
}

pub fn bluetooth_allowed() -> bool {
    cfg!(target_os = "linux") || std::env::var_os("VISE_DUALSENSE_BT").is_some_and(|v| v == "1")
}

#[cfg(not(target_os = "ios"))]
impl DualSense {
    pub fn open(api: &HidApi) -> Option<HidResult<Self>> {
        static SKIPPED: AtomicBool = AtomicBool::new(false);
        let bluetooth = |d: &&hidapi::DeviceInfo| matches!(d.bus_type(), BusType::Bluetooth);
        let pads = api
            .device_list()
            .filter(|d| is_dualsense(d.vendor_id(), d.product_id()));
        let (wireless, wired): (Vec<_>, Vec<_>) = pads.partition(bluetooth);
        let info = match wired.first() {
            Some(info) => info,
            None if bluetooth_allowed() => wireless.first()?,
            None => {
                if !wireless.is_empty() && !SKIPPED.swap(true, Ordering::Relaxed) {
                    log::info!("Vise: no DualSense output over Bluetooth on this system");
                }
                return None;
            }
        };
        let transport = match info.bus_type() {
            BusType::Bluetooth => Transport::Bluetooth,
            _ => Transport::Usb,
        };
        Some(info.open_device(api).map(|device| {
            let mut firmware = [0; 64];
            firmware[0] = 0x20;
            let version = match device.get_feature_report(&mut firmware) {
                Ok(n) if n >= 46 => Some(u16::from_le_bytes([firmware[44], firmware[45]])),
                _ => None,
            };
            let vibration = Vibration::of(info.product_id(), version);
            DualSense {
                device,
                transport,
                vibration,
                seq: 0,
            }
        }))
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }

    pub fn write(&mut self, out: &Output) -> HidResult<()> {
        match self.transport {
            Transport::Usb => self.device.write(&out.usb(self.vibration))?,
            Transport::Bluetooth => {
                let report = out.bluetooth(self.vibration, self.seq);
                self.seq = (self.seq + 1) & 0x0f;
                self.device.write(&report)?
            }
        };
        Ok(())
    }

    pub fn read(&self, buf: &mut [u8], millis: i32) -> HidResult<usize> {
        self.device.read_timeout(buf, millis)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub triggers: [TriggerEffect; 2],
    pub lightbar: Option<[u8; 3]>,
    pub player_leds: Option<u8>,
}

impl Default for State {
    fn default() -> Self {
        State {
            triggers: [TRIGGER_OFF; 2],
            lightbar: None,
            player_leds: None,
        }
    }
}

pub type Rumble = Vec<([u8; 2], Duration)>;

#[cfg_attr(target_os = "ios", allow(dead_code))]
enum Message {
    State(State),
    Rumble(Rumble),
    Scan,
}

pub struct Link {
    tx: Option<Sender<Message>>,
    open: Arc<AtomicU8>,
    thread: Option<JoinHandle<()>>,
}

impl Link {
    #[cfg(not(target_os = "ios"))]
    pub fn start() -> Link {
        let (tx, rx) = mpsc::channel();
        let open = Arc::new(AtomicU8::new(0));
        let shown = open.clone();
        let thread = std::thread::Builder::new()
            .name("dualsense".into())
            .spawn(move || Writer::new(shown).run(rx))
            .map_err(|e| log::warn!("Vise: DualSense output: {e}"))
            .ok();
        Link {
            tx: Some(tx),
            open,
            thread,
        }
    }

    #[cfg(target_os = "ios")]
    pub fn start() -> Link {
        Link {
            tx: None,
            open: Arc::new(AtomicU8::new(0)),
            thread: None,
        }
    }

    pub fn open(&self) -> Option<Transport> {
        match self.open.load(Ordering::Relaxed) {
            1 => Some(Transport::Usb),
            2 => Some(Transport::Bluetooth),
            _ => None,
        }
    }

    pub fn scan(&self) {
        self.send(Message::Scan);
    }

    pub fn set(&self, state: State) {
        self.send(Message::State(state));
    }

    pub fn rumble(&self, rumble: Rumble) {
        self.send(Message::Rumble(rumble));
    }

    fn send(&self, message: Message) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(message);
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_os = "ios"))]
const SCAN_TRIES: u32 = 5;

#[cfg(not(target_os = "ios"))]
struct Writer {
    api: Option<HidApi>,
    pad: Option<DualSense>,
    open: Arc<AtomicU8>,
    state: State,
    sent: Option<(State, [u8; 2])>,
    lights: Lights,
    rumble: Rumble,
    step_end: Option<Instant>,
    scans_left: u32,
    next_scan: Instant,
}

#[cfg(not(target_os = "ios"))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lights {
    Waiting(Instant),
    Ready,
    Reset,
}

#[cfg(not(target_os = "ios"))]
const ANIMATION_OVER: u32 = 10_200_000;

#[cfg(not(target_os = "ios"))]
fn bluetooth_clock(report: &[u8]) -> Option<u32> {
    match report {
        [0x31, _, state @ ..] if state.len() >= 31 => {
            Some(u32::from_le_bytes(state[27..31].try_into().ok()?))
        }
        _ => None,
    }
}

#[cfg(not(target_os = "ios"))]
impl Writer {
    fn new(open: Arc<AtomicU8>) -> Self {
        Writer {
            api: None,
            pad: None,
            open,
            state: State::default(),
            sent: None,
            lights: Lights::Ready,
            rumble: Vec::new(),
            step_end: None,
            scans_left: 1,
            next_scan: Instant::now(),
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Message>) {
        loop {
            let now = Instant::now();
            if self.pad.is_none() && self.scans_left > 0 && now >= self.next_scan {
                self.scan();
            }
            let waiting = matches!(self.lights, Lights::Waiting(_)) && self.pad.is_some();
            let wake = [
                self.step_end,
                (self.pad.is_none() && self.scans_left > 0).then_some(self.next_scan),
                waiting.then(|| now + Duration::from_millis(250)),
            ]
            .into_iter()
            .flatten()
            .min();
            let message = match wake {
                Some(at) => rx.recv_timeout(at.saturating_duration_since(now)),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match message {
                Ok(Message::State(state)) => self.state = state,
                Ok(Message::Rumble(rumble)) => {
                    self.rumble = rumble;
                    self.rumble.reverse();
                    self.step_end = None;
                }
                Ok(Message::Scan) => {
                    let mut buf = [0; 80];
                    if self
                        .pad
                        .as_ref()
                        .is_some_and(|pad| pad.read(&mut buf, 0).is_err())
                    {
                        self.lost("unplugged");
                    }
                    if self.pad.is_none() {
                        self.scans_left = SCAN_TRIES;
                        self.next_scan = Instant::now();
                    }
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.watch_animation();
            self.advance_rumble();
            self.flush();
        }
        if let Some(pad) = self.pad.as_mut() {
            let release = Output {
                lightbar: self.state.lightbar.map(|_| DEFAULT_LIGHTBAR),
                player_leds: self.state.player_leds.map(|_| PLAYER_1),
                ..Output::release()
            };
            let _ = pad.write(&release);
        }
    }

    fn scan(&mut self) {
        self.scans_left -= 1;
        self.next_scan = Instant::now() + Duration::from_secs(1);
        let api = match self.api.as_mut() {
            Some(api) => api.refresh_devices().map(|()| &*api),
            None => HidApi::new().map(|api| &*self.api.insert(api)),
        };
        let found = match api {
            Ok(api) => DualSense::open(api),
            Err(e) => {
                log::warn!("Vise: DualSense output: {e}");
                None
            }
        };
        match found {
            Some(Ok(pad)) => {
                log::info!("DualSense output over {:?}", pad.transport());
                self.open
                    .store(1 + pad.transport() as u8, Ordering::Relaxed);
                self.lights = match pad.transport() {
                    Transport::Usb => Lights::Ready,
                    Transport::Bluetooth => {
                        Lights::Waiting(Instant::now() + Duration::from_secs(1))
                    }
                };
                self.pad = Some(pad);
                self.sent = None;
                self.scans_left = 0;
                self.flush();
            }
            Some(Err(e)) => {
                log::info!("Vise: the DualSense can't be opened for output: {e}");
                self.scans_left = 0;
            }
            None => {}
        }
    }

    fn watch_animation(&mut self) {
        let (Lights::Waiting(deadline), Some(pad)) = (self.lights, self.pad.as_ref()) else {
            return;
        };
        let mut buf = [0; 80];
        let mut clock = None;
        while let Ok(n) = pad.read(&mut buf, 0)
            && n > 0
        {
            clock = bluetooth_clock(&buf[..n]).or(clock);
        }
        let over = match clock {
            Some(clock) => clock >= ANIMATION_OVER,
            None => Instant::now() >= deadline,
        };
        if over {
            self.lights = Lights::Ready;
        }
    }

    fn advance_rumble(&mut self) {
        let now = Instant::now();
        match self.step_end {
            Some(end) if end > now => return,
            Some(_) => {
                self.rumble.pop();
            }
            None => {}
        }
        self.step_end = self.rumble.last().map(|&(_, length)| now + length);
    }

    fn flush(&mut self) {
        let Some(pad) = self.pad.as_mut() else {
            return;
        };
        let motors = self.rumble.last().map_or([0, 0], |(m, _)| *m);
        let mut state = self.state;
        if matches!(self.lights, Lights::Waiting(_)) {
            state.lightbar = None;
            state.player_leds = None;
        }
        let sent = self.sent;
        if sent == Some((state, motors)) {
            return;
        }
        let mut first = Output {
            rumble: motors,
            ..Output::default()
        };
        if sent.is_none() {
            first.full_power = true;
        }
        let lit = state.lightbar.is_some() || state.player_leds.is_some();
        if lit && self.lights == Lights::Ready {
            first.release_leds = true;
            self.lights = Lights::Reset;
        }
        if first.full_power || first.release_leds {
            if let Err(e) = pad.write(&first) {
                return self.lost(&e.to_string());
            }
        }
        let mut out = Output {
            rumble: motors,
            ..Output::default()
        };
        let old = sent.map(|(s, _)| s);
        if old.is_none_or(|s| s.triggers != state.triggers) {
            out.left_trigger = Some(state.triggers[0]);
            out.right_trigger = Some(state.triggers[1]);
        }
        if old.is_none_or(|s| s.lightbar != state.lightbar) {
            out.lightbar = state.lightbar;
        }
        if old.is_none_or(|s| s.player_leds != state.player_leds) {
            out.player_leds = state.player_leds;
        }
        match pad.write(&out) {
            Ok(()) => self.sent = Some((state, motors)),
            Err(e) => self.lost(&e.to_string()),
        }
    }

    fn lost(&mut self, why: &str) {
        log::info!("DualSense output closed: {why}");
        self.pad = None;
        self.open.store(0, Ordering::Relaxed);
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_clock_offset() {
        let mut report = [0; 78];
        report[0] = 0x31;
        report[2 + 27..2 + 31].copy_from_slice(&ANIMATION_OVER.to_le_bytes());
        assert_eq!(bluetooth_clock(&report), Some(10_200_000));
        report[0] = 0x01;
        assert_eq!(bluetooth_clock(&report), None);
        assert_eq!(bluetooth_clock(&[0x31, 0, 0]), None);
    }
}
