//! Compositor state and the Wayland protocol handlers it implements.

use std::{ffi::OsString, sync::Arc};

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{Space, Window, WindowSurfaceType},
    input::{Seat, SeatHandler, SeatState, pointer::CursorImageStatus},
    output::Output,
    reexports::{
        calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Client, Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Logical, Point, Serial},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface,
        },
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::PointerConstraintsHandler,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
    },
};

use opendap_device::Description;

use super::backend::Backend;

pub struct Composer {
    pub handle: LoopHandle<'static, Composer>,
    pub display_handle: DisplayHandle,
    pub socket_name: OsString,
    pub description: Description,

    pub space: Space<Window>,
    pub output: Option<Output>,
    pub backend: Option<Backend>,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub _output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Composer>,
    pub seat: Seat<Composer>,
    /// An absolute pointer acting as one finger (input.rs).
    pub pointer_location: Point<f64, Logical>,
    pub pointer_contact: bool,
    /// The touch indicator, a developer option (touches.rs).
    pub touches: super::touches::Touches,
    /// The system control's presses (controls.rs).
    pub controls: super::controls::Controls,
    /// The keys a role sent the activity when it went down, to release the same ones (input.rs).
    pub forwarded: std::collections::HashMap<opendap_device::Role, &'static [u32]>,
    /// Counts the focus steps held down, so a held button's repeat stops with its release.
    pub repeat_gen: u64,
    /// The generation of an attached keyboard's held key (input.rs `keyboard_repeat`).
    pub key_repeat_gen: u64,
    /// The volume role whose press became a long press (a track), so its release steps nothing.
    pub volume_long: Option<opendap_device::Role>,
    /// What the status bar shows (status.rs).
    pub status: super::status::Status,
    /// The system layers (layers.rs), once the display is up.
    pub layers: Option<super::layers::SystemLayers>,
    /// Where the last contact was, for a touch's release (which has no position).
    pub last_touch: Point<f64, Logical>,
    /// With the display off, the one gesture the screen takes: a double tap wakes the display.
    pub wake: super::input::Wake,
    /// Contacts that started in the status bar: the system's until they end (input.rs).
    pub system_contacts: Vec<smithay::backend::input::TouchSlot>,
    /// A ping to the activity still unanswered, since when (input.rs: input to an activity that
    /// does not respond is dropped, never queued).
    pub ping_since: Option<std::time::Instant>,
    /// Which client that ping went to: only its pong answers it.
    pub pinged: Option<smithay::wayland::shell::xdg::ShellClient>,
    /// A ping is due shortly after the last key (input.rs).
    pub ping_scheduled: bool,
}

impl Composer {
    pub fn new(
        handle: LoopHandle<'static, Composer>,
        display: Display<Composer>,
        description: Description,
    ) -> Self {
        let dh = display.handle();
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        // A touch screen is the reference input. The buttons are roles the composer resolves first
        // (input.rs); what passes to the activity arrives as the standard keys of focus
        // navigation, on a keyboard with no auto-repeat (a held button never floods the focus).
        seat.add_touch();
        seat.add_keyboard(smithay::input::keyboard::XkbConfig::default(), 200, 0)
            .expect("the seat's keyboard");
        // A pointer for hover only (a mouse, the simulator window): contact is always touch.
        seat.add_pointer();

        let socket = ListeningSocketSource::new_auto().expect("no free Wayland socket name");
        let socket_name = socket.socket_name().to_os_string();
        handle
            .insert_source(socket, |stream, _, composer| {
                if let Err(err) = composer
                    .display_handle
                    .insert_client(stream, Arc::new(ClientState::default()))
                {
                    tracing::warn!(?err, "client refused");
                }
            })
            .expect("cannot listen on the Wayland socket");
        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, composer| {
                    // Safety: the display is owned by this event source and never dropped while in use.
                    unsafe { display.get_mut().dispatch_clients(composer).unwrap() };
                    Ok(PostAction::Continue)
                },
            )
            .expect("cannot watch the Wayland display");

        Self {
            handle,
            display_handle: dh,
            socket_name,
            description,
            space: Space::default(),
            output: None,
            backend: None,
            compositor_state,
            xdg_shell_state,
            shm_state,
            _output_manager_state: output_manager_state,
            seat_state,
            seat,
            pointer_location: Point::from((0.0, 0.0)),
            pointer_contact: false,
            touches: super::touches::Touches::new(),
            controls: Default::default(),
            forwarded: Default::default(),
            repeat_gen: 0,
            key_repeat_gen: 0,
            status: Default::default(),
            volume_long: None,
            layers: None,
            last_touch: Point::from((0.0, 0.0)),
            wake: super::input::Wake::default(),
            system_contacts: Vec::new(),
            ping_since: None,
            pinged: None,
            ping_scheduled: false,
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, (p + location).to_f64()))
            })
    }

    /// Every client surface is the instrument's full screen: no windows, no decorations.
    fn configure_full_screen(&self, surface: &ToplevelSurface) {
        let size = self
            .output
            .as_ref()
            .and_then(|o| self.space.output_geometry(o))
            .map(|g| g.size);
        surface.with_pending_state(|state| {
            state.size = size;
            state.states.set(xdg_toplevel::State::Fullscreen);
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl CompositorHandler for Composer {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &root))
            {
                window.on_commit();
            }
        }
        // Damage arrived: draw on the next refresh. Without commits nothing is drawn (spec §5).
        if let Some(backend) = self.backend.as_mut() {
            backend.schedule();
        }
        super::backend::render_if_idle(self);
    }
}

impl BufferHandler for Composer {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Composer {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl XdgShellHandler for Composer {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // The activity answered: it responds (input.rs).
    fn client_pong(&mut self, client: smithay::wayland::shell::xdg::ShellClient) {
        if self.pinged.as_ref().is_none_or(|p| *p == client) {
            self.ping_since = None;
            self.pinged = None;
        }
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // A new activity starts answered: a ping left by one that went away is not held against it.
        self.ping_since = None;
        self.pinged = None;
        self.configure_full_screen(&surface);
        // The activity on screen takes the keys the buttons send it (input.rs).
        let wl = surface.wl_surface().clone();
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), true);
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(wl), smithay::utils::SERIAL_COUNTER.next_serial());
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        self.configure_full_screen(&surface);
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.configure_full_screen(&surface);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if self.pinged.as_ref().is_some_and(|p| *p == surface.client()) {
            self.ping_since = None;
            self.pinged = None;
        }
        let gone: Vec<Window> = self
            .space
            .elements()
            .filter(|w| w.toplevel().is_some_and(|t| t == &surface))
            .cloned()
            .collect();
        for window in gone {
            self.space.unmap_elem(&window);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.schedule();
        }
        super::backend::render_if_idle(self);
    }

    // The product host draws its own menus inside its surface; popups are not offered.
    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        surface.send_popup_done();
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
    }
}

impl SeatHandler for Composer {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Composer> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}

impl PointerConstraintsHandler for Composer {}

impl OutputHandler for Composer {}

smithay::delegate_dispatch2!(Composer);
