//! `odap-testbed`: the UI primitives activity (SYS-071), a developer activity included in
//! Blackbird OS that exercises the UI primitives one screen at a time, built from Blackbird UI's
//! components. The debug UI mode (SYS-070) draws over it as over any activity: with the system's
//! flag on, its components draw as blueprints, and the composer draws its regions, the physical
//! controls, touches and gestures above it. It draws only on input, so an idle screen stays idle.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use opendap_device::Description;
use opendap_input::{
    anchor::{self, Rect as ARect},
    arena, gesture,
    matched::{Matched, Side as MSide},
    nav::{self, Effect, NavNode, Navigator},
    nested, node, space,
};
use opendap_media::{fit, plot};
use opendap_motion::{
    animator::{Animator, Kind as AnimKind},
    easing::Bezier,
    interpolate::{Frame, frame},
    presence::Presence,
    transition::{End as TEnd, Transition},
    budget::Budget,
    spring::Spring,
};
use opendap_forms::{
    form::Form,
    kind::{Issue, Kind, Value},
};
use opendap_layout::{
    data::{
        cache::Cache,
        gate::{Cost, Gate},
        pages::{ItemState, Pages, Paging},
    },
    extents::{Extents, Measured},
    focus as vfocus,
    pager::{resist, settle},
    recycle::Pool,
    window::{Overscan, visible, window},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, VecModel};

slint::include_modules!();

/// Where the device description is, first match wins (as the composer reads it).
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
/// composer offers no fractional scale). 1 when they agree.
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

/// The debug UI mode's flag (SYS-070), as the composer reads it: the file, or the environment.
fn debug_ui() -> bool {
    // The switch file overrides the environment's default: `1` on, `0` off (a simulator's
    // switch can turn off what the board's support package turned on).
    match std::fs::read_to_string("/run/opendap/debug-ui") {
        Ok(v) => v.trim() != "0",
        Err(_) => std::env::var("ODAP_DEBUG_UI").is_ok_and(|v| v == "1"),
    }
}

#[cfg(target_os = "linux")]
fn watch_debug(weak: slint::Weak<Testbed>) {
    // SAFETY: inotify calls with a valid C string; a blocking read into a buffer of its size.
    unsafe {
        let fd = libc::inotify_init1(libc::IN_CLOEXEC);
        if fd < 0 {
            return;
        }
        let dir = std::ffi::CString::new("/run/opendap").expect("no NUL");
        libc::inotify_add_watch(fd, dir.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_TO);
        let mut buf = [0u8; 4096];
        let mut last = debug_ui();
        while libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) > 0 {
            let on = debug_ui();
            if on != last {
                last = on;
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.global::<Blueprint>().set_on(on);
                    }
                });
            }
        }
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
    // The flag is watched (inotify, no polling): the blueprint switches in place, over whatever
    // state is on screen (screen, focus, scroll, locks), never restarting anything.
    #[cfg(target_os = "linux")]
    {
        let weak = ui.as_weak();
        std::thread::spawn(move || watch_debug(weak));
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
        /// The row the buttons focus, by index (its key).
        focused: Option<usize>,
    }
    let list = Rc::new(RefCell::new(ListState {
        track: space::Track::default(),
        offset: 0.0,
        offset_at_press: 0.0,
        last_y: 0.0,
        focused: None,
    }));
    let list_k = list.clone();
    let weak = ui.as_weak();
    let started_list = Instant::now();
    ui.on_spaces_list(move |kind, x, y, ax, ay, w, h| {
        let Some(ui) = weak.upgrade() else { return };
        let mut st = list.borrow_mut();
        let now = started_list.elapsed().as_millis() as u64;
        let unit = ui.get_unit();
        let viewport = gesture::Frame { rtl: ui.get_strip_rtl(), ..gesture::Frame::new((w, h), unit) }.at((ax, ay));
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
    let dial_k = dial.clone();
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
        ui.set_dial_angle(d.0);
    });

    // A strip: 0 to 1 from its start edge, mirrored right to left.
    let weak = ui.as_weak();
    ui.on_spaces_strip(move |x, w, rtl| {
        let Some(ui) = weak.upgrade() else { return };
        let frame = gesture::Frame { rtl, ..gesture::Frame::new((w, 32.0), ui.get_unit()) };
        let (v, _) = space::normalised(&frame, (x, 16.0));
        ui.set_strip_value(v);
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
    let chain: Rc<RefCell<([nested::Scroller; 2], bool, f32)>> = Rc::new(RefCell::new((
        [nested::Scroller::new(8.0 * 24.0, 72.0), nested::Scroller::new(400.0, 150.0)],
        false,
        0.0f32,
    )));
    let chain_k = chain.clone();
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
    // Sections of a thousand rows, each headed by a 28 px header above its first row, pinned at
    // the list's top while it passes (opendap-layout's sticky).
    const SECTION: usize = 1000;
    const HEAD: f32 = 28.0;
    // The reveal's scroll: a curve the system starts, retargeted by every press (ui-model §8).
    const REVEAL: AnimKind = AnimKind::Timed { duration: 0.18, curve: Bezier::STANDARD };
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
        /// The focused item, by index (its key here), never by slot.
        focused: Option<usize>,
        /// How far the contact moved: a tap when it did not.
        travel: f32,
        activated: Option<usize>,
        /// Action on a placeholder: it waits for its data (input events §3.7).
        activation: vfocus::Activation,
        /// The reveal's scroll toward the focused item.
        reveal: Animator,
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
        focused: None,
        travel: 0.0,
        activated: None,
        activation: vfocus::Activation::default(),
        reveal: Animator::new(0.0, REVEAL),
    }));
    let vtimer = Rc::new(Timer::default());
    let started_reveal = Instant::now();
    // Lays out the window and asks for what it lacks; page arrivals call it again.
    fn vrender(st: &Rc<RefCell<Virtual>>, ui: &Testbed) {
        let mut v = st.borrow_mut();
        let range = window(&v.extents, v.offset, v.viewport, v.velocity, Overscan::default());
        for i in range.clone() {
            let head = if i % SECTION == 0 { HEAD } else { 0.0 };
            v.extents.measure(i, head + if i % 7 == 0 { 72.0 } else { 48.0 });
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
        // What Action waited for, once its data is here.
        let ready = v.activation.waiting().is_some_and(|i| v.pages.item(i) == ItemState::Ready);
        if let Some(i) = v.activation.arrived(|_| ready) {
            v.activated = Some(i);
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
            v.model.push(VRow { index: 0, y: -1000.0, h: 0.0, tall: false, ready: false, cover: false, focused: false, head: 0.0 });
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
                    focused: v.focused == Some(k),
                    head: if k % SECTION == 0 { HEAD } else { 0.0 },
                };
                v.model.set_row_data(slot, row);
            }
        }
        ui.set_vrows(ModelRc::from(v.model.clone()));
        // The pinned header: the section at the list's top.
        let k = v.extents.index_at(v.offset) / SECTION;
        let start = v.extents.offset(k * SECTION);
        let end = if (k + 1) * SECTION < VCOUNT { v.extents.offset((k + 1) * SECTION) } else { v.extents.total() };
        ui.set_vband_y(opendap_layout::sticky::header_y(start, end, HEAD, v.offset));
        ui.set_vband_text(format!("Rows {}–{}", k * SECTION, ((k + 1) * SECTION).min(VCOUNT) - 1).into());
        ui.set_vtext(
            format!(
                "{VCOUNT} rows, {}-{} on screen, built {}\nslots {}, rebound {}, {:.0} px/s\npages {} loaded, {} loading, {} cancelled, {} evicted\ncovers {} decoded{}\nfocus: {}{}",
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
                if v.gate.admits(Cost::Costly, v.velocity) { "" } else { ", waiting for the scroll to slow" },
                v.focused.map_or("none".to_string(), |i| format!("row {i}, slot {}", v.pool.slot_of(&i).map_or("-".into(), |s| s.to_string()))),
                match (v.activation.waiting(), v.activated) {
                    (Some(i), _) => format!(", row {i} waits for its data"),
                    (None, Some(i)) => format!(", activated row {i}"),
                    _ => String::new(),
                }
            )
            .into(),
        );
    }
    // The buttons in the list: the focus walks items (not slots), the list keeps it in view.
    let weak = ui.as_weak();
    let st_keys = vstate.clone();
    let vtimer_k = vtimer.clone();
    ui.on_vkey(move |kind| {
        let Some(ui) = weak.upgrade() else { return false };
        let handled = {
            let mut v = st_keys.borrow_mut();
            let step = match kind.as_str() {
                "next" => Some(vfocus::step(v.focused, 1, VCOUNT)),
                "prev" => Some(vfocus::step(v.focused, -1, VCOUNT)),
                "enter-down" => Some(vfocus::step(None, 1, VCOUNT)),
                "enter-up" => Some(vfocus::step(None, -1, VCOUNT)),
                "activate" => {
                    // A placeholder waits for its data, with the loading shown; then it is taken.
                    if let Some(i) = v.focused {
                        let ready = v.pages.item(i) == ItemState::Ready;
                        if let Some(i) = v.activation.press(i, ready) {
                            v.activated = Some(i);
                        }
                    }
                    None
                }
                _ => None,
            };
            match step {
                Some(vfocus::Step::Item(i)) => {
                    // Entering keeps a focus the list already had (a tap, a return).
                    let i = if kind.as_str() == "enter-down" { v.focused.unwrap_or(i) } else { i };
                    // Moving away gives up a wait for data (input events §3.7).
                    v.activation.moved(i);
                    v.focused = Some(i);
                    v.velocity = 0.0;
                    // Revealed below the pinned header, from where the scroll is going (a held
                    // button retargets it), animated unless motion is reduced.
                    let now = started_reveal.elapsed().as_secs_f32();
                    let going = v.reveal.sample(now).0;
                    let from = if v.reveal.running(now) { v.reveal.target() } else { going };
                    let max = (v.extents.total() - v.viewport).max(0.0);
                    let mut area = [nested::RevealArea {
                        scroller: nested::Scroller { offset: from, max, takes_first: false },
                        viewport: v.viewport,
                        at: 0.0,
                        inset_start: HEAD,
                        inset_end: 0.0,
                    }];
                    let top = v.extents.offset(i) + if i % SECTION == 0 { HEAD } else { 0.0 };
                    let to = nested::reveal(&mut area, top, v.extents.offset(i) + v.extents.size(i))[0];
                    v.reveal.reduce_motion = ui.get_reduce_motion();
                    if !v.reveal.running(now) {
                        v.reveal = Animator::new(v.offset, REVEAL);
                    }
                    v.reveal.retarget(to, now, None);
                    v.offset = v.reveal.sample(now).0;
                    if v.reveal.running(now) {
                        // Frames only while it moves.
                        let (st, weak, timer) = (st_keys.clone(), ui.as_weak(), Rc::downgrade(&vtimer_k));
                        vtimer_k.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
                            let Some(ui) = weak.upgrade() else { return };
                            let now = started_reveal.elapsed().as_secs_f32();
                            let running = {
                                let mut v = st.borrow_mut();
                                v.offset = v.reveal.sample(now).0;
                                v.reveal.running(now)
                            };
                            vrender(&st, &ui);
                            if !running && let Some(t) = timer.upgrade() {
                                t.stop();
                            }
                        });
                    }
                    true
                }
                Some(vfocus::Step::Leave) => false,
                None => true,
            }
        };
        vrender(&st_keys, &ui);
        handled
    });
    let started_v = Instant::now();
    let weak = ui.as_weak();
    let st = vstate.clone();
    let vtimer_p = vtimer.clone();
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
                    // A touch stops the reveal's scroll where it is.
                    vtimer_p.stop();
                    v.reveal = Animator::new(v.offset, REVEAL);
                    v.last_y = y;
                    v.last_ms = ms;
                    v.velocity = 0.0;
                    v.travel = 0.0;
                    return;
                }
                // A move, or the release with the last stretch it carries.
                "move" | "up" => {
                    let dy = y - v.last_y;
                    v.travel += dy.abs();
                    // A tap (it did not move past the slop) focuses the row under it.
                    // The pinned header covers the row under it: a tap there focuses nothing.
                    let on_band = (ui.get_vband_y()..ui.get_vband_y() + HEAD).contains(&y);
                    if kind.as_str() == "up" && !on_band && v.travel < opendap_input::thresholds::SLOP_PX * ui.get_unit() {
                        let i = v.extents.index_at(v.offset + y);
                        v.activation.moved(i);
                        v.focused = Some(i);
                    }
                    let dt = ms.saturating_sub(v.last_ms).max(1) as f32 / 1000.0;
                    v.velocity = if kind.as_str() == "up" { 0.0 } else { -dy / dt };
                    v.last_y = y;
                    v.last_ms = ms;
                    let max = (v.extents.total() - v.viewport).max(0.0);
                    v.offset = (v.offset - dy).clamp(0.0, max);
                }
                // Opened, or cancelled (no tap): the scroll is still; costly loads may run.
                _ => v.velocity = 0.0,
            }
        }
        vrender(&st, &ui);
    });

    // The Media screen. A spring (opendap-motion): the block follows the finger, and when let
    // go springs back to the centre with the finger's velocity; frames run only while it moves
    // (the budget), and stop at rest.
    struct SpringState {
        animator: Animator,
        dragging: bool,
        x: f32,
        press_x: f32,
        last: (f32, f32),
        velocity: f32,
        frames: u32,
        budget: Budget,
    }
    let spring = Rc::new(RefCell::new(SpringState {
        animator: Animator::new(0.0, AnimKind::Spring(Spring::SETTLE)),
        dragging: false,
        x: 0.0,
        press_x: 0.0,
        last: (0.0, 0.0),
        velocity: 0.0,
        frames: 0,
        budget: Budget::default(),
    }));
    let started_s = Instant::now();
    let spring_timer = Rc::new(Timer::default());
    let spring_k = spring.clone();
    let weak = ui.as_weak();
    let (sp, st_timer) = (spring.clone(), spring_timer.clone());
    ui.on_spring_drag(move |kind, x| {
        let Some(ui) = weak.upgrade() else { return };
        let now = started_s.elapsed().as_secs_f32();
        let mut s = sp.borrow_mut();
        match kind.as_str() {
            "down" => {
                s.dragging = true;
                s.press_x = x - s.x;
                s.last = (x, now);
                s.velocity = 0.0;
                s.budget.stop(1);
                st_timer.stop();
            }
            "move" if s.dragging => {
                let dt = (now - s.last.1).max(1e-3);
                s.velocity = (x - s.last.0) / dt;
                s.last = (x, now);
                s.x = x - s.press_x;
                ui.set_spring_x(s.x);
            }
            "up" | "kick" => {
                s.dragging = false;
                if kind.as_str() == "up" {
                    let (x0, v0) = (s.x, s.velocity);
                    s.animator = Animator::new(x0, AnimKind::Spring(Spring::SETTLE));
                    s.animator.retarget(0.0, now, Some(v0));
                }
                s.frames = 0;
                s.budget.start(1, None);
                let (sp2, weak2) = (sp.clone(), ui.as_weak());
                let timer_w = Rc::downgrade(&st_timer);
                st_timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
                    let Some(ui) = weak2.upgrade() else { return };
                    let now = started_s.elapsed().as_secs_f32();
                    let mut s = sp2.borrow_mut();
                    let (x, _) = s.animator.sample(now);
                    s.x = x;
                    s.frames += 1;
                    ui.set_spring_x(x);
                    if !s.animator.running(now) {
                        s.budget.stop(1);
                        s.x = 0.0;
                        ui.set_spring_x(0.0);
                    }
                    let at_rest = !s.budget.needs_frame(now);
                    let needs = s.budget.needs_frame(now);
                    ui.set_spring_text(
                        format!(
                            "{}: {} frames{}",
                            if needs { "moving" } else { "at rest" },
                            s.frames,
                            if needs { "" } else { ", then none" }
                        )
                        .into(),
                    );
                    if at_rest {
                        if let Some(t) = timer_w.upgrade() {
                            t.stop();
                        }
                    }
                });
            }
            "open" => {
                // The waveform: a million samples, decimated to the strip's columns.
                let n = 1_000_000usize;
                let samples: Vec<f32> = (0..n)
                    .map(|i| {
                        let t = i as f32 / n as f32;
                        let env = (t * std::f32::consts::PI).sin();
                        let v = env * 0.6 * ((i as f32) * 0.0031).sin() * ((i as f32) * 0.00011).cos();
                        if i == 612_345 { 0.98 } else { v }
                    })
                    .collect();
                let cols = plot::min_max(&samples, 144);
                ui.set_wave_lo(ModelRc::new(VecModel::from(cols.iter().map(|c| c.0).collect::<Vec<_>>())));
                ui.set_wave_hi(ModelRc::new(VecModel::from(cols.iter().map(|c| c.1).collect::<Vec<_>>())));
                let tile = fit::decode_size((3000, 3000), (96.0, 96.0), 1.0, fit::Fit::Cover);
                let screen = fit::decode_size((3000, 3000), (320.0, 320.0), 1.0, fit::Fit::Contain);
                ui.set_media_text(
                    format!(
                        "{n} samples to {} columns, peak {:.2} kept\na 3000 px cover decodes at {}x{} in a tile,\n{}x{} on Now Playing",
                        cols.len(),
                        cols.iter().map(|c| c.1).fold(0.0f32, f32::max),
                        tile.0,
                        tile.1,
                        screen.0,
                        screen.1
                    )
                    .into(),
                );
                ui.set_spring_text("at rest: 0 frames".into());
            }
            _ => {}
        }
    });

    // The Navigation screen's stops (input events §3.7), each navigated by opendap-input's nav:
    // a shelf of albums, and a player whose seek is adjustable inside it (two levels).
    let stops = Rc::new(RefCell::new(Stops {
        // The shelf is a stop over a virtual list (input events §3.7): nav keeps its levels with one
        // child standing for the list, and opendap-layout walks the list's items by index.
        shelf: Navigator::new(NavNode::group("screen", vec![NavNode::stop("shelf", vec![NavNode::leaf("album")])])),
        shelf_index: 0,
        player: Navigator::new(NavNode::group(
            "screen",
            vec![NavNode::stop("player", vec![NavNode::leaf("play"), NavNode::adjustable("seek")])],
        )),
        // 72 px albums 80 px apart, in the 288 px of content.
        shelf_scroll: nested::Scroller::new(SHELF as f32 * 80.0 - 8.0, 288.0),
        seek: 30.0,
    }));

    // Volume past an end of the screen stays there (input events §3.7): Slint's focus chain wraps
    // around the window, so the screen's end stop sends the opposite key back. An edge reached by
    // that key itself is ignored: a screen with nothing focusable would bounce between its ends.
    let weak = ui.as_weak();
    let bouncing = Rc::new(std::cell::Cell::new(false));
    ui.on_screen_edge(move |end| {
        if bouncing.get() {
            return;
        }
        let (weak, bouncing) = (weak.clone(), bouncing.clone());
        Timer::single_shot(Duration::ZERO, move || {
            let Some(ui) = weak.upgrade() else { return };
            bouncing.set(true);
            // Volume + arrives as Shift+Tab: Shift may still be down, which would turn this Tab
            // backward again.
            let shift: SharedString = slint::platform::Key::Shift.into();
            ui.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text: shift });
            let text: SharedString = if end { slint::platform::Key::Backtab.into() } else { slint::platform::Key::Tab.into() };
            ui.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
            ui.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
            // Its own focus changes are handled within a frame; repeats come 50 ms apart at most.
            Timer::single_shot(Duration::from_millis(30), move || bouncing.set(false));
        });
    });

    shelf_tiles(&ui, 0.0);
    // The player shows its seek's value from the start, not from its first key.
    ui.set_nav_player(NavView { focus: "".into(), depth: 0, offset: 0.0, value: stops.borrow().seek });

    // The Keyboard screen's form (opendap-forms): fields of several kinds, their filter before
    // anything is typed, their values bound without echo, issues once a field is left.
    let form = Rc::new(RefCell::new(
        Form::new()
            .field("Profile name", Kind::Text { min: 1, max: Some(24) }, Value::Empty, true)
            .field("Preamp (dB)", Kind::Gain { min: -12.0, max: 0.0, step: 0.5 }, Value::Number(0.0), false)
            .field("Crossfeed (Hz)", Kind::Frequency { min: 300.0, max: 1200.0 }, Value::Number(700.0), false)
            .field("Email", Kind::Email, Value::Empty, false)
            .field("Pairing code", Kind::Pin { len: 6 }, Value::Empty, false)
            .field("Released", Kind::Date, Value::Empty, false)
            .field("Sleep timer", Kind::Duration, Value::Duration(30 * 60_000), false)
            .field("Tags", Kind::Tags { max: Some(5) }, Value::Empty, false),
    ));
    let field_view = |form: &Form, i: usize| {
        let f = &form.fields[i];
        FieldView {
            label: f.name.clone().into(),
            text: f.binding.draft().into(),
            issue: form.shown(&f.name).first().map_or(String::new(), |iss| issue_words(&f.binding.kind, iss)).into(),
            purpose: format!("{:?}", f.binding.kind.purpose()).to_lowercase().into(),
        }
    };
    {
        let f = form.borrow();
        ui.set_kb_fields(ModelRc::new(VecModel::from((0..f.fields.len()).map(|i| field_view(&f, i)).collect::<Vec<_>>())));
    }
    let (weak, form_e) = (ui.as_weak(), form.clone());
    ui.on_field_edited(move |i, text| {
        let Some(ui) = weak.upgrade() else { return text };
        let mut f = form_e.borrow_mut();
        let i = i as usize;
        let name = f.fields[i].name.clone();
        // What this edit inserted (between what stayed before and after it): the kind's filter
        // refuses it before it reaches anything, the text goes back to what it was.
        let prev = f.fields[i].binding.draft().to_string();
        let inserted = inserted_text(&prev, &text);
        if !f.fields[i].binding.kind.allows(&inserted) {
            return prev.into();
        }
        f.input(&name, &text);
        let view = field_view(&f, i);
        ui.get_kb_fields().set_row_data(i, FieldView { text: text.clone(), ..view });
        text
    });
    let (weak, form_l) = (ui.as_weak(), form.clone());
    ui.on_field_left(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        let mut f = form_l.borrow_mut();
        let name = f.fields[i as usize].name.clone();
        f.blur(&name);
        ui.get_kb_fields().set_row_data(i as usize, field_view(&f, i as usize));
    });

    // The keyboard experiment's keys reach the focused field as key events, as a virtual keyboard's
    // do (virtual-keyboard-v1): the field keeps the focus.
    let weak = ui.as_weak();
    ui.on_kb_key(move |k| {
        let Some(ui) = weak.upgrade() else { return };
        let text: SharedString = if k.as_str() == "⌫" { slint::platform::Key::Backspace.into() } else { k };
        ui.window().dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        ui.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
    });

    // A dialog's frosted backdrop: what the window shows, shrunk four times and blurred once
    // (opendap-media's blur), drawn by Blackbird UI's scrim under a graphite tint.
    let weak = ui.as_weak();
    ui.on_frost(move || {
        let Some(ui) = weak.upgrade() else { return };
        let Ok(shot) = ui.window().take_snapshot() else { return };
        let (w, h) = (shot.width() as usize, shot.height() as usize);
        let started = Instant::now();
        let (small, sw, sh) = opendap_media::blur::frosted(shot.as_bytes(), w, h, 4, 12);
        let frost = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&small, sw as u32, sh as u32);
        ui.global::<Backdrop>().set_frost(slint::Image::from_rgba8(frost));
        eprintln!("frosted {w}x{h} in {:?}", started.elapsed());
    });

    // The Layers screen's popover, placed by opendap-input's anchor: below its button, flipped
    // above when it does not fit, slid to stay inside the screen.
    let weak = ui.as_weak();
    ui.on_place_menu(move |x, y, w, h, menu_h| {
        let Some(ui) = weak.upgrade() else { return };
        let size = ui.window().size().to_logical(ui.window().scale_factor());
        let (_, (px, py)) = anchor::place(
            anchor::Rect::new(x, y, w, h),
            (w, menu_h),
            anchor::Side::Below,
            4.0,
            anchor::Rect::new(0.0, 0.0, size.width, size.height),
            ui.get_strip_rtl(),
        );
        ui.set_anchor_x(px);
        ui.set_anchor_y(py);
    });

    // The Transitions screen: a pager (opendap-layout::pager), a morph between a tile and its
    // detail (opendap-motion::transition and interpolate, opendap-input::matched), enter and exit
    // (opendap-motion::presence). Frames only while something moves.
    let tr = Rc::new(RefCell::new(Tr::new()));
    let tr_timer = Rc::new(Timer::default());
    let tr_clock = Instant::now();
    tr_render(&ui, &mut tr.borrow_mut(), 0.0);
    let tr_kick = {
        let (weak, tr, timer) = (ui.as_weak(), tr.clone(), Rc::downgrade(&tr_timer));
        Rc::new(move || {
            let Some(t) = timer.upgrade() else { return };
            if t.running() {
                return;
            }
            let (weak, tr, timer2) = (weak.clone(), tr.clone(), timer.clone());
            t.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
                let Some(ui) = weak.upgrade() else { return };
                let now = tr_clock.elapsed().as_secs_f32();
                let running = tr_render(&ui, &mut tr.borrow_mut(), now);
                if !running && let Some(t) = timer2.upgrade() {
                    t.stop();
                }
            });
        })
    };
    {
        let (weak, tr, kick) = (ui.as_weak(), tr.clone(), tr_kick.clone());
        ui.on_tr_pager(move |kind, x| {
            let Some(ui) = weak.upgrade() else { return };
            let now = tr_clock.elapsed().as_secs_f32();
            let rtl = ui.get_strip_rtl();
            tr.borrow_mut().pager_touch(kind.as_str(), if rtl { -x } else { x }, now);
            tr_render(&ui, &mut tr.borrow_mut(), now);
            kick();
        });
    }
    {
        let (weak, tr, kick) = (ui.as_weak(), tr.clone(), tr_kick.clone());
        ui.on_tr_tile(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let now = tr_clock.elapsed().as_secs_f32();
            tr.borrow_mut().open(i as usize, now, ui.get_reduce_motion());
            kick();
        });
    }
    {
        let (weak, tr, kick) = (ui.as_weak(), tr.clone(), tr_kick.clone());
        ui.on_tr_detail_touch(move |kind, y| {
            let Some(ui) = weak.upgrade() else { return };
            let now = tr_clock.elapsed().as_secs_f32();
            tr.borrow_mut().detail_touch(kind.as_str(), y, now);
            tr_render(&ui, &mut tr.borrow_mut(), now);
            kick();
        });
    }
    {
        let (weak, tr, kick) = (ui.as_weak(), tr.clone(), tr_kick.clone());
        ui.on_tr_list(move || {
            let Some(ui) = weak.upgrade() else { return };
            let now = tr_clock.elapsed().as_secs_f32();
            tr.borrow_mut().toggle_line(now, ui.get_reduce_motion());
            kick();
        });
    }

    // The experiments' controls under the buttons (KeyControl): what each key means for each,
    // through the same primitives the touch uses.
    let (tr_k, tr_kick_k) = (tr.clone(), tr_kick.clone());
    let weak = ui.as_weak();
    ui.on_control_key(move |name, key| {
        let Some(ui) = weak.upgrade() else { return false };
        if matches!(name.as_str(), "pager" | "morph") {
            let now = tr_clock.elapsed().as_secs_f32();
            let handled = tr_k.borrow_mut().key(name.as_str(), key.as_str(), now, ui.get_reduce_motion(), ui.get_strip_rtl());
            tr_kick_k();
            return handled;
        }
        if matches!(name.as_str(), "shelf" | "player") {
            return stop_key(&ui, &mut stops.borrow_mut(), &name, &key);
        }
        // ← → are a keyboard's arrows: more toward the end, mirrored right to left. Volume and
        // ↑ ↓ (inc, dec) are more and less by their place, never mirrored.
        let rtl = ui.get_strip_rtl();
        let key: SharedString = match (key.as_str(), rtl) {
            ("right", false) | ("left", true) => "inc".into(),
            ("left", false) | ("right", true) => "dec".into(),
            _ => key,
        };
        let step = if key.as_str() == "inc" { 1.0 } else { -1.0 };
        match (name.as_str(), key.as_str()) {
            ("dial", "inc" | "dec") => {
                let mut d = dial_k.borrow_mut();
                let to = (d.0 + 15.0 * step).rem_euclid(360.0);
                d.1 += space::angle_delta(d.0, to);
                d.0 = to;
                ui.set_dial_text(format!("angle {:.0}°\nturned {:.0}°", d.0, d.1).into());
        ui.set_dial_angle(d.0);
                true
            }
            ("strip", "inc" | "dec") => {
                let v = (ui.get_strip_value() + 0.05 * step as f32).clamp(0.0, 1.0);
                ui.set_strip_value(v);
                ui.set_strip_text(format!("{v:.2} from the start edge").into());
                true
            }
            ("nested", "inc" | "dec") => {
                // Volume − (dec) scrolls on, as the finger moving up does: the inner list first.
                let mut c = chain_k.borrow_mut();
                let left = nested::scroll(&mut c.0, -24.0 * step as f32);
                ui.set_nested_inner(c.0[0].offset);
                ui.set_nested_outer(c.0[1].offset);
                ui.set_nested_text(format!("inner {:.0}/{:.0}  outer {:.0}/{:.0}  left {:.0}", c.0[0].offset, c.0[0].max, c.0[1].offset, c.0[1].max, left).into());
                true
            }
            ("spring", "activate") => {
                // Action throws the block: the spring takes it with that velocity and brings it back.
                let mut s = spring_k.borrow_mut();
                let now = started_s.elapsed().as_secs_f32();
                s.animator = Animator::new(0.0, AnimKind::Spring(Spring::SETTLE));
                s.animator.retarget(0.0, now, Some(1800.0));
                s.frames = 0;
                drop(s);
                ui.invoke_spring_drag("kick".into(), 0.0);
                true
            }
            ("arena", "activate") => {
                ui.set_arena_text("Action: activate (a semantic action, no arena)".into());
                true
            }
            ("spaces", "next" | "prev" | "enter-down" | "enter-up" | "reveal") => {
                const ROWS: usize = 20;
                let mut l = list_k.borrow_mut();
                let s = match key.as_str() {
                    "next" => opendap_layout::focus::step(l.focused, 1, ROWS),
                    "prev" => opendap_layout::focus::step(l.focused, -1, ROWS),
                    "enter-up" => opendap_layout::focus::step(None, -1, ROWS),
                    _ => opendap_layout::focus::step(l.focused.filter(|_| false), 1, ROWS),
                };
                let opendap_layout::focus::Step::Item(i) = s else { return false };
                // Entering or revealing keeps the item it had.
                let i = if matches!(key.as_str(), "enter-down" | "reveal") { l.focused.unwrap_or(i) } else { i };
                l.focused = Some(i);
                ui.set_spaces_focused(i as i32);
                let rows = opendap_layout::extents::Fixed { count: ROWS, size: 42.0 };
                l.offset = opendap_layout::focus::keep_in_view(&rows, i, l.offset, 168.0);
                ui.set_spaces_offset(l.offset);
                let address = node::NodePath(vec![
                    node::Node::new(node::NodeId::new(node::NodeKind::Activity, "ui-primitives"), gesture::Frame::new((0.0, 0.0), 1.0)),
                    node::Node::new(node::NodeId::new(node::NodeKind::Container, "list"), gesture::Frame::new((288.0, 168.0), 1.0)),
                    node::Node::new(node::NodeId::new(node::NodeKind::Region, "content"), gesture::Frame::new((288.0, 840.0), 1.0).at((0.0, -l.offset))),
                    node::Node::new(node::NodeId::new(node::NodeKind::Element, format!("row-{i}")), gesture::Frame::new((288.0, 42.0), 1.0).at((0.0, i as f32 * 42.0))),
                ])
                .address();
                ui.set_spaces_lines(ModelRc::new(VecModel::from(vec![
                    SharedString::from(address),
                    SharedString::from(format!("focused by the buttons: row-{i}, @content y {:.0}", i as f32 * 42.0)),
                ])));
                true
            }
            ("spaces", "activate") => true,
            _ => false,
        }
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
        // Leading and trailing follow the layout's direction (design system §8.5, §9.5).
        let surface = gesture::Frame { rtl: ui.get_strip_rtl(), ..gesture::Frame::new((w, h), ui.get_unit()) };
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
        // The composer keeps the last frame and restarts the activity (composer spec §9).
        "panic" => panic!("testbed: a panic, asked for"),
        // An unresponsive activity: the event loop blocks, nothing draws.
        "hang" => std::thread::sleep(std::time::Duration::from_secs(5)),
        _ => std::process::exit(0),
    });

    ui.run()
}

/// The Navigation screen's stops and their state.
/// The Navigation screen's shelf: ten thousand albums, only a window of them built.
const SHELF: usize = 10_000;

struct Stops {
    shelf: Navigator,
    /// The album the focus is on inside the shelf, by index (its key).
    shelf_index: usize,
    player: Navigator,
    shelf_scroll: nested::Scroller,
    seek: f32,
}

/// A key on one of the Navigation screen's stops, through its navigator; false lets the focus
/// move on (past the stop at its own level) or the screen go back.
fn stop_key(ui: &Testbed, st: &mut Stops, name: &str, key: &str) -> bool {
    let shelf = name == "shelf";
    let k = match (key, ui.get_strip_rtl()) {
        ("next", _) => Some(nav::Key::Next),
        ("prev", _) => Some(nav::Key::Previous),
        ("activate", _) => Some(nav::Key::Action),
        ("escape", _) => Some(nav::Key::Escape),
        // A keyboard's arrows, in reading order: mirrored right to left.
        ("right", false) | ("left", true) => Some(nav::Key::Forward),
        ("left", false) | ("right", true) => Some(nav::Key::Backward),
        _ => None,
    };
    // Inside the engaged shelf, Volume and the arrows walk its list by index (opendap-layout);
    // past either end the shelf lets go, as nav does for any stop.
    let along = match k {
        Some(nav::Key::Next | nav::Key::Forward) => Some(1),
        Some(nav::Key::Previous | nav::Key::Backward) => Some(-1),
        _ => None,
    };
    if shelf
        && st.shelf.depth() == 1
        && let Some(d) = along
    {
        let said = match vfocus::step(Some(st.shelf_index), d, SHELF) {
            vfocus::Step::Item(i) => {
                st.shelf_index = i;
                format!("moved to album {}", i + 1)
            }
            vfocus::Step::Leave => {
                st.shelf.key(nav::Key::Escape);
                "released shelf".into()
            }
        };
        stop_view(ui, st, name, &said);
        return true;
    }
    let (handled, said) = match (k, key) {
        (Some(k), _) => {
            let e = if shelf { st.shelf.key(k) } else { st.player.key(k) };
            if shelf {
                // Entering: at the first album, or the last from the far side (← in reading order).
                match &e {
                    Effect::Engaged(_) => st.shelf_index = 0,
                    Effect::Moved(n) if n == "album" => st.shelf_index = SHELF - 1,
                    _ => {}
                }
            }
            let said = match &e {
                Effect::Moved(n) => format!("moved to {n}"),
                Effect::Engaged(n) => format!("engaged {n}"),
                Effect::Released(n) => format!("released {n}"),
                Effect::Activated(n) if n == "album" => format!("activated album {}", st.shelf_index + 1),
                Effect::Activated(n) => format!("activated {n}"),
                Effect::Adjusted(n, d) => {
                    st.seek = (st.seek + 2.0 * *d as f32).clamp(0.0, 100.0);
                    format!("{n} {d:+}")
                }
                Effect::Refused(n) => format!("{n} refused"),
                Effect::AtEnd => "past the end: on".into(),
                Effect::Back => "back".into(),
                Effect::Closed { layer, to } => format!("closed {layer}, to {to}"),
            };
            (!matches!(e, Effect::AtEnd | Effect::Back), said)
        }
        _ => {
            let nav = if shelf { &mut st.shelf } else { &mut st.player };
            match key {
                // Keys arrived on the stop: it, not inside it.
                "enter" => {
                    nav.touched(name);
                    (true, format!("on {name}"))
                }
                // The focus hid or left: every level releases.
                "release" => {
                    nav.release_all();
                    nav.touched(name);
                    (true, format!("{name} released"))
                }
                t if t.starts_with("touch:") => {
                    let what = &t["touch:".len()..];
                    match what.strip_prefix('a').and_then(|n| n.parse::<usize>().ok()) {
                        Some(i) if shelf => {
                            st.shelf_index = i;
                            st.shelf.touched("album");
                            (true, format!("touched album {}", i + 1))
                        }
                        _ => {
                            nav.touched(what);
                            (true, format!("touched {what}"))
                        }
                    }
                }
                _ => (false, String::new()),
            }
        }
    };
    stop_view(ui, st, name, &said);
    handled
}

/// What a stop shows after a key: the focus inside it, its level, and for the shelf, the built
/// window of albums scrolled to keep the focused one in view.
fn stop_view(ui: &Testbed, st: &mut Stops, name: &str, said: &str) {
    let shelf = name == "shelf";
    let nav = if shelf { &st.shelf } else { &st.player };
    let inside = nav.focus().filter(|f| *f != name).map(str::to_string);
    let depth = nav.depth() as i32;
    let focus = match inside.as_deref() {
        Some("album") => format!("a{}", st.shelf_index),
        Some(f) => f.to_string(),
        None => String::new(),
    };
    let mut offset = 0.0;
    if shelf {
        if inside.is_some() {
            let x = st.shelf_index as f32 * 80.0;
            let mut area = [nested::RevealArea { scroller: st.shelf_scroll, viewport: 288.0, at: 0.0, inset_start: 0.0, inset_end: 0.0 }];
            nested::reveal(&mut area, x, x + 72.0);
            st.shelf_scroll = area[0].scroller;
        }
        offset = st.shelf_scroll.offset;
        shelf_tiles(ui, offset);
    }
    let view = NavView { focus: focus.clone().into(), depth, offset, value: st.seek };
    if shelf {
        ui.set_nav_shelf(view);
    } else {
        ui.set_nav_player(view);
    }
    if !said.is_empty() {
        let at = if focus.is_empty() { format!(", on {name}") } else { format!(", in {name}: {focus}") };
        ui.set_nav_text(format!("{said}\nlevel {depth}{at}").into());
    }
}

/// The albums to build for the shelf at `offset`: only the window around what shows.
fn shelf_tiles(ui: &Testbed, offset: f32) {
    let extents = opendap_layout::extents::Fixed { count: SHELF, size: 80.0 };
    let o = Overscan { around: 80.0, lead_s: 0.0, max_ahead: 0.0 };
    let tiles: Vec<NavTile> = window(&extents, offset, 288.0, 0.0, o).map(|i| NavTile { index: i as i32 }).collect();
    ui.set_nav_tiles(ModelRc::new(VecModel::from(tiles)));
}

/// An issue in words, for the field it is about (English here; the interface's language on the
/// device, from its catalogue).
fn issue_words(kind: &Kind, issue: &Issue) -> String {
    match issue {
        Issue::Required => "Required".into(),
        Issue::Malformed => match kind {
            Kind::Email => "Not an email address".into(),
            Kind::Date => "A date as YYYY-MM-DD".into(),
            Kind::Pin { len } => format!("{len} digits"),
            Kind::Duration => "A time as m:ss or h:mm:ss".into(),
            _ => "Not understood".into(),
        },
        Issue::BelowMin(m) => format!("At least {m}"),
        Issue::AboveMax(m) => format!("At most {m}"),
        Issue::OffStep(s) => format!("In steps of {s}"),
        Issue::TooShort(n) => format!("At least {n}"),
        Issue::TooLong(n) => format!("At most {n}"),
        Issue::Custom(k) => k.clone(),
    }
}

/// What an edit put in, between what stayed before it and after it ("" for a delete).
fn inserted_text(before: &str, after: &str) -> String {
    let (b, a): (Vec<char>, Vec<char>) = (before.chars().collect(), after.chars().collect());
    let start = b.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let room = b.len().min(a.len()) - start;
    let end = b.iter().rev().zip(a.iter().rev()).take(room).take_while(|(x, y)| x == y).count();
    a[start..a.len() - end].iter().collect()
}

/// The Transitions screen's state: the pager, the morph, the lines that enter and leave.
struct Tr {
    /// The pager: its offset (pixels into the strip, in reading order), the page it rests on, its
    /// settling animation, and the drag in progress (where it began, the last move, its velocity).
    page: usize,
    pager: Animator,
    drag: Option<(f32, f32, f32, f32)>,
    velocity: f32,
    /// The morph: the tile it opened from and its transition; a downward drag on the detail.
    morph: Transition,
    tile: usize,
    detail_drag: Option<(f32, f32)>,
    matched: Matched,
    /// The lines and whether line 2 is removed.
    lines: Presence<usize>,
    removed: bool,
}

/// The stage's sizes, in design pixels: the content's width, the morph's stage, its tiles.
const TR_W: f32 = 288.0;
const TR_PAGES: usize = 3;
const TR_DETAIL: (f32, f32, f32, f32) = (0.0, 0.0, 288.0, 190.0);

impl Tr {
    fn new() -> Self {
        let mut matched = Matched::default();
        for i in 0..3 {
            matched.declare("tiles", &i.to_string(), MSide::Source, ARect::new(16.0 + i as f32 * 96.0, 120.0, 64.0, 64.0));
            matched.declare("tiles", &i.to_string(), MSide::Destination, ARect::new(TR_DETAIL.0, TR_DETAIL.1, TR_DETAIL.2, TR_DETAIL.3));
        }
        let mut lines = Presence::default();
        lines.update(&[0, 1, 2, 3], -10.0);
        Tr {
            page: 0,
            pager: Animator::new(0.0, AnimKind::Spring(Spring::SETTLE)),
            drag: None,
            velocity: 0.0,
            morph: Transition::new(TEnd::From, TR_DETAIL.3),
            tile: 0,
            detail_drag: None,
            matched,
            lines,
            removed: false,
        }
    }

    fn offset(&self, now: f32) -> f32 {
        match self.drag {
            Some((x0, _, _, off0)) => {
                let raw = off0 + (x0 - self.drag.unwrap().1);
                let max = (TR_PAGES - 1) as f32 * TR_W;
                // Past the ends the strip resists the finger (rubber band).
                if raw < 0.0 {
                    resist(raw, TR_W)
                } else if raw > max {
                    max + resist(raw - max, TR_W)
                } else {
                    raw
                }
            }
            None => self.pager.sample(now).0,
        }
    }

    fn pager_touch(&mut self, kind: &str, x: f32, now: f32) {
        match kind {
            "down" => {
                let off = self.offset(now);
                self.drag = Some((x, x, now, off));
                self.velocity = 0.0;
            }
            "move" => {
                if let Some((x0, last, t, off0)) = self.drag {
                    let dt = (now - t).max(0.001);
                    self.velocity = (last - x) / dt;
                    self.drag = Some((x0, x, now, off0));
                }
            }
            _ => {
                let off = self.offset(now);
                // Only recent motion counts: a finger that rested before lifting has no velocity
                // (every velocity tracker's window, input events §7).
                if self.drag.is_some_and(|(_, _, t, _)| now - t > 0.05) {
                    self.velocity = 0.0;
                }
                self.drag = None;
                let from = self.page;
                self.page = settle(from, off, self.velocity, TR_W, TR_PAGES);
                self.pager = Animator::new(off, AnimKind::Spring(Spring::SETTLE));
                self.pager.retarget(self.page as f32 * TR_W, now, Some(self.velocity));
            }
        }
    }

    fn turn(&mut self, d: i32, now: f32, reduce: bool) {
        let to = (self.page as i32 + d).clamp(0, TR_PAGES as i32 - 1) as usize;
        self.page = to;
        let off = self.offset(now);
        self.pager = Animator::new(off, AnimKind::Timed { duration: 0.2, curve: Bezier::STANDARD });
        self.pager.reduce_motion = reduce;
        self.pager.retarget(to as f32 * TR_W, now, None);
    }

    fn open(&mut self, tile: usize, now: f32, reduce: bool) {
        self.morph.reduce_motion = reduce;
        let open = self.morph.settled(now) == Some(TEnd::To) || self.morph.running(now) && self.morph.progress(now) > 0.0 && self.tile == tile;
        if open {
            self.morph.go(TEnd::From, now);
        } else {
            self.tile = tile;
            self.morph.go(TEnd::To, now);
        }
    }

    fn detail_touch(&mut self, kind: &str, y: f32, now: f32) {
        match kind {
            "down" => {
                self.morph.grab(now);
                self.detail_drag = Some((y, now));
                self.velocity = 0.0;
            }
            "move" => {
                if let Some((last, t)) = self.detail_drag {
                    let dt = (now - t).max(0.001);
                    // Down is toward closing: the transition's way back.
                    self.morph.drag(-(y - last));
                    self.velocity = -(y - last) / dt;
                    self.detail_drag = Some((y, now));
                }
            }
            _ => {
                if self.detail_drag.is_some_and(|(_, t)| now - t > 0.05) {
                    self.velocity = 0.0;
                }
                let moved = self.detail_drag.take().is_some_and(|_| self.velocity.abs() > 1.0);
                if moved || self.morph.progress(now) < 0.98 {
                    self.morph.release(self.velocity, now);
                } else {
                    // A tap on the open detail: back.
                    self.morph.release(0.0, now);
                    self.morph.go(TEnd::From, now);
                }
            }
        }
    }

    fn toggle_line(&mut self, now: f32, reduce: bool) {
        self.removed = !self.removed;
        self.lines.reduce_motion = reduce;
        let keys: Vec<usize> = (0..4).filter(|k| !(self.removed && *k == 1)).collect();
        self.lines.update(&keys, now);
    }

    fn key(&mut self, name: &str, key: &str, now: f32, reduce: bool, _rtl: bool) -> bool {
        match (name, key) {
            ("pager", "inc") => self.turn(1, now, reduce),
            ("pager", "dec") => self.turn(-1, now, reduce),
            ("morph", "activate") => self.open(self.tile, now, reduce),
            _ => return false,
        }
        true
    }
}

/// Places the Transitions screen at `now`; whether anything still moves.
fn tr_render(ui: &Testbed, t: &mut Tr, now: f32) -> bool {
    let off = t.offset(now);
    ui.set_tr_pager_offset(off);
    ui.set_tr_page(t.page as i32);
    let p = t.morph.progress(now);
    let pairs = t.matched.pairs("tiles");
    let tile_frame = |r: ARect, radius: f32| Frame::new(r.origin.0, r.origin.1, r.size.0, r.size.1, radius);
    let tiles: Vec<TrBox> = pairs
        .iter()
        .enumerate()
        .map(|(i, (_, from, _))| {
            let f = tile_frame(*from, 8.0);
            // The opened tile is the detail now; the others recede.
            let opacity = if i == t.tile && p > 0.0 { 0.0 } else { 1.0 - 0.6 * p };
            TrBox { x: f.x, y: f.y, w: f.w, h: f.h, radius: f.radius, opacity, label: format!("Album {}", i + 1).into() }
        })
        .collect();
    ui.set_tr_tiles(ModelRc::new(VecModel::from(tiles)));
    let (_, from, to) = pairs[t.tile];
    let f = frame(tile_frame(from, 8.0), tile_frame(to, 16.0), p);
    ui.set_tr_detail(TrBox { x: f.x, y: f.y, w: f.w, h: f.h, radius: f.radius, opacity: if p > 0.001 { 1.0 } else { 0.0 }, label: format!("Album {}", t.tile + 1).into() });
    let mut y = 0.0;
    let rows: Vec<TrBox> = t
        .lines
        .frame(now)
        .into_iter()
        .map(|(k, _, prog)| {
            let h = 40.0 * prog;
            let r = TrBox { x: 0.0, y, w: TR_W, h, radius: 0.0, opacity: prog, label: format!("Line {}", k + 1).into() };
            y += h;
            r
        })
        .collect();
    ui.set_tr_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_tr_text(
        format!(
            "page {} of {TR_PAGES}, offset {off:.0}\nmorph {p:.2}{}\nlines {}",
            t.page + 1,
            match t.morph.settled(now) {
                Some(TEnd::To) => ", open",
                Some(TEnd::From) => ", closed",
                None => ", moving",
            },
            if t.removed { "1, 3, 4 (2 removed)" } else { "1 to 4" }
        )
        .into(),
    );
    t.pager.running(now) || t.morph.running(now) || t.lines.running(now) || t.drag.is_some() || t.detail_drag.is_some()
}
