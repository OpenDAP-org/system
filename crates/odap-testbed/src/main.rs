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
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

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

/// The region a contact starts in, for the testbed to light it.
fn zone(at: (f32, f32), size: (f32, f32)) -> &'static str {
    if at.1 < 28.0 {
        "status"
    } else if at.0 < gesture::EDGE_PX {
        "leading"
    } else if at.0 > size.0 - gesture::EDGE_PX {
        "trailing"
    } else if at.1 > size.1 - gesture::EDGE_PX {
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
    match description() {
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
                ui.set_touch_zone(zone(at, size).into());
                *contact.borrow_mut() = Some(Contact {
                    at,
                    since: Instant::now(),
                });
            }
            "up" => {
                if let Some(c) = contact.borrow_mut().take() {
                    let g = gesture::classify(c.at, at, c.since.elapsed().as_millis() as u64, size);
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
