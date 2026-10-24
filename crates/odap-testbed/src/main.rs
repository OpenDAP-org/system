//! `odap-testbed`: a host that shows a blueprint test screen in place of the product's places, for
//! now. It marks out the shell's regions and the physical controls from the device description,
//! and prototypes and checks the UI primitives: events, gestures, effects, regions and exceptions.
//! It draws only on input, so an idle screen stays idle.

mod gesture;

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use opendap_device::Description;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

slint::include_modules!();

/// Where the device description is, first match wins (as the shell reads it).
const DESCRIPTIONS: &[&str] = &[
    "/run/opendap/device/description.toml",
    "/usr/share/opendap/device/description.toml",
];

fn description() -> Option<Description> {
    DESCRIPTIONS
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| Description::parse(&text).ok())
}

/// The controls on the display's edges, in logical pixels.
fn controls(d: &Description) -> Vec<ControlMark> {
    let Some(display) = d.display.as_ref() else {
        return Vec::new();
    };
    let scale = display.scale().round().max(1.0);
    d.controls
        .iter()
        .map(|c| ControlMark {
            role: kebab(&format!("{:?}", c.role)).into(),
            edge: format!("{:?}", c.edge).to_lowercase().into(),
            centre: display.mm_to_px(c.offset_mm) / scale,
            span: display.mm_to_px(c.length_mm) / scale,
        })
        // A role with several codes (a host key too) is drawn once.
        .fold(Vec::<ControlMark>::new(), |mut out, m| {
            if !out.iter().any(|o| o.role == m.role) {
                out.push(m);
            }
            out
        })
}

/// `VolumeUp` as the description writes it, `volume-up`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// How long the gesture's toast stays (Android's short toast).
const TOAST: Duration = Duration::from_millis(2000);

fn logical_size(ui: &Testbed) -> (f32, f32) {
    let s = ui.window().size();
    let f = ui.window().scale_factor();
    (s.width as f32 / f, s.height as f32 / f)
}

/// The region a contact starts in, for the testbed to light it; `unit` as in `classify`.
fn zone(at: (f32, f32), size: (f32, f32), unit: f32) -> &'static str {
    let edge = gesture::EDGE_PX * unit;
    if at.1 < 28.0 * unit {
        "status"
    } else if at.0 < edge {
        "leading"
    } else if at.0 > size.0 - edge {
        "trailing"
    } else if at.1 > size.1 - edge {
        "bottom"
    } else {
        ""
    }
}

fn show_toast(ui: &Testbed, timer: &Timer, text: String) {
    ui.set_toast(text.into());
    let weak = ui.as_weak();
    timer.start(TimerMode::SingleShot, TOAST, move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_toast("".into());
        }
    });
}

/// Logical pixels per design pixel on this hardware: the exact scale its density and reading
/// distance give (design system §18.2), over the scale the window was given (an integer: the
/// shell offers no fractional scale). 1 when they agree.
fn unit(d: Option<&Description>, applied: f32) -> f32 {
    d.and_then(|d| d.display.as_ref())
        .map(|x| x.scale() / applied.max(0.01))
        .unwrap_or(1.0)
}

/// What the testbed says about its calibration, line by line.
fn calibration(d: &Description, applied: f32) -> (Vec<SharedString>, bool) {
    let mut out = Vec::new();
    let Some(x) = d.display.as_ref() else {
        return (vec!["no display".into()], false);
    };
    let exact = x.scale();
    let deviation = (applied / exact - 1.0) * 100.0;
    let mm = |design_px: f32| design_px * x.design_px_mm();
    out.push(format!(
        "{:.0} ppi · {:.2} px/mm · {:.1} × {:.1} mm · {:?} distance",
        x.ppi(),
        x.px_per_mm(),
        x.width_mm,
        x.height_mm,
        x.distance
    ));
    out.push(format!(
        "design px {:.3} mm · scale {exact:.3} exact, {applied:.0} applied ({deviation:+.1} %)",
        x.design_px_mm()
    ));
    out.push(format!(
        "targets 48 · 52 · 64 → {:.1} · {:.1} · {:.1} mm",
        mm(48.0),
        mm(52.0),
        mm(64.0)
    ));
    out.push(format!(
        "edge 20 → {:.1} mm · margin 16 → {:.1} mm · slop {} → {:.1} mm",
        mm(gesture::EDGE_PX),
        mm(16.0),
        gesture::SLOP_PX,
        mm(gesture::SLOP_PX)
    ));
    match d.touch {
        Some(t) => {
            let (sx, sy) = t.step_mm(x);
            out.push(format!(
                "touch {} × {} · {:.4} × {:.4} mm a step · {} contacts",
                t.resolution_x, t.resolution_y, sx, sy, t.contacts
            ));
        }
        None => out.push("no touch panel described".into()),
    }
    (
        out.into_iter().map(SharedString::from).collect(),
        deviation.abs() > 2.0,
    )
}

struct Contact {
    at: (f32, f32),
    since: Instant,
}

const USAGE: &str = "usage: odap-testbed [--no-grid] [--no-regions] [--no-controls] [--tests]
  --no-grid, --no-regions, --no-controls   start with that layer off (each is also a checkbox)
  --tests                                  start with the tests sheet open";

fn main() -> Result<(), slint::PlatformError> {
    let ui = Testbed::new()?;
    // The layers are flags, so a unit or a test run starts the screen it needs.
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--no-grid" => ui.set_show_grid(false),
            "--no-regions" => ui.set_show_regions(false),
            "--no-controls" => ui.set_show_controls(false),
            "--tests" => ui.set_sheet_open(true),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                eprintln!("odap-testbed: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let desc = Rc::new(description());
    // The applied scale is known once the window is on screen: calibrate then, once.
    let weak = ui.as_weak();
    let calib_desc = desc.clone();
    Timer::single_shot(Duration::ZERO, move || {
        let Some(ui) = weak.upgrade() else { return };
        let applied = ui.window().scale_factor();
        ui.set_unit(unit((*calib_desc).as_ref(), applied));
        if let Some(d) = (*calib_desc).as_ref() {
            let (lines, off) = calibration(d, applied);
            ui.set_calibration(ModelRc::new(VecModel::from(lines)));
            ui.set_calibration_off(off);
        }
    });
    match (*desc).clone() {
        Some(d) => {
            let size = d
                .display
                .as_ref()
                .map(|x| format!("{} × {} px", x.width_px, x.height_px))
                .unwrap_or_else(|| "no display".into());
            ui.set_device(format!("{} · {size}", d.identity.name).into());
            ui.set_controls(ModelRc::new(VecModel::from(controls(&d))));
        }
        None => ui.set_device("no device description".into()),
    }

    let contact: Rc<RefCell<Option<Contact>>> = Rc::default();
    // The toast leaves by itself: one timer per gesture, none while idle.
    let toast_timer = Rc::new(Timer::default());
    let weak = ui.as_weak();
    ui.on_contact(move |kind, x, y| {
        let Some(ui) = weak.upgrade() else { return };
        let at = (x, y);
        let size = logical_size(&ui);
        match kind.as_str() {
            "down" => {
                ui.set_events(ui.get_events() + 1);
                ui.set_touch_zone(zone(at, size, ui.get_unit()).into());
                *contact.borrow_mut() = Some(Contact {
                    at,
                    since: Instant::now(),
                });
            }
            "up" => {
                if let Some(c) = contact.borrow_mut().take() {
                    let g = gesture::classify(
                        c.at,
                        at,
                        c.since.elapsed().as_millis() as u64,
                        size,
                        ui.get_unit(),
                    );
                    show_toast(&ui, &toast_timer, g.describe());
                }
            }
            "cancel" => {
                contact.borrow_mut().take();
                show_toast(&ui, &toast_timer, "cancelled".into());
            }
            _ => {}
        }
        let down = contact.borrow().is_some();
        ui.set_touching(down);
        if down {
            ui.set_touch_x(x);
            ui.set_touch_y(y);
        }
    });

    ui.on_exception(|what| match what.as_str() {
        // The shell keeps the last frame and restarts the host (shell spec §9).
        "panic" => panic!("testbed: a panic, asked for"),
        // An unresponsive host: the event loop blocks, nothing draws.
        "hang" => std::thread::sleep(std::time::Duration::from_secs(5)),
        _ => std::process::exit(0),
    });

    ui.run()
}
