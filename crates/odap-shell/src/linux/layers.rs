//! The system layers (spec §2, §7): Slint components the shell draws itself, above every client,
//! with Slint's software renderer into a buffer the compositor places on top. While a layer is up,
//! touch and the pointer go to it only: no client sees them.
//!
//! Today: the power menu, beside `main`.

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

pub struct SystemLayers {
    window: Rc<MinimalSoftwareWindow>,
    ui: Layers,
    width: usize,
    height: usize,
    pixels: Vec<PremultipliedRgbaColor>,
    buffer: Option<MemoryRenderBuffer>,
    requests: Rc<RefCell<Vec<Request>>>,
}

impl SystemLayers {
    /// For a display of `size` physical pixels at `scale` physical pixels per logical pixel, with
    /// `main` centred `main_y` logical pixels from the top (the device description).
    pub fn new(size: (u32, u32), scale: f32, main_y: f32) -> Option<Self> {
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
        })
    }

    pub fn power_open(&self) -> bool {
        self.ui.get_power_open()
    }

    pub fn open_power(&mut self, can_recovery: bool) {
        self.ui.set_can_recovery(can_recovery);
        self.ui.set_point_at_main(false);
        self.ui.set_focused_choice(-1);
        self.ui.set_power_open(true);
        self.window.request_redraw();
    }

    pub fn close(&mut self) {
        self.ui.set_power_open(false);
        self.buffer = None;
    }

    /// The number of choices the volume buttons move through: Power off, Restart, Restart to
    /// recovery when present, Cancel.
    fn choices(&self) -> i32 {
        if self.ui.get_can_recovery() { 4 } else { 3 }
    }

    /// Volume + (`-1`, up) or − (`1`, down): the focus moves, from none to the first choice.
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

    /// Action: the focused choice, as a touch on it would.
    pub fn activate_focus(&mut self) {
        let n = self.choices();
        let request = match self.ui.get_focused_choice() {
            0 => {
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
        self.power_open() && self.window.has_active_animations()
    }

    /// The layers' element, above every client, while a layer is up.
    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.power_open() {
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

#[derive(Clone, Copy)]
pub enum Contact {
    Down,
    Moved,
    Up,
}
