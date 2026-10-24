//! `odap-testbed`: the UI primitives activity (SYS-071), a developer activity included in
//! Blackbird OS that exercises the UI primitives one screen at a time, built from Blackbird UI's
//! components. The debug UI mode (SYS-070) draws over it as over any activity: with the system's
//! flag on, its components draw as blueprints, and the shell draws its regions, the physical
//! controls, touches and gestures above it. It draws only on input, so an idle screen stays idle.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use opendap_device::Description;
use opendap_input::gesture;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

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

/// The debug UI mode's flag (SYS-070), as the shell reads it: the file, or the environment.
fn debug_ui() -> bool {
    // The switch file overrides the environment's default: `1` on, `0` off (a simulator's
    // switch can turn off what the board's support package turned on).
    match std::fs::read_to_string("/run/opendap/debug-ui") {
        Ok(v) => v.trim() != "0",
        Err(_) => std::env::var("ODAP_DEBUG_UI").is_ok_and(|v| v == "1"),
    }
}

const USAGE: &str = "usage: odap-testbed [SCREEN]
  SCREEN   open that primitive's screen (Calibration, Targets, Gestures, Controls, Motion,
           Opacity, Exceptions)";

fn main() -> Result<(), slint::PlatformError> {
    let ui = Testbed::new()?;
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "-h" || arg == "--help" {
            println!("{USAGE}");
            return Ok(());
        }
        ui.set_screen(arg.into());
    }
    ui.global::<Blueprint>().set_on(debug_ui());
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
    let device = match (*desc).as_ref() {
        Some(d) => {
            let size = d
                .display
                .as_ref()
                .map(|x| format!("{} × {} px", x.width_px, x.height_px))
                .unwrap_or_else(|| "no display".into());
            format!("{} · {size}", d.identity.name)
        }
        None => "no device description".into(),
    };
    ui.set_device(device.into());

    // The gesture pad: a contact followed while it lasts (opendap-input's Contact), so a long
    // press is named while the finger is still down and nothing follows when it lifts.
    let contact: Rc<RefCell<Option<gesture::Contact>>> = Rc::default();
    let started = Instant::now();
    let long_timer = Rc::new(Timer::default());
    let weak = ui.as_weak();
    ui.on_pad(move |kind, x, y, w, h| {
        let Some(ui) = weak.upgrade() else { return };
        // The pad is a frame of its own: its regions are its edges and corners, not the screen's
        // (input events §3.2.1).
        let surface = gesture::Frame::new((w, h), ui.get_unit());
        let now = started.elapsed().as_millis() as u64;
        match kind.as_str() {
            "down" => {
                ui.set_contacts(ui.get_contacts() + 1);
                *contact.borrow_mut() = Some(gesture::Contact::down((x, y), now, surface));
                let contact = contact.clone();
                let weak = ui.as_weak();
                long_timer.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_millis(gesture::LONG_MS),
                    move || {
                        let Some(ui) = weak.upgrade() else { return };
                        let now = started.elapsed().as_millis() as u64;
                        if let Some(g) = contact.borrow_mut().as_mut().and_then(|c| c.long_due(now)) {
                            ui.set_gesture(g.describe().into());
                        }
                    },
                );
            }
            "move" => {
                // A pan or a drag, named live phase by phase.
                if let Some(g) = contact.borrow_mut().as_mut().and_then(|c| c.moved((x, y), now)) {
                    ui.set_gesture(g.describe().into());
                }
            }
            "up" => {
                long_timer.stop();
                if let Some(mut c) = contact.borrow_mut().take()
                    && let Some(g) = c.up((x, y), now).pop()
                {
                    ui.set_gesture(g.describe().into());
                }
            }
            _ => {
                long_timer.stop();
                let g = contact.borrow_mut().take().and_then(|mut c| c.cancel());
                ui.set_gesture(g.map_or("cancelled".into(), |g| g.describe()).into());
            }
        }
    });

    ui.on_exception(|what| match what.as_str() {
        // The shell keeps the last frame and restarts the activity (shell spec §9).
        "panic" => panic!("testbed: a panic, asked for"),
        // An unresponsive activity: the event loop blocks, nothing draws.
        "hang" => std::thread::sleep(std::time::Duration::from_secs(5)),
        _ => std::process::exit(0),
    });

    ui.run()
}
