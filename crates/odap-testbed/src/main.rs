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
use opendap_input::{
    arena, gesture,
    nav::{self, Effect, NavNode, Navigator},
    nested, node, space,
};
use opendap_media::{fit, plot};
use opendap_motion::{
    animator::{Animator, Kind as AnimKind},
    easing::Bezier,
    budget::Budget,
    spring::Spring,
};
use opendap_layout::{
    data::{
        cache::Cache,
        gate::{Cost, Gate},
        pages::{ItemState, Pages, Paging},
    },
    extents::{Extents, Measured},
    focus as vfocus,
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
        shelf: Navigator::new(NavNode::group(
            "screen",
            vec![NavNode::stop("shelf", (1..=8).map(|i| NavNode::leaf(&format!("a{i}"))).collect())],
        )),
        player: Navigator::new(NavNode::group(
            "screen",
            vec![NavNode::stop("player", vec![NavNode::leaf("play"), NavNode::adjustable("seek")])],
        )),
        // Eight 72 px albums 80 px apart, in the 288 px of content.
        shelf_scroll: nested::Scroller::new(8.0 * 80.0 - 8.0, 288.0),
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

    // The experiments' controls under the buttons (KeyControl): what each key means for each,
    // through the same primitives the touch uses.
    let weak = ui.as_weak();
    ui.on_control_key(move |name, key| {
        let Some(ui) = weak.upgrade() else { return false };
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

/// The Navigation screen's stops and their state.
struct Stops {
    shelf: Navigator,
    player: Navigator,
    shelf_scroll: nested::Scroller,
    seek: f32,
}

/// A key on one of the Navigation screen's stops, through its navigator; false lets the focus
/// move on (past the stop at its own level) or the screen go back.
fn stop_key(ui: &Testbed, st: &mut Stops, name: &str, key: &str) -> bool {
    let rtl = ui.get_strip_rtl();
    let nav = if name == "shelf" { &mut st.shelf } else { &mut st.player };
    let k = match (key, rtl) {
        ("next", _) => Some(nav::Key::Next),
        ("prev", _) => Some(nav::Key::Previous),
        ("activate", _) => Some(nav::Key::Action),
        ("escape", _) => Some(nav::Key::Escape),
        // A keyboard's arrows, in reading order: mirrored right to left.
        ("right", false) | ("left", true) => Some(nav::Key::Forward),
        ("left", false) | ("right", true) => Some(nav::Key::Backward),
        _ => None,
    };
    let (handled, said) = match (k, key) {
        (Some(k), _) => {
            let e = nav.key(k);
            let said = match &e {
                Effect::Moved(n) => format!("moved to {n}"),
                Effect::Engaged(n) => format!("engaged {n}"),
                Effect::Released(n) => format!("released {n}"),
                Effect::Activated(n) => format!("activated {n}"),
                Effect::Adjusted(n, d) => {
                    st.seek = (st.seek + 2.0 * *d as f32).clamp(0.0, 100.0);
                    format!("{n} {:+}", d)
                }
                Effect::Refused(n) => format!("{n} refused"),
                Effect::AtEnd => "past the end: on".into(),
                Effect::Back => "back".into(),
                Effect::Closed { layer, to } => format!("closed {layer}, to {to}"),
            };
            (!matches!(e, Effect::AtEnd | Effect::Back), said)
        }
        // Keys arrived on the stop: it, not inside it.
        (None, "enter") => {
            nav.touched(name);
            (true, format!("on {name}"))
        }
        // The focus hid or left: every level releases.
        (None, "release") => {
            nav.release_all();
            nav.touched(name);
            (true, format!("{name} released"))
        }
        (None, t) if t.starts_with("touch:") => {
            nav.touched(&t["touch:".len()..]);
            (true, format!("touched {}", &t["touch:".len()..]))
        }
        _ => (false, String::new()),
    };
    let focus = nav.focus().filter(|f| *f != name).unwrap_or("").to_string();
    let depth = nav.depth() as i32;
    let mut offset = 0.0;
    if name == "shelf" {
        // The focused album kept in view in the shelf (nested::reveal, one area).
        if let Some(i) = focus.strip_prefix('a').and_then(|n| n.parse::<f32>().ok()) {
            let mut area = [nested::RevealArea { scroller: st.shelf_scroll, viewport: 288.0, at: 0.0, inset_start: 0.0, inset_end: 0.0 }];
            nested::reveal(&mut area, (i - 1.0) * 80.0, (i - 1.0) * 80.0 + 72.0);
            st.shelf_scroll = area[0].scroller;
        }
        offset = st.shelf_scroll.offset;
    }
    let view = NavView { focus: focus.clone().into(), depth, offset, value: st.seek };
    if name == "shelf" {
        ui.set_nav_shelf(view);
    } else {
        ui.set_nav_player(view);
    }
    if !said.is_empty() {
        ui.set_nav_text(format!("{said}\nlevel {depth}{}", if focus.is_empty() { format!(", on {name}") } else { format!(", in {name}: {focus}") }).into());
    }
    handled
}
