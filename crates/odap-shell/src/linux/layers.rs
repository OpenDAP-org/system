//! The system layers (spec §2, §7): Slint components the shell draws itself, above every client,
//! with Slint's software renderer into a buffer the compositor places on top. While a layer is up,
//! touch and the pointer go to it only: no client sees them.
//!
//! Today: the power menu, beside `main`, and the debug UI mode (SYS-070): the blueprint's
//! references over the real interface, while its flag is on. That layer takes no input.

use std::{cell::RefCell, rc::Rc, time::Instant};

use slint::{
    ComponentHandle, LogicalPosition, PhysicalSize,
    platform::{
        Platform, PointerEventButton, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
    },
};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                Kind,
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    utils::{Logical, Point, Transform},
};
use tracing::warn;

slint::include_modules!();

/// Slint's platform for the shell: one window, the layers, drawn on demand.
struct ShellPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for ShellPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

/// What a layer asked for, handled by the shell (controls.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Restart,
    RestartToRecovery,
    Cancel,
}

/// The debug UI mode's runtime switch; the environment's default is `ODAP_DEBUG_UI`.
const DEBUG_SWITCH: &str = "/run/opendap/debug-ui";

/// Whether the debug UI mode is on: the switch file, or the environment.
pub fn debug_ui() -> bool {
    std::path::Path::new(DEBUG_SWITCH).exists()
        || std::env::var("ODAP_DEBUG_UI").is_ok_and(|v| v == "1")
}

/// A contact the debug UI mode follows, to light its region and name its gesture.
struct DebugContact {
    at: (f32, f32),
    since: Instant,
}

pub struct SystemLayers {
    window: Rc<MinimalSoftwareWindow>,
    ui: Layers,
    width: usize,
    height: usize,
    pixels: Vec<PremultipliedRgbaColor>,
    buffer: Option<MemoryRenderBuffer>,
    requests: Rc<RefCell<Vec<Request>>>,
    controls: Rc<slint::VecModel<ControlMark>>,
    contact: Option<DebugContact>,
    /// Logical pixels per design pixel (the hardware's exact scale over the applied one).
    unit: f32,
    /// Counts the gestures shown, so a callout's timer clears only its own.
    gesture_seq: u64,
}

impl SystemLayers {
    /// For a display of `size` physical pixels at `scale` physical pixels per logical pixel, with
    /// `main` centred `main_y` logical pixels from the top (the device description).
    /// `controls`: each physical control's role, edge, centre and length (logical pixels); `unit`:
    /// logical pixels per design pixel.
    pub fn new(
        size: (u32, u32),
        scale: f32,
        main_y: f32,
        controls: Vec<(String, String, f32, f32)>,
        unit: f32,
    ) -> Option<Self> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        let platform = ShellPlatform {
            window: window.clone(),
            start: Instant::now(),
        };
        if let Err(err) = slint::platform::set_platform(Box::new(platform)) {
            warn!(%err, "system layers");
            return None;
        }
        window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        window.set_size(PhysicalSize::new(size.0, size.1));
        let ui = match Layers::new() {
            Ok(ui) => ui,
            Err(err) => {
                warn!(%err, "system layers");
                return None;
            }
        };
        ui.set_main_y(main_y);
        ui.set_unit(unit);
        let marks = Rc::new(slint::VecModel::from(
            controls
                .into_iter()
                .map(|(role, edge, centre, span)| ControlMark {
                    role: role.into(),
                    edge: edge.into(),
                    centre,
                    span,
                    down: false,
                })
                .collect::<Vec<_>>(),
        ));
        ui.set_controls(slint::ModelRc::from(marks.clone()));
        ui.set_debug(debug_ui());
        let requests = Rc::new(RefCell::new(Vec::new()));
        let sink = requests.clone();
        ui.on_power_choice(move |what| {
            let request = match what.as_str() {
                "restart" => Request::Restart,
                "recovery" => Request::RestartToRecovery,
                _ => Request::Cancel,
            };
            sink.borrow_mut().push(request);
        });
        if let Err(err) = ui.show() {
            warn!(%err, "system layers");
            return None;
        }
        let (width, height) = (size.0 as usize, size.1 as usize);
        Some(Self {
            window,
            ui,
            width,
            height,
            pixels: vec![PremultipliedRgbaColor::default(); width * height],
            buffer: None,
            requests,
            controls: marks,
            contact: None,
            unit,
            gesture_seq: 0,
        })
    }

    pub fn power_open(&self) -> bool {
        self.ui.get_power_open()
    }

    /// Whether any layer is on screen.
    fn visible(&self) -> bool {
        self.power_open() || !self.ui.get_toast().is_empty() || self.ui.get_debug()
    }

    /// Reads the debug UI mode's flag again; true when it changed.
    pub fn refresh_debug(&mut self) -> bool {
        let on = debug_ui();
        if on == self.ui.get_debug() {
            return false;
        }
        self.ui.set_debug(on);
        self.window.request_redraw();
        if !self.visible() {
            self.buffer = None;
        }
        true
    }

    /// A physical control went down or up (every role, system combinations included).
    pub fn control(&mut self, role: &str, down: bool) {
        use slint::Model;
        for i in 0..self.controls.row_count() {
            let mut m = self.controls.row_data(i).unwrap();
            if m.role == role && m.down != down {
                m.down = down;
                self.controls.set_row_data(i, m);
                self.window.request_redraw();
            }
        }
    }

    /// A contact for the debug UI mode, at `at` (logical pixels): its crosshair and region while
    /// it is down; when it lifts, the gesture it was, to show for a while (the caller clears it).
    pub fn debug_contact(&mut self, at: Point<f64, Logical>, event: Contact) -> Option<u64> {
        if !self.ui.get_debug() {
            return None;
        }
        let at = (at.x as f32, at.y as f32);
        let size = (
            self.width as f32 / self.window.scale_factor(),
            self.height as f32 / self.window.scale_factor(),
        );
        let mut gesture = None;
        match event {
            Contact::Down => {
                self.contact = Some(DebugContact {
                    at,
                    since: Instant::now(),
                });
                self.ui.set_touch_zone(zone(at, size, self.unit).into());
                self.ui.set_touching(true);
            }
            Contact::Moved => {}
            Contact::Up => {
                if let Some(c) = self.contact.take() {
                    let g = opendap_gesture::classify(
                        c.at,
                        at,
                        c.since.elapsed().as_millis() as u64,
                        size,
                        self.unit,
                    );
                    gesture = Some(g.describe());
                }
                self.ui.set_touching(false);
            }
        }
        self.ui.set_touch_x(at.0);
        self.ui.set_touch_y(at.1);
        self.window.request_redraw();
        let text = gesture?;
        self.ui.set_gesture(text.as_str().into());
        self.gesture_seq += 1;
        Some(self.gesture_seq)
    }

    /// The gesture's callout leaves, unless a newer gesture took its place.
    pub fn clear_gesture(&mut self, seq: u64) {
        if seq == self.gesture_seq {
            self.ui.set_gesture("".into());
            self.window.request_redraw();
        }
    }

    /// A short message that leaves by itself; the shell clears it with `hide_toast`.
    pub fn show_toast(&mut self, text: &str) {
        self.ui.set_toast(text.into());
        self.window.request_redraw();
    }

    pub fn hide_toast(&mut self) {
        self.ui.set_toast("".into());
        self.window.request_redraw();
        if !self.visible() {
            self.buffer = None;
        }
    }

    pub fn open_power(&mut self, can_recovery: bool) {
        self.ui.set_can_recovery(can_recovery);
        self.ui.set_point_at_main(false);
        // Focus on entry (design system §8.4): the first choice.
        self.ui.set_focused_choice(0);
        self.ui.set_power_open(true);
        self.window.request_redraw();
    }

    pub fn close(&mut self) {
        self.ui.set_power_open(false);
        self.window.request_redraw();
        if !self.visible() {
            self.buffer = None;
        }
    }

    /// The number of choices the volume buttons move through: Power off, Restart, Restart to
    /// recovery when present, Cancel.
    fn choices(&self) -> i32 {
        if self.ui.get_can_recovery() { 4 } else { 3 }
    }

    /// Volume + (`-1`, up) or − (`1`, down): the focus moves, wrapping around (it starts on the
    /// first choice; none only if something cleared it).
    pub fn move_focus(&mut self, step: i32) {
        let n = self.choices();
        let focus = self.ui.get_focused_choice();
        let next = if focus < 0 {
            if step > 0 { 0 } else { n - 1 }
        } else {
            (focus + step).rem_euclid(n)
        };
        self.ui.set_focused_choice(next);
        self.window.request_redraw();
    }

    /// Action: the screen's primary action, or else the focused element (T1 controls spec §1.3).
    /// Here the primary action is Power off, which Action only points at; with a choice focused,
    /// Action takes it as a touch on it would.
    pub fn activate_focus(&mut self) {
        let n = self.choices();
        let request = match self.ui.get_focused_choice() {
            // No focus: the menu's primary action, Power off, which only Main confirms (spec §7).
            0 | -1 => {
                self.ui.set_point_at_main(true);
                self.window.request_redraw();
                None
            }
            1 => Some(Request::Restart),
            2 if n == 4 => Some(Request::RestartToRecovery),
            f if f == n - 1 => Some(Request::Cancel),
            _ => None,
        };
        if let Some(r) = request {
            self.requests.borrow_mut().push(r);
        }
    }

    /// Whether a layer takes input now; then no client gets it.
    pub fn takes_input(&self) -> bool {
        self.power_open()
    }

    /// A contact at `at` (logical pixels): down, moved, or up.
    pub fn contact(&mut self, at: Point<f64, Logical>, event: Contact) {
        let position = LogicalPosition::new(at.x as f32, at.y as f32);
        let event = match event {
            Contact::Down => WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            },
            Contact::Moved => WindowEvent::PointerMoved { position },
            Contact::Up => WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            },
        };
        let released = matches!(event, WindowEvent::PointerReleased { .. });
        self.window.dispatch_event(event);
        // A finger leaves the screen: nothing stays hovered.
        if released {
            self.window.dispatch_event(WindowEvent::PointerExited);
        }
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests.borrow_mut())
    }

    /// Whether Slint has animations running (frames must keep coming).
    pub fn animating(&self) -> bool {
        self.visible() && self.window.has_active_animations()
    }

    /// The layers' element, above every client, while a layer is up.
    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.visible() {
            return None;
        }
        slint::platform::update_timers_and_animations();
        let (width, height) = (self.width, self.height);
        let pixels = &mut self.pixels;
        let drawn = self.window.draw_if_needed(|r| {
            pixels.fill(PremultipliedRgbaColor::default());
            r.render(pixels, width);
        });
        if drawn || self.buffer.is_none() {
            // Slint's premultiplied RGBA to the compositor's ARGB8888 (B, G, R, A bytes).
            let mut bytes = Vec::with_capacity(width * height * 4);
            for p in &self.pixels {
                bytes.extend_from_slice(&[p.blue, p.green, p.red, p.alpha]);
            }
            self.buffer = Some(MemoryRenderBuffer::from_slice(
                &bytes,
                Fourcc::Argb8888,
                (width as i32, height as i32),
                1,
                Transform::Normal,
                None,
            ));
        }
        let buffer = self.buffer.as_ref()?;
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0.0, 0.0),
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .map_err(|err| warn!(?err, "system layers"))
        .ok()
    }
}

/// The region a contact starts in, for the debug UI mode to light it; `unit` as in `classify`.
fn zone(at: (f32, f32), size: (f32, f32), unit: f32) -> &'static str {
    let edge = opendap_gesture::EDGE_PX * unit;
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

#[derive(Clone, Copy)]
pub enum Contact {
    Down,
    Moved,
    Up,
}
