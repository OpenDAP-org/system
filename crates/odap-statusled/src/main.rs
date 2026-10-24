//! The status LED (T1 controls spec §2): of every state that applies, it shows the one with the
//! highest priority, in the IEC 60073 colour and pattern the spec gives it, on whatever LED the
//! device description names (`indicators`, role `status`), RGB or one colour.
//!
//! Where each state comes from:
//! - The battery and the charger: the kernel's `power_supply` class, followed through its
//!   uevents.
//! - Booting: from the start until the shell's first frame (`/run/opendap/shell/ui-ready`).
//! - What other services know (a fault, an update, recovery, eject first, pairing, activity): a
//!   file each in `/run/opendap/led/`, present while the state lasts. A developer takes the LED
//!   below priority 5 with `/run/opendap/led/developer` (`r g b [pattern]`, §2.2 rule 1). These
//!   files stand in for the `org.opendap.hw.Led` interface until the bus has it.
//!
//! No wakeups while nothing changes: uevents and inotify wake it, and a pattern's timer runs only
//! while a blinking or breathing state shows.

use std::time::Duration;

/// How a state lights the LED (§2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Solid,
    /// A slow fade in and out, 2 s period.
    Breathing,
    /// 1 Hz, 50 % duty.
    SlowBlink,
    /// 4 Hz, 50 % duty.
    FastBlink,
    /// One 100 ms flash every 3 s (activity, §2.2 state 10).
    Pulse,
}

impl Pattern {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "solid" => Pattern::Solid,
            "breathing" => Pattern::Breathing,
            "slow-blink" => Pattern::SlowBlink,
            "fast-blink" => Pattern::FastBlink,
            "pulse" => Pattern::Pulse,
            _ => return None,
        })
    }

    /// The brightness, 0 to 1, `t` into the pattern, and how long it stays so (`None`: for
    /// good).
    pub fn at(self, t: Duration) -> (f32, Option<Duration>) {
        let ms = t.as_millis() as u64;
        let square = |period: u64, on: u64| {
            let p = ms % period;
            if p < on {
                (1.0, Some(Duration::from_millis(on - p)))
            } else {
                (0.0, Some(Duration::from_millis(period - p)))
            }
        };
        match self {
            Pattern::Solid => (1.0, None),
            Pattern::SlowBlink => square(1000, 500),
            Pattern::FastBlink => square(250, 125),
            Pattern::Pulse => square(3000, 100),
            // 25 steps a second: smooth to the eye, and only while it breathes.
            Pattern::Breathing => {
                let phase = (ms % 2000) as f32 / 2000.0;
                ((1.0 - (phase * std::f32::consts::TAU).cos()) / 2.0, Some(Duration::from_millis(40)))
            }
        }
    }
}

/// What the LED shows: a colour (r, g, b) and its pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Show {
    pub rgb: [u8; 3],
    pub pattern: Pattern,
}

// IEC 60073 colours (§2): red danger or fault, amber warning or abnormal, green normal, blue
// action required, white neutral information. Amber on an RGB LED is red with some green.
const RED: [u8; 3] = [255, 0, 0];
const AMBER: [u8; 3] = [255, 96, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255, 255, 255];

/// Battery thresholds, in percent. Proposed (Android's defaults: a warning at 15 %, critical at
/// 5 %); the controls spec gives none yet.
pub const LOW_PERCENT: u8 = 15;
pub const CRITICAL_PERCENT: u8 = 5;

/// Every state that may apply now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inputs {
    pub fault: bool,
    pub updating: bool,
    pub recovery: bool,
    pub eject_first: bool,
    pub pairing: bool,
    pub activity: bool,
    pub booting: bool,
    /// The battery's charge, if there is a battery.
    pub battery: Option<u8>,
    pub charging: bool,
    /// On external power.
    pub external: bool,
    pub developer: Option<Show>,
}

/// The state with the highest priority that applies, as the LED shows it (§2.2); `None` is
/// normal operation: dark.
pub fn decide(i: &Inputs) -> Option<Show> {
    let show = |rgb, pattern| Some(Show { rgb, pattern });
    let discharging = !i.charging && !i.external;
    let critical = discharging && i.battery.is_some_and(|b| b <= CRITICAL_PERCENT);
    let low = discharging && i.battery.is_some_and(|b| b <= LOW_PERCENT);
    let charged = i.external && !i.charging && i.battery.is_some_and(|b| b >= 100);
    if i.fault {
        show(RED, Pattern::FastBlink)
    } else if critical {
        show(RED, Pattern::SlowBlink)
    } else if i.updating {
        show(BLUE, Pattern::Breathing)
    } else if i.recovery {
        show(BLUE, Pattern::Solid)
    } else if i.eject_first {
        show(BLUE, Pattern::SlowBlink)
    } else if low {
        show(AMBER, Pattern::SlowBlink)
    } else if i.charging {
        show(AMBER, Pattern::Solid)
    } else if charged {
        show(GREEN, Pattern::Solid)
    } else if i.pairing {
        show(BLUE, Pattern::FastBlink)
    } else if i.activity {
        show(WHITE, Pattern::Pulse)
    } else if i.booting {
        show(WHITE, Pattern::Breathing)
    } else {
        i.developer
    }
}

/// A developer's request: `r g b [pattern]`.
pub fn parse_developer(text: &str) -> Option<Show> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let c = |i: usize| words.get(i).and_then(|w| w.parse::<u8>().ok());
    let pattern = match words.get(3) {
        Some(w) => Pattern::parse(w)?,
        None => Pattern::Solid,
    };
    Some(Show {
        rgb: [c(0)?, c(1)?, c(2)?],
        pattern,
    })
}

#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    if let Err(e) = linux::run() {
        eprintln!("odap-statusled: {e}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    eprintln!("odap-statusled: Linux only");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_priority_wins() {
        let mut i = Inputs {
            booting: true,
            charging: true,
            external: true,
            battery: Some(40),
            ..Default::default()
        };
        assert_eq!(decide(&i), Some(Show { rgb: AMBER, pattern: Pattern::Solid }), "charging over booting");
        i.updating = true;
        assert_eq!(decide(&i).unwrap().pattern, Pattern::Breathing);
        assert_eq!(decide(&i).unwrap().rgb, BLUE);
        i.fault = true;
        assert_eq!(decide(&i), Some(Show { rgb: RED, pattern: Pattern::FastBlink }));
    }

    #[test]
    fn the_battery_warns_only_while_discharging() {
        let mut i = Inputs {
            battery: Some(4),
            ..Default::default()
        };
        assert_eq!(decide(&i), Some(Show { rgb: RED, pattern: Pattern::SlowBlink }));
        i.battery = Some(12);
        assert_eq!(decide(&i), Some(Show { rgb: AMBER, pattern: Pattern::SlowBlink }));
        i.charging = true;
        i.external = true;
        assert_eq!(decide(&i), Some(Show { rgb: AMBER, pattern: Pattern::Solid }));
        i.charging = false;
        i.battery = Some(100);
        assert_eq!(decide(&i), Some(Show { rgb: GREEN, pattern: Pattern::Solid }));
        i.external = false;
        assert_eq!(decide(&i), None, "normal operation is dark");
    }

    #[test]
    fn a_developer_never_overrides_priorities_one_to_five_and_takes_the_rest() {
        let dev = parse_developer("10 200 30 fast-blink");
        assert_eq!(dev, Some(Show { rgb: [10, 200, 30], pattern: Pattern::FastBlink }));
        let mut i = Inputs { developer: dev, ..Default::default() };
        assert_eq!(decide(&i), dev);
        i.recovery = true;
        assert_eq!(decide(&i), Some(Show { rgb: BLUE, pattern: Pattern::Solid }));
        assert_eq!(parse_developer("1 2"), None);
        assert_eq!(parse_developer("1 2 3 wobble"), None);
    }

    #[test]
    fn patterns_wake_only_while_they_change() {
        assert_eq!(Pattern::Solid.at(Duration::from_secs(9)), (1.0, None));
        assert_eq!(Pattern::SlowBlink.at(Duration::from_millis(200)), (1.0, Some(Duration::from_millis(300))));
        assert_eq!(Pattern::SlowBlink.at(Duration::from_millis(700)), (0.0, Some(Duration::from_millis(300))));
        assert_eq!(Pattern::Pulse.at(Duration::from_millis(50)).0, 1.0);
        assert_eq!(Pattern::Pulse.at(Duration::from_millis(150)), (0.0, Some(Duration::from_millis(2850))));
        let (b, _) = Pattern::Breathing.at(Duration::from_millis(1000));
        assert!((b - 1.0).abs() < 1e-3, "full at half the period");
    }
}
