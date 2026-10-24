//! Display output: DRM/KMS with GBM buffers and the GLES renderer, on a libseat session.
//!
//! Whatever the hardware offers is used: the first connected connector, its preferred mode and
//! the CRTC and planes it can drive. Nothing about a board is assumed (hardware support spec §9).
//! A frame is drawn only after a client commits, and at most once per refresh (spec §5).

use std::{os::fd::AsFd, path::PathBuf, time::Duration};

use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::{
    backend::{
        allocator::{
            Fourcc,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode,
            compositor::{DrmCompositor, FrameFlags},
            exporter::gbm::{GbmFramebufferExporter, NodeFilter},
        },
        egl::{EGLContext, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{Color32F, ImportDma, ImportMemWl, gles::GlesRenderer},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev,
    },
    output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        drm::control::{Device as _, ModeTypeFlags, connector},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::{DeviceFd, Transform},
};
use tracing::{info, warn};

use smithay::backend::renderer::element::{
    AsRenderElements, memory::MemoryRenderBufferRenderElement, surface::WaylandSurfaceRenderElement,
};
use smithay::utils::Scale as UtilScale;

use super::{splash::Splash, state::Shell};

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

/// The design system's background (`semantic.bg`, graphite 950), shown where no client draws.
const CLEAR: Color32F = Color32F::new(0.0667, 0.0745, 0.0824, 1.0);

smithay::backend::renderer::element::render_elements! {
    pub ShellElement<=GlesRenderer>;
    Window=WaylandSurfaceRenderElement<GlesRenderer>,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
}

pub struct Backend {
    splash: Splash,
    splash_timer: bool,
    touches_timer: bool,
    session: LibSeatSession,
    libinput: Libinput,
    renderer: GlesRenderer,
    compositor: Compositor,
    drm: DrmDevice,
    start: std::time::Instant,
    frame_pending: bool,
    redraw: bool,
    /// The display's power: off after a short press of `main` (controls.rs).
    display_off: bool,
    /// A screenshot is asked for (Main + Volume −): taken with the next frame.
    screenshot: bool,
}

impl Backend {
    pub fn schedule(&mut self) {
        self.redraw = true;
    }

    pub fn request_screenshot(&mut self) {
        self.screenshot = true;
        self.redraw = true;
    }

    pub fn display_on(&self) -> bool {
        !self.display_off
    }

    /// The display off (DPMS off, every plane disabled; clients get no frame callbacks, so they
    /// stop drawing) or on again with a full frame.
    pub fn set_display(&mut self, on: bool) {
        if on == !self.display_off {
            return;
        }
        if on {
            self.display_off = false;
            if let Err(err) = self.compositor.reset_state() {
                warn!(?err, "display on");
            }
            self.redraw = true;
        } else {
            if let Err(err) = self.compositor.clear() {
                warn!(?err, "display off");
            }
            self.display_off = true;
            self.frame_pending = false;
        }
    }

    /// Lets the display go to a boot UI that is armed to cover this shell's restart (odap-boot
    /// cover): drops DRM master with the last frame still on screen, and waits until the boot UI's
    /// splash screen has replaced it, so the display shows no blank frame. Without an armed boot UI
    /// it returns at once.
    pub fn hand_over(&self) {
        use opendap_splash::{COVER_ARMED_PATH, COVER_SHOWN_PATH};
        use smithay::reexports::drm::Device as _;
        if !std::path::Path::new(COVER_ARMED_PATH).exists() {
            return;
        }
        if let Err(err) = self.drm.device_fd().release_master_lock() {
            warn!(%err, "cannot let the display go");
            return;
        }
        let deadline = std::time::Instant::now() + HAND_OVER_TIMEOUT;
        while !std::path::Path::new(COVER_SHOWN_PATH).exists()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// How long a toast stays (Android's short toast).
const TOAST: Duration = Duration::from_secs(2);

/// The longest a stopping shell waits for the boot UI to take the display.
const HAND_OVER_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session: {0}")]
    Session(String),
    #[error("no display device on this seat")]
    NoGpu,
    #[error("display device: {0}")]
    Drm(String),
    #[error("no connected display")]
    NoConnector,
    #[error("renderer: {0}")]
    Render(String),
}

pub fn init(shell: &mut Shell) -> Result<(), Error> {
    let (mut session, notifier) =
        LibSeatSession::new().map_err(|e| Error::Session(e.to_string()))?;
    let seat = session.seat();

    let path: PathBuf = udev::primary_gpu(&seat)
        .ok()
        .flatten()
        .or_else(|| udev::all_gpus(&seat).ok()?.into_iter().next())
        .ok_or(Error::NoGpu)?;
    info!(gpu = %path.display(), "display device");

    let fd = session
        .open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| Error::Session(e.to_string()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (mut drm, drm_notifier) =
        DrmDevice::new(fd.clone(), true).map_err(|e| Error::Drm(e.to_string()))?;
    let gbm = GbmDevice::new(fd).map_err(|e| Error::Drm(e.to_string()))?;
    let node = DrmNode::from_file(gbm.as_fd()).ok();

    // Safety: the EGL display lives as long as the renderer, which this backend owns.
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| Error::Render(e.to_string()))?;
    let context = EGLContext::new(&egl).map_err(|e| Error::Render(e.to_string()))?;
    let renderer =
        unsafe { GlesRenderer::new(context) }.map_err(|e| Error::Render(e.to_string()))?;

    // The first connected connector and a CRTC it can use.
    let resources = drm
        .resource_handles()
        .map_err(|e| Error::Drm(e.to_string()))?;
    let connector = resources
        .connectors()
        .iter()
        .filter_map(|c| drm.get_connector(*c, true).ok())
        .find(|c| c.state() == connector::State::Connected)
        .ok_or(Error::NoConnector)?;
    let crtc = connector
        .encoders()
        .iter()
        .filter_map(|e| drm.get_encoder(*e).ok())
        .flat_map(|e| resources.filter_crtcs(e.possible_crtcs()))
        .next()
        .ok_or(Error::NoConnector)?;
    // The panel's own size from the device description first, then a mode set on the kernel
    // command line (USERDEF), then the connector's preferred one.
    let native = shell
        .description
        .display
        .as_ref()
        .map(|d| (d.width_px as u16, d.height_px as u16));
    let modes = connector.modes();
    let mode = modes
        .iter()
        .find(|m| Some(m.size()) == native)
        .or_else(|| {
            modes
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::USERDEF))
        })
        .or_else(|| {
            modes
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
        })
        .or_else(|| modes.first())
        .copied()
        .ok_or(Error::NoConnector)?;

    // Physical size: what the connector reports, else the device description's.
    let (mut w_mm, mut h_mm) = connector
        .size()
        .map(|(w, h)| (w as i32, h as i32))
        .unwrap_or((0, 0));
    if (w_mm == 0 || h_mm == 0)
        && let Some(d) = shell.description.display.as_ref()
    {
        (w_mm, h_mm) = (d.width_mm.round() as i32, d.height_mm.round() as i32);
    }
    let scale = shell
        .description
        .display
        .as_ref()
        .map(|d| d.scale().round().max(1.0) as i32)
        .unwrap_or(1);
    let name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );
    let output = Output::new(
        name.clone(),
        PhysicalProperties {
            size: (w_mm, h_mm).into(),
            subpixel: Subpixel::Unknown,
            make: "OpenDAP".into(),
            model: shell.description.identity.name.clone(),
            serial_number: String::new(),
        },
    );
    let output_mode = OutputMode::from(mode);
    output.set_preferred(output_mode);
    output.change_current_state(
        Some(output_mode),
        Some(Transform::Normal),
        Some(Scale::Integer(scale)),
        Some((0, 0).into()),
    );
    output.create_global::<Shell>(&shell.display_handle);
    shell.space.map_output(&output, (0, 0));
    info!(connector = %name, w = mode.size().0, h = mode.size().1, w_mm, h_mm, scale, "output");

    let surface = drm
        .create_surface(crtc, mode, &[connector.handle()])
        .map_err(|e| Error::Drm(e.to_string()))?;
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(
        gbm.clone(),
        node.map(NodeFilter::Node).unwrap_or(NodeFilter::All),
    );
    let planes = drm.planes(&crtc).ok();
    let compositor = Compositor::new(
        &output,
        surface,
        planes,
        allocator,
        exporter,
        [
            Fourcc::Argb8888,
            Fourcc::Xrgb8888,
            Fourcc::Abgr8888,
            Fourcc::Xbgr8888,
        ],
        renderer.dmabuf_formats(),
        (64, 64).into(),
        Some(gbm),
    )
    .map_err(|e| Error::Render(e.to_string()))?;

    shell.shm_state.update_formats(renderer.shm_formats());

    // Input from every device of the seat, through libinput.
    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|_| Error::Session("libinput cannot use the seat".into()))?;
    shell
        .handle
        .insert_source(
            LibinputInputBackend::new(libinput.clone()),
            |event, _, shell| super::input::process(shell, event),
        )
        .map_err(|e| Error::Session(e.to_string()))?;

    shell
        .handle
        .insert_source(drm_notifier, |event, _, shell| match event {
            DrmEvent::VBlank(_) => frame_done(shell),
            DrmEvent::Error(err) => warn!(?err, "display"),
        })
        .map_err(|e| Error::Drm(e.to_string()))?;

    shell
        .handle
        .insert_source(notifier, |event, _, shell| {
            let Some(backend) = shell.backend.as_mut() else {
                return;
            };
            match event {
                SessionEvent::PauseSession => {
                    info!("session paused");
                    backend.libinput.suspend();
                }
                SessionEvent::ActivateSession => {
                    info!("session active");
                    if backend.libinput.resume().is_err() {
                        warn!("libinput did not resume");
                    }
                    if let Err(err) = backend.compositor.reset_state() {
                        warn!(?err, "display state reset");
                    }
                    backend.frame_pending = false;
                    backend.redraw = true;
                    render_if_idle(shell);
                }
            }
        })
        .map_err(|e| Error::Session(e.to_string()))?;

    shell.output = Some(output);
    shell.backend = Some(Backend {
        splash: Splash::new((mode.size().0 as i32, mode.size().1 as i32).into()),
        splash_timer: false,
        touches_timer: false,
        session,
        libinput,
        renderer,
        compositor,
        drm,
        start: std::time::Instant::now(),
        frame_pending: false,
        redraw: true,
        display_off: false,
        screenshot: false,
    });
    // The system layers, at the display's size, with `main` where the description puts it.
    let main_y = shell
        .description
        .display
        .as_ref()
        .zip(shell.description.control(opendap_device::Role::Main))
        .map(|(d, c)| d.mm_to_px(c.offset_mm) / scale as f32)
        .unwrap_or(mode.size().1 as f32 / scale as f32 / 4.0);
    // The physical controls for the debug UI mode, where the description places them: one mark
    // per role (a role with several codes, a host key too, is drawn once).
    let mut controls: Vec<(String, String, f32, f32)> = Vec::new();
    if let Some(d) = shell.description.display.as_ref() {
        for c in &shell.description.controls {
            let role = kebab(&format!("{:?}", c.role));
            if controls.iter().any(|(r, ..)| *r == role) {
                continue;
            }
            controls.push((
                role,
                format!("{:?}", c.edge).to_lowercase(),
                d.mm_to_px(c.offset_mm) / scale as f32,
                d.mm_to_px(c.length_mm) / scale as f32,
            ));
        }
    }
    // Logical pixels per design pixel: the hardware's exact scale over the applied one.
    let unit = shell
        .description
        .display
        .as_ref()
        .map(|d| d.scale() / scale as f32)
        .unwrap_or(1.0);
    shell.layers = super::layers::SystemLayers::new(
        (mode.size().0 as u32, mode.size().1 as u32),
        scale as f32,
        main_y,
        controls,
        unit,
    );
    render_if_idle(shell);
    Ok(())
}

/// Draws if something changed and no frame is waiting for the display.
pub fn render_if_idle(shell: &mut Shell) {
    let handle = shell.handle.clone();
    let Shell {
        space,
        output,
        backend,
        touches,
        layers,
        ..
    } = shell;
    let (Some(backend), Some(output)) = (backend.as_mut(), output.as_ref()) else {
        return;
    };
    if backend.display_off
        || backend.frame_pending
        || !backend.redraw
        || !backend.session.is_active()
    {
        return;
    }
    backend.redraw = false;
    let scale = UtilScale::from(output.current_scale().fractional_scale());
    let mut windows: Vec<ShellElement> = Vec::new();
    for window in space.elements() {
        let location = space
            .element_location(window)
            .unwrap_or_default()
            .to_physical_precise_round(scale);
        windows.extend(window.render_elements::<ShellElement>(
            &mut backend.renderer,
            location,
            scale,
            1.0,
        ));
    }
    // The product host's first frame lets the splash screen go (splash.rs).
    if !windows.is_empty() {
        backend.splash.client_ready();
        // Booting ends at the first frame on screen: the status LED stops its booting state.
        static READY: std::sync::Once = std::sync::Once::new();
        READY.call_once(|| {
            let _ = std::fs::write("/run/opendap/ui-ready", b"");
        });
    } else if backend.splash.done() && !layers.as_ref().is_some_and(|l| l.power_open()) {
        // No product host on screen after boot: it exited and is being restarted. The display
        // keeps the last frame until the new host draws, instead of going black (spec §9).
        return;
    }
    let mut elements: Vec<ShellElement> = Vec::with_capacity(windows.len() + 3);
    // Above everything: the touch indicator, the system layers, the splash screen, the clients.
    elements.extend(
        touches
            .elements(&mut backend.renderer, scale.x)
            .into_iter()
            .map(ShellElement::Memory),
    );
    let touch_marks = elements.len();
    elements.extend(
        layers
            .as_mut()
            .and_then(|l| l.element(&mut backend.renderer))
            .map(ShellElement::Memory),
    );
    elements.extend(
        backend
            .splash
            .element(&mut backend.renderer)
            .map(ShellElement::Memory),
    );
    elements.extend(windows);
    // A screenshot shows what the display shows, without the touch indicator (a debug aid).
    if std::mem::take(&mut backend.screenshot) {
        let size = output.current_mode().map(|m| m.size).unwrap_or_default();
        let shown = &elements[touch_marks..];
        let saved = super::screenshot::capture(&mut backend.renderer, shown, size, scale, CLEAR);
        let message = match saved {
            Ok(path) => {
                info!(path = %path.display(), "screenshot");
                "Screenshot saved"
            }
            Err(err) => {
                warn!(%err, "screenshot");
                "Screenshot failed"
            }
        };
        if let Some(l) = layers.as_mut() {
            l.show_toast(message);
            let _ = handle.insert_source(Timer::from_duration(TOAST), |_, _, shell| {
                if let Some(l) = shell.layers.as_mut() {
                    l.hide_toast();
                }
                if let Some(b) = shell.backend.as_mut() {
                    b.schedule();
                }
                render_if_idle(shell);
                TimeoutAction::Drop
            });
            backend.redraw = true;
        }
    }
    // While the splash screen fades, every refresh draws; while it waits out its minimum, a timer
    // wakes the shell at the end of it (a static frame has no damage, so no refresh comes).
    if backend.splash.animating()
        || touches.animating()
        || layers.as_ref().is_some_and(|l| l.animating())
    {
        backend.redraw = true;
    }
    // The resting pointer's bubble fades out on its own: one timer, never a periodic one.
    if let Some(at) = touches.wake_at()
        && !backend.touches_timer
    {
        backend.touches_timer = true;
        let _ = handle.insert_source(Timer::from_deadline(at), |_, _, shell| {
            if let Some(b) = shell.backend.as_mut() {
                b.touches_timer = false;
                b.redraw = true;
            }
            render_if_idle(shell);
            TimeoutAction::Drop
        });
    }
    if let Some(at) = backend.splash.wake_at()
        && !backend.splash_timer
    {
        backend.splash_timer = true;
        let _ = handle.insert_source(Timer::from_deadline(at), |_, _, shell| {
            if let Some(b) = shell.backend.as_mut() {
                b.splash_timer = false;
                b.redraw = true;
            }
            render_if_idle(shell);
            TimeoutAction::Drop
        });
    }
    match backend.compositor.render_frame(
        &mut backend.renderer,
        &elements,
        CLEAR,
        FrameFlags::DEFAULT,
    ) {
        Ok(result) if !result.is_empty => match backend.compositor.queue_frame(()) {
            Ok(()) => backend.frame_pending = true,
            Err(err) => warn!(?err, "queue frame"),
        },
        // Nothing visible changed: no frame, but clients may draw their next one.
        Ok(_) => send_frames(space, output, backend.start.elapsed()),
        Err(err) => warn!(?err, "render frame"),
    }
}

fn frame_done(shell: &mut Shell) {
    if let (Some(backend), Some(output)) = (shell.backend.as_mut(), shell.output.as_ref()) {
        if let Err(err) = backend.compositor.frame_submitted() {
            warn!(?err, "frame submitted");
        }
        backend.frame_pending = false;
        send_frames(&shell.space, output, backend.start.elapsed());
    }
    render_if_idle(shell);
}

/// One frame callback per refresh, to the visible surfaces only (spec §5).
fn send_frames(
    space: &smithay::desktop::Space<smithay::desktop::Window>,
    output: &Output,
    time: Duration,
) {
    for window in space.elements() {
        window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
}

/// `VolumeUp` as descriptions and the layers write it, `volume-up`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    out
}
