pub mod device;
#[cfg(not(target_os = "ios"))]
pub mod haptics;
#[cfg(target_os = "ios")]
#[path = "haptics_stub.rs"]
pub mod haptics;
pub mod trigger;

pub use device::{Link, Transport};

pub const SONY: u16 = 0x054c;
pub const DUALSENSE: u16 = 0x0ce6;
pub const DUALSENSE_EDGE: u16 = 0x0df2;

pub fn is_dualsense(vendor: u16, product: u16) -> bool {
    vendor == SONY && matches!(product, DUALSENSE | DUALSENSE_EDGE)
}

pub const USB_REPORT_ID: u8 = 0x02;
pub const USB_REPORT_LEN: usize = 1 + COMMON_LEN;
pub const BT_REPORT_ID: u8 = 0x31;
pub const BT_REPORT_LEN: usize = 78;
const BT_TAG: u8 = 0x10;
const BT_CRC_SEED: u8 = 0xa2;
const COMMON_LEN: usize = 47;

const FLAGS0: usize = 0;
const FLAGS1: usize = 1;
const MOTOR_RIGHT: usize = 2;
const MOTOR_LEFT: usize = 3;
const RIGHT_TRIGGER: usize = 10;
const LEFT_TRIGGER: usize = 21;
const ATTENUATION: usize = 36;
const FLAGS2: usize = 38;
const LIGHTBAR_SETUP: usize = 41;
const PLAYER_LEDS: usize = 43;
const LIGHTBAR: usize = 44;

const FLAG0_COMPATIBLE_VIBRATION: u8 = 1 << 0;
const FLAG0_HAPTICS_SELECT: u8 = 1 << 1;
const FLAG0_RIGHT_TRIGGER: u8 = 1 << 2;
const FLAG0_LEFT_TRIGGER: u8 = 1 << 3;
const FLAG1_LIGHTBAR: u8 = 1 << 2;
const FLAG1_RELEASE_LEDS: u8 = 1 << 3;
const FLAG1_PLAYER_LEDS: u8 = 1 << 4;
const FLAG1_ATTENUATION: u8 = 1 << 6;
const FLAG2_LIGHTBAR_SETUP: u8 = 1 << 1;
const FLAG2_VIBRATION_V2: u8 = 1 << 2;
const LIGHTBAR_LIGHT_OUT: u8 = 1 << 1;
pub const PLAYER_LEDS_INSTANT: u8 = 1 << 5;
pub const PLAYER_1: u8 = 1 << 2;
pub const DEFAULT_LIGHTBAR: [u8; 3] = [0, 0, 128];

pub type TriggerEffect = [u8; 11];
pub const TRIGGER_OFF: TriggerEffect = [0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Output {
    pub rumble: [u8; 2],
    pub left_trigger: Option<TriggerEffect>,
    pub right_trigger: Option<TriggerEffect>,
    pub lightbar: Option<[u8; 3]>,
    pub player_leds: Option<u8>,
    pub release_leds: bool,
    pub full_power: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vibration {
    Legacy,
    #[default]
    V2,
}

impl Vibration {
    pub fn of(product: u16, update_version: Option<u16>) -> Self {
        if product == DUALSENSE_EDGE || update_version.is_some_and(|v| v >= 0x0224) {
            Vibration::V2
        } else {
            Vibration::Legacy
        }
    }
}

impl Output {
    pub fn release() -> Self {
        Output {
            rumble: [0, 0],
            left_trigger: Some(TRIGGER_OFF),
            right_trigger: Some(TRIGGER_OFF),
            ..Output::default()
        }
    }

    pub fn common(&self, vibration: Vibration) -> [u8; COMMON_LEN] {
        let mut c = [0; COMMON_LEN];
        if self.rumble != [0, 0] {
            c[FLAGS0] |= FLAG0_HAPTICS_SELECT;
            let [left, right] = match vibration {
                Vibration::V2 => {
                    c[FLAGS2] |= FLAG2_VIBRATION_V2;
                    self.rumble
                }
                Vibration::Legacy => {
                    c[FLAGS0] |= FLAG0_COMPATIBLE_VIBRATION;
                    self.rumble.map(|m| m >> 1)
                }
            };
            c[MOTOR_LEFT] = left;
            c[MOTOR_RIGHT] = right;
        }
        if self.full_power {
            c[FLAGS1] |= FLAG1_ATTENUATION;
            c[ATTENUATION] = 0;
        }
        if let Some(effect) = self.right_trigger {
            c[FLAGS0] |= FLAG0_RIGHT_TRIGGER;
            c[RIGHT_TRIGGER..RIGHT_TRIGGER + 11].copy_from_slice(&effect);
        }
        if let Some(effect) = self.left_trigger {
            c[FLAGS0] |= FLAG0_LEFT_TRIGGER;
            c[LEFT_TRIGGER..LEFT_TRIGGER + 11].copy_from_slice(&effect);
        }
        if self.release_leds {
            c[FLAGS1] |= FLAG1_RELEASE_LEDS;
            c[FLAGS2] |= FLAG2_LIGHTBAR_SETUP;
            c[LIGHTBAR_SETUP] = LIGHTBAR_LIGHT_OUT;
        }
        if let Some(rgb) = self.lightbar {
            c[FLAGS1] |= FLAG1_LIGHTBAR;
            c[LIGHTBAR..LIGHTBAR + 3].copy_from_slice(&rgb);
        }
        if let Some(leds) = self.player_leds {
            c[FLAGS1] |= FLAG1_PLAYER_LEDS;
            c[PLAYER_LEDS] = leds;
        }
        c
    }

    pub fn usb(&self, vibration: Vibration) -> [u8; USB_REPORT_LEN] {
        let mut r = [0; USB_REPORT_LEN];
        r[0] = USB_REPORT_ID;
        r[1..].copy_from_slice(&self.common(vibration));
        r
    }

    pub fn bluetooth(&self, vibration: Vibration, seq: u8) -> [u8; BT_REPORT_LEN] {
        let mut r = [0; BT_REPORT_LEN];
        r[0] = BT_REPORT_ID;
        r[1] = (seq & 0x0f) << 4;
        r[2] = BT_TAG;
        r[3..3 + COMMON_LEN].copy_from_slice(&self.common(vibration));
        let crc = bt_crc(&r[..BT_REPORT_LEN - 4]);
        r[BT_REPORT_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
        r
    }
}

pub fn bt_crc(report: &[u8]) -> u32 {
    !report.iter().fold(crc32_update(!0, BT_CRC_SEED), |crc, b| {
        crc32_update(crc, *b)
    })
}

fn crc32_update(crc: u32, byte: u8) -> u32 {
    let mut crc = crc ^ u32::from(byte);
    for _ in 0..8 {
        crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        !data.iter().fold(!0, |crc, b| crc32_update(crc, *b))
    }

    #[test]
    fn crc_is_zlibs() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(bt_crc(b"123456789"), crc32(b"\xa2123456789"));
    }

    #[test]
    fn usb_report_layout() {
        let out = Output {
            rumble: [200, 40],
            right_trigger: Some([0x25, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            lightbar: Some([255, 128, 0]),
            player_leds: Some(0b00100),
            ..Output::default()
        };
        let r = out.usb(Vibration::V2);
        assert_eq!(r.len(), 48);
        assert_eq!(r[0], 0x02);
        assert_eq!(r[1], 0b0000_0110);
        assert_eq!(r[2], 0b0001_0100);
        assert_eq!(r[3..5], [40, 200]);
        assert_eq!(r[11..22], [0x25, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(r[22..33], [0; 11]);
        assert_eq!(r[39], FLAG2_VIBRATION_V2);
        assert_eq!(r[44], 0b00100);
        assert_eq!(r[45..48], [255, 128, 0]);
    }

    #[test]
    fn legacy_vibration() {
        let c = Output {
            rumble: [200, 41],
            ..Output::default()
        }
        .common(Vibration::Legacy);
        assert_eq!(c[FLAGS0], 0x03);
        assert_eq!(c[MOTOR_RIGHT..=MOTOR_LEFT], [20, 100]);
        assert_eq!(c[FLAGS2], 0);
    }

    #[test]
    fn vibration_mode_of_the_firmware() {
        assert_eq!(Vibration::of(DUALSENSE, Some(0x0223)), Vibration::Legacy);
        assert_eq!(Vibration::of(DUALSENSE, Some(0x0224)), Vibration::V2);
        assert_eq!(Vibration::of(DUALSENSE, Some(0x0641)), Vibration::V2);
        assert_eq!(Vibration::of(DUALSENSE, None), Vibration::Legacy);
        assert_eq!(Vibration::of(DUALSENSE_EDGE, None), Vibration::V2);
    }

    #[test]
    fn still_motors_send_no_rumble_flags() {
        assert_eq!(Output::default().common(Vibration::V2), [0; COMMON_LEN]);
        assert_eq!(Output::default().common(Vibration::Legacy), [0; COMMON_LEN]);
    }

    #[test]
    fn release_frees_both_triggers_and_stops_the_motors() {
        let c = Output::release().common(Vibration::V2);
        assert_eq!(c[FLAGS0], FLAG0_LEFT_TRIGGER | FLAG0_RIGHT_TRIGGER);
        assert_eq!(c[FLAGS0], 0b1100);
        assert_eq!(c[MOTOR_RIGHT..=MOTOR_LEFT], [0, 0]);
        assert_eq!(c[RIGHT_TRIGGER..RIGHT_TRIGGER + 11], TRIGGER_OFF);
        assert_eq!(c[LEFT_TRIGGER..LEFT_TRIGGER + 11], TRIGGER_OFF);
        assert_eq!(c[FLAGS1], 0);
        assert_eq!(c[FLAGS2], 0);
    }

    #[test]
    fn full_power_clears_the_attenuation() {
        let c = Output {
            full_power: true,
            ..Output::default()
        }
        .common(Vibration::V2);
        assert_eq!(c[FLAGS1], 0x40);
        assert_eq!(c[36], 0);
    }

    #[test]
    fn release_leds_resets_the_lights() {
        let mut expected = [0; BT_REPORT_LEN];
        expected[..5].copy_from_slice(&[0x31, 0x50, 0x10, 0x00, 0x08]);
        expected[3 + 38] = 0x02;
        expected[3 + 41] = 0x02;
        expected[74..].copy_from_slice(&0xc72d_0f94u32.to_le_bytes());
        let out = Output {
            release_leds: true,
            ..Output::default()
        };
        assert_eq!(out.bluetooth(Vibration::V2, 5), expected);
    }

    #[test]
    fn bluetooth_report_layout() {
        let out = Output {
            lightbar: Some([1, 2, 3]),
            ..Output::default()
        };
        let r = out.bluetooth(Vibration::V2, 0x13);
        assert_eq!(r.len(), 78);
        assert_eq!(r[..3], [0x31, 0x30, 0x10]);
        assert_eq!(r[3..50], out.common(Vibration::V2));
        assert_eq!(r[50..74], [0; 24]);
        let crc = u32::from_le_bytes(r[74..].try_into().unwrap());
        assert_eq!(crc, crc32(&[&[0xa2], &r[..74]].concat()));
        assert_ne!(out.bluetooth(Vibration::V2, 1)[74..], r[74..]);
    }

    #[test]
    fn host_defaults() {
        assert_eq!(PLAYER_1, 0x04);
        assert_eq!(DEFAULT_LIGHTBAR, [0, 0, 128]);
        assert_eq!(PLAYER_LEDS_INSTANT, 0x20);
    }

    #[test]
    fn dualsense_ids() {
        assert!(is_dualsense(0x054c, 0x0ce6));
        assert!(is_dualsense(0x054c, 0x0df2));
        assert!(!is_dualsense(0x054c, 0x09cc));
    }
}
