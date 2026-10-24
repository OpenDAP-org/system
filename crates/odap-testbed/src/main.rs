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
use opendap_input::{arena, gesture, nested, node, space};
use opendap_layout::{
    data::{
        cache::Cache,
        gate::{Cost, Gate},
        pages::{ItemState, Pages, Paging},
    },
    extents::{Extents, Measured},
    recycle::Pool,
    window::{Overscan, visible, window},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, VecModel};

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

    // The Spaces screen: the input primitives in use (opendap-input's space and node). A list
    // scrolled by its own pan: the finger moves in the viewport, while in the content, which
    // follows it, it stays put; the row under it, its address and its own coordinates.
    const ROW: f32 = 42.0;
    const ROWS: usize = 20;
    struct ListState {
        track: space::Track,
        offset: f32,
        offset_at_press: f32,
        last_y: f32,
    }
    let list = Rc::new(RefCell::new(ListState {
        track: space::Track::default(),
        offset: 0.0,
        offset_at_press: 0.0,
        last_y: 0.0,
    }));
    let weak = ui.as_weak();
    let started_list = Instant::now();
    ui.on_spaces_list(move |kind, x, y, ax, ay, w, h| {
        let Some(ui) = weak.upgrade() else { return };
        let mut st = list.borrow_mut();
        let now = started_list.elapsed().as_millis() as u64;
        let unit = ui.get_unit();
        let viewport = gesture::Frame::new((w, h), unit).at((ax, ay));
        let content_h = ROWS as f32 * ROW;
        let content = |offset: f32| viewport.content((w, content_h), (0.0, offset));
        match kind.as_str() {
            "down" => {
                st.track = space::Track::default();
                st.offset_at_press = st.offset;
                st.last_y = y;
            }
            "move" => {
                // The list's own pan: the content follows the finger.
                let dy = y - st.last_y;
                st.offset = (st.offset - dy).clamp(0.0, (content_h - h).max(0.0));
                st.last_y = y;
            }
            _ => {}
        }
        st.track.push((ax + x, ay + y), now);
        ui.set_spaces_offset(st.offset);
        let vp = gesture::FramePath(vec![viewport]);
        let ct_now = gesture::FramePath(vec![viewport, content(st.offset)]);
        let ct_then = gesture::FramePath(vec![viewport, content(st.offset_at_press)]);
        let (Some(p_vp), Some(p_ct), Some(t_vp), Some(t_ct)) = (
            st.track.position(&vp),
            st.track.position(&ct_now),
            st.track.translation(&vp, &vp),
            st.track.translation(&ct_then, &ct_now),
        ) else {
            return;
        };
        let i = ((p_ct.1 / ROW).floor().max(0.0) as usize).min(ROWS - 1);
        let path = |offset: f32| {
            node::NodePath(vec![
                node::Node::new(node::NodeId::new(node::NodeKind::Activity, "ui-primitives"), gesture::Frame::new((0.0, 0.0), unit)),
                node::Node::new(node::NodeId::new(node::NodeKind::Container, "list"), viewport),
                node::Node::new(node::NodeId::new(node::NodeKind::Region, "content"), content(offset)),
                node::Node::new(node::NodeId::new(node::NodeKind::Element, format!("row-{i}")), gesture::Frame::new((w, ROW), unit).at((0.0, i as f32 * ROW))),
            ])
        };
        let row = path(st.offset);
        let in_row = row.frames().to_inner((ax + x, ay + y));
        let region = st.track.region(&vp).map(|r| format!("{r:?}")).unwrap_or_default();
        let lines: Vec<SharedString> = vec![
            row.address().into(),
            format!("@viewport {:.0}, {:.0}  moved {:.0}, {:.0}", p_vp.0, p_vp.1, t_vp.0, t_vp.1).into(),
            format!("@content  {:.0}, {:.0}  moved {:.0}, {:.0}", p_ct.0, p_ct.1, t_ct.0, t_ct.1).into(),
            format!("in row-{i}  {:.0}, {:.0}", in_row.0, in_row.1).into(),
            format!("region in the viewport: {region}").into(),
        ];
        ui.set_spaces_lines(ModelRc::new(VecModel::from(lines)));
    });

    // A dial: the angle around its centre, and the total turn, unwrapped across 0°/360°.
    let dial: Rc<RefCell<(f32, f32)>> = Rc::default();
    let weak = ui.as_weak();
    ui.on_spaces_dial(move |kind, x, y, w, h| {
        let Some(ui) = weak.upgrade() else { return };
        let (angle, _) = space::polar((w / 2.0, h / 2.0), (x, y));
        let mut d = dial.borrow_mut();
        if kind.as_str() == "down" {
            d.0 = angle;
        } else {
            d.1 += space::angle_delta(d.0, angle);
            d.0 = angle;
        }
        ui.set_dial_text(format!("angle {:.0}°\nturned {:.0}°", d.0, d.1).into());
    });

    // A strip: 0 to 1 from its start edge, mirrored right to left.
    let weak = ui.as_weak();
    ui.on_spaces_strip(move |x, w, rtl| {
        let Some(ui) = weak.upgrade() else { return };
        let frame = gesture::Frame { rtl, ..gesture::Frame::new((w, 32.0), ui.get_unit()) };
        let (v, _) = space::normalised(&frame, (x, 16.0));
        ui.set_strip_text(format!("{v:.2} from the {} edge", if rtl { "right (start)" } else { "left (start)" }).into());
    });

    // The Arena screen: tap, double tap (when opted in) and long press compete for each contact;
    // a timer ticks the arena at its deadlines, as live time would.
    let arena: Rc<RefCell<Option<arena::Arena>>> = Rc::default();
    let arena_timer = Rc::new(Timer::default());
    let started_arena = Instant::now();
    fn fresh(double: bool, unit: f32) -> arena::Arena {
        let a = arena::Arena::new();
        let a = if double { a.add(arena::DoubleTap::new(unit), arena::Priority::Normal) } else { a };
        a.add(arena::Tap::new(unit), arena::Priority::Normal)
            .add(arena::LongPress::new(unit), arena::Priority::Normal)
    }
    fn arm(
        timer: &Rc<Timer>,
        arena: &Rc<RefCell<Option<arena::Arena>>>,
        weak: slint::Weak<Testbed>,
        started: Instant,
    ) {
        let Some(deadline) = arena.borrow().as_ref().and_then(|a| a.deadline()) else {
            timer.stop();
            return;
        };
        let now = started.elapsed().as_millis() as u64;
        let (arena, t) = (arena.clone(), timer.clone());
        timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(deadline.saturating_sub(now)),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                let ms = started.elapsed().as_millis() as u64;
                let won = arena.borrow_mut().as_mut().map(|a| a.feed(arena::Input::Tick { ms }).won);
                if let Some(w) = won.and_then(|w| w.first().cloned()) {
                    ui.set_arena_text(format!("{w} won").into());
                }
                arm(&t, &arena, ui.as_weak(), started);
            },
        );
    }
    let weak = ui.as_weak();
    ui.on_arena_pad(move |kind, x, y| {
        let Some(ui) = weak.upgrade() else { return };
        let ms = started_arena.elapsed().as_millis() as u64;
        let input = match kind.as_str() {
            "down" => arena::Input::Down { at: (x, y), ms },
            "move" => arena::Input::Move { at: (x, y), ms },
            "up" => arena::Input::Up { at: (x, y), ms },
            _ => arena::Input::Cancel,
        };
        {
            let mut a = arena.borrow_mut();
            // A new arena when the members change (the switch) or the last contact settled.
            if kind.as_str() == "down"
                && (a.is_none() || a.as_ref().is_some_and(|a| a.deadline().is_none()))
            {
                *a = Some(fresh(ui.get_arena_double(), ui.get_unit()));
                ui.set_arena_text("…".into());
            }
            if let Some(w) = a.as_mut().and_then(|a| a.feed(input).won.first().cloned()) {
                ui.set_arena_text(format!("{w} won").into());
            }
        }
        arm(&arena_timer, &arena, ui.as_weak(), started_arena);
    });

    // Nested scrolling with its own primitive: the finger's move goes to the inner list first
    // when it started there, and what the inner one cannot take goes to the outer one.
    let chain = Rc::new(RefCell::new((
        [nested::Scroller::new(8.0 * 24.0, 72.0), nested::Scroller::new(400.0, 150.0)],
        false,
        0.0f32,
    )));
    let weak = ui.as_weak();
    ui.on_nested_pan(move |kind, _x, y, in_inner| {
        let Some(ui) = weak.upgrade() else { return };
        let mut st = chain.borrow_mut();
        if kind.as_str() == "down" {
            st.1 = in_inner;
            st.2 = y;
            return;
        }
        let delta = st.2 - y;
        st.2 = y;
        let start = if st.1 { 0 } else { 1 };
        let left = nested::scroll(&mut st.0[start..], delta);
        ui.set_nested_inner(st.0[0].offset);
        ui.set_nested_outer(st.0[1].offset);
        ui.set_nested_text(
            format!(
                "inner {:.0}/{:.0}  outer {:.0}/{:.0}  left {:.0}",
                st.0[0].offset, st.0[0].max, st.0[1].offset, st.0[1].max, left
            )
            .into(),
        );
    });

    // The Virtual screen: a hundred thousand rows scrolled by the list's own pan. opendap-layout
    // says which rows to build (the window, with prefetch ahead by velocity), measures them as
    // they are built (every seventh is taller), reuses a bounded set of slots by key, loads the
    // rows' data a page at a time (each page takes 250 ms, as a slow card would), and decodes
    // covers only while the scroll is slow, within a cache's budget.
    const VCOUNT: usize = 100_000;
    struct Virtual {
        extents: Measured,
        pool: Pool<usize>,
        pages: Pages,
        covers: Cache<usize, ()>,
        gate: Gate,
        model: Rc<VecModel<VRow>>,
        offset: f32,
        viewport: f32,
        last_y: f32,
        last_ms: u64,
        velocity: f32,
        rebound: usize,
        cancelled: usize,
        evicted: usize,
    }
    let vstate = Rc::new(RefCell::new(Virtual {
        extents: Measured::new(VCOUNT, 48.0),
        pool: Pool::new(),
        pages: Pages::new(Paging::default(), Some(VCOUNT)),
        // About sixty decoded covers at most (proposed: to measure).
        covers: Cache::new(60),
        gate: Gate::default(),
        model: Rc::new(VecModel::default()),
        offset: 0.0,
        viewport: 260.0,
        last_y: 0.0,
        last_ms: 0,
        velocity: 0.0,
        rebound: 0,
        cancelled: 0,
        evicted: 0,
    }));
    // Lays out the window and asks for what it lacks; page arrivals call it again.
    fn vrender(st: &Rc<RefCell<Virtual>>, ui: &Testbed) {
        let mut v = st.borrow_mut();
        let range = window(&v.extents, v.offset, v.viewport, v.velocity, Overscan::default());
        for i in range.clone() {
            v.extents.measure(i, if i % 7 == 0 { 72.0 } else { 48.0 });
        }
        let keys: Vec<usize> = range.clone().collect();
        let changes = v.pool.update(&keys);
        v.rebound = changes.bound.len();
        // The data: the pages this window needs, most needed first.
        let forward = v.velocity >= 0.0;
        let plan = v.pages.plan(range.clone(), forward);
        v.cancelled += plan.cancel.len();
        v.evicted += plan.evict.len();
        for page in plan.load {
            let (st, weak) = (st.clone(), ui.as_weak());
            Timer::single_shot(Duration::from_millis(250), move || {
                st.borrow_mut().pages.arrived(page, 50, false);
                if let Some(ui) = weak.upgrade() {
                    vrender(&st, &ui);
                }
            });
        }
        // Covers: decoded only while the scroll is slow, kept within the cache's budget.
        let shown = visible(&v.extents, v.offset, v.viewport);
        if v.gate.admits(Cost::Costly, v.velocity) {
            for i in shown.clone() {
                if v.pages.item(i) == ItemState::Ready && v.covers.get(&i).is_none() {
                    v.covers.put(i, (), 1);
                }
            }
        }
        while v.model.row_count() < v.pool.slots() {
            v.model.push(VRow { index: 0, y: -1000.0, h: 0.0, tall: false, ready: false, cover: false });
        }
        for &k in &keys {
            if let Some(slot) = v.pool.slot_of(&k) {
                let ready = v.pages.item(k) == ItemState::Ready;
                let cover = v.covers.get(&k).is_some();
                let row = VRow {
                    index: k as i32,
                    y: v.extents.offset(k) - v.offset,
                    h: v.extents.size(k),
                    tall: k % 7 == 0,
                    ready,
                    cover,
                };
                v.model.set_row_data(slot, row);
            }
        }
        ui.set_vrows(ModelRc::from(v.model.clone()));
        ui.set_vtext(
            format!(
                "{VCOUNT} rows, {}-{} on screen, built {}\nslots {}, rebound {}, {:.0} px/s\npages {} loaded, {} loading, {} cancelled, {} evicted\ncovers {} decoded{}",
                shown.start,
                shown.end.saturating_sub(1),
                range.len(),
                v.pool.slots(),
                v.rebound,
                v.velocity,
                v.pages.loaded(),
                v.pages.in_flight(),
                v.cancelled,
                v.evicted,
                v.covers.len(),
                if v.gate.admits(Cost::Costly, v.velocity) { "" } else { ", waiting for the scroll to slow" }
            )
            .into(),
        );
    }
    let started_v = Instant::now();
    let weak = ui.as_weak();
    let st = vstate.clone();
    ui.on_vpan(move |kind, y, h| {
        let Some(ui) = weak.upgrade() else { return };
        {
            let mut v = st.borrow_mut();
            let ms = started_v.elapsed().as_millis() as u64;
            if h > 0.0 {
                v.viewport = h;
            }
            match kind.as_str() {
                "down" => {
                    v.last_y = y;
                    v.last_ms = ms;
                    v.velocity = 0.0;
                    return;
                }
                // A move, or the release with the last stretch it carries.
                "move" | "up" => {
                    let dy = y - v.last_y;
                    let dt = ms.saturating_sub(v.last_ms).max(1) as f32 / 1000.0;
                    v.velocity = if kind.as_str() == "up" { 0.0 } else { -dy / dt };
                    v.last_y = y;
                    v.last_ms = ms;
                    let max = (v.extents.total() - v.viewport).max(0.0);
                    v.offset = (v.offset - dy).clamp(0.0, max);
                }
                // Opened: the scroll is still; costly loads may run.
                _ => v.velocity = 0.0,
            }
        }
        vrender(&st, &ui);
    });

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
