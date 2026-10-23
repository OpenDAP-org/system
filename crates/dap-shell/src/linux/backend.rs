//! Display output: DRM/KMS with GBM buffers and the GLES renderer, on a libseat session.
//!
//! Whatever the hardware offers is used: the first connected connector, its preferred mode and
//! the CRTC and planes it can drive. Nothing about a board is assumed (hardware support spec §9).
//! A frame is drawn only after a client commits, and at most once per refresh (spec §5).

use std::{os::fd::AsFd, path::PathBuf, time::Duration};

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

use super::state::Shell;

type Compositor = DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

/// The design system's background (`semantic.bg`, graphite 950), shown where no client draws.
const CLEAR: Color32F = Color32F::new(0.0667, 0.0745, 0.0824, 1.0);

pub struct Backend {
    session: LibSeatSession,
    libinput: Libinput,
    renderer: GlesRenderer,
    compositor: Compositor,
    _drm: DrmDevice,
    start: std::time::Instant,
    frame_pending: bool,
    redraw: bool,
}

impl Backend {
    pub fn schedule(&mut self) {
        self.redraw = true;
    }
}

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
    let (mut session, notifier) = LibSeatSession::new().map_err(|e| Error::Session(e.to_string()))?;
    let seat = session.seat();

    let path: PathBuf = udev::primary_gpu(&seat)
        .ok()
        .flatten()
        .or_else(|| udev::all_gpus(&seat).ok()?.into_iter().next())
        .ok_or(Error::NoGpu)?;
    info!(gpu = %path.display(), "display device");

    let fd = session
        .open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)
        .map_err(|e| Error::Session(e.to_string()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (mut drm, drm_notifier) = DrmDevice::new(fd.clone(), true).map_err(|e| Error::Drm(e.to_string()))?;
    let gbm = GbmDevice::new(fd).map_err(|e| Error::Drm(e.to_string()))?;
    let node = DrmNode::from_file(gbm.as_fd()).ok();

    // Safety: the EGL display lives as long as the renderer, which this backend owns.
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| Error::Render(e.to_string()))?;
    let context = EGLContext::new(&egl).map_err(|e| Error::Render(e.to_string()))?;
    let renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| Error::Render(e.to_string()))?;

    // The first connected connector and a CRTC it can use.
    let resources = drm.resource_handles().map_err(|e| Error::Drm(e.to_string()))?;
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
    let native = shell.description.display.as_ref().map(|d| (d.width_px as u16, d.height_px as u16));
    let modes = connector.modes();
    let mode = modes
        .iter()
        .find(|m| Some(m.size()) == native)
        .or_else(|| modes.iter().find(|m| m.mode_type().contains(ModeTypeFlags::USERDEF)))
        .or_else(|| modes.iter().find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED)))
        .or_else(|| modes.first())
        .copied()
        .ok_or(Error::NoConnector)?;

    // Physical size: what the connector reports, else the device description's.
    let (mut w_mm, mut h_mm) = connector.size().map(|(w, h)| (w as i32, h as i32)).unwrap_or((0, 0));
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
    let name = format!("{}-{}", connector.interface().as_str(), connector.interface_id());
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
    output.change_current_state(Some(output_mode), Some(Transform::Normal), Some(Scale::Integer(scale)), Some((0, 0).into()));
    output.create_global::<Shell>(&shell.display_handle);
    shell.space.map_output(&output, (0, 0));
    info!(connector = %name, w = mode.size().0, h = mode.size().1, w_mm, h_mm, scale, "output");

    let surface = drm.create_surface(crtc, mode, &[connector.handle()]).map_err(|e| Error::Drm(e.to_string()))?;
    let allocator = GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let exporter = GbmFramebufferExporter::new(gbm.clone(), node.map(NodeFilter::Node).unwrap_or(NodeFilter::All));
    let planes = drm.planes(&crtc).ok();
    let compositor = Compositor::new(
        &output,
        surface,
        planes,
        allocator,
        exporter,
        [Fourcc::Argb8888, Fourcc::Xrgb8888, Fourcc::Abgr8888, Fourcc::Xbgr8888],
        renderer.dmabuf_formats(),
        (64, 64).into(),
        Some(gbm),
    )
    .map_err(|e| Error::Render(e.to_string()))?;

    shell.shm_state.update_formats(renderer.shm_formats());

    // Input from every device of the seat, through libinput.
    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput.udev_assign_seat(&seat).map_err(|_| Error::Session("libinput cannot use the seat".into()))?;
    shell
        .handle
        .insert_source(LibinputInputBackend::new(libinput.clone()), |event, _, shell| {
            super::input::process(shell, event)
        })
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
            let Some(backend) = shell.backend.as_mut() else { return };
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
        session,
        libinput,
        renderer,
        compositor,
        _drm: drm,
        start: std::time::Instant::now(),
        frame_pending: false,
        redraw: true,
    });
    render_if_idle(shell);
    Ok(())
}

/// Draws if something changed and no frame is waiting for the display.
pub fn render_if_idle(shell: &mut Shell) {
    let Shell { space, output, backend, .. } = shell;
    let (Some(backend), Some(output)) = (backend.as_mut(), output.as_ref()) else { return };
    if backend.frame_pending || !backend.redraw || !backend.session.is_active() {
        return;
    }
    backend.redraw = false;
    let elements = match space.render_elements_for_output(&mut backend.renderer, output, 1.0) {
        Ok(elements) => elements,
        Err(err) => {
            warn!(?err, "render elements");
            return;
        }
    };
    match backend
        .compositor
        .render_frame(&mut backend.renderer, &elements, CLEAR, FrameFlags::DEFAULT)
    {
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
fn send_frames(space: &smithay::desktop::Space<smithay::desktop::Window>, output: &Output, time: Duration) {
    for window in space.elements() {
        window.send_frame(output, time, Some(Duration::ZERO), |_, _| Some(output.clone()));
    }
}
