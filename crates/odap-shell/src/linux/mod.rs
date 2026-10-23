mod backend;
mod input;
mod key_names;
mod splash;
mod state;
mod touches;

use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use calloop::signals::{Signal, Signals};
use opendap_device::Description;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, prelude::*};

use state::Shell;

/// Where the device description is found, first match wins: the one the hardware provides at boot
/// (a virtual board's firmware, a board's own identification), then a board support package's.
const DESCRIPTIONS: &[&str] = &[
    "/run/opendap/device/description.toml",
    "/usr/share/opendap/device/description.toml",
];

struct Args {
    description: PathBuf,
    /// The product host, started once the display is up, until the orchestrator starts it.
    client: Vec<String>,
}

fn args() -> Args {
    let mut description = DESCRIPTIONS
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(DESCRIPTIONS[0]));
    let mut client = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--description" => description = it.next().map(PathBuf::from).unwrap_or(description),
            "--" => {
                client = it.collect();
                break;
            }
            other => {
                eprintln!(
                    "odap-shell: unknown argument {other}\nusage: odap-shell [--description FILE] [-- COMMAND...]"
                );
                std::process::exit(2);
            }
        }
    }
    Args {
        description,
        client,
    }
}

fn logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    // Native journal fields on the device (ADR 0008); plain stderr elsewhere.
    match tracing_journald::layer() {
        Ok(journald) if std::env::var_os("JOURNAL_STREAM").is_some() => {
            registry.with(journald).init()
        }
        _ => registry
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init(),
    }
}

pub fn main() {
    logging();
    let args = args();

    let description = match std::fs::read_to_string(&args.description)
        .map_err(|e| e.to_string())
        .and_then(|text| Description::parse(&text).map_err(|e| e.to_string()))
    {
        Ok(d) => d,
        Err(err) => {
            error!(path = %args.description.display(), %err, "device description");
            std::process::exit(1);
        }
    };
    info!(device = %description.identity.id, "starting");

    let mut event_loop: EventLoop<'static, Shell> = EventLoop::try_new().expect("event loop");
    let display: Display<Shell> = Display::new().expect("Wayland display");
    let mut shell = Shell::new(event_loop.handle(), display, description);

    if let Err(err) = backend::init(&mut shell) {
        error!(%err, "display");
        std::process::exit(1);
    }
    info!(socket = ?shell.socket_name, "listening");

    // Asked to stop (systemd's SIGTERM): hand the display to the boot UI, then exit. Set up before
    // any thread starts, so every thread keeps these signals blocked for the event loop; the
    // product host, a child process, starts with them unblocked.
    let signals = Signals::new(&[Signal::SIGTERM, Signal::SIGINT]).expect("signals");
    event_loop
        .handle()
        .insert_source(signals, |_, _, shell| {
            if let Some(backend) = shell.backend.as_ref() {
                backend.hand_over();
            }
            info!("stopped");
            std::process::exit(0);
        })
        .expect("cannot watch for signals");

    // Until the orchestrator owns activities (ADR 0002), the shell supervises the product host: it
    // restarts it when it exits, and the display keeps the last frame meanwhile.
    if !args.client.is_empty() {
        supervise_product(args.client, shell.socket_name.clone());
    }

    event_loop
        .run(None, &mut shell, |shell| {
            let _ = shell.display_handle.flush_clients();
        })
        .expect("event loop");
    std::process::exit(1);
}

/// The first restart is quick; repeated failures wait longer, up to `RESTART_MAX`. A host that
/// stayed up `HEALTHY` counts as recovered, and its next restart is quick again.
const RESTART_MIN: Duration = Duration::from_millis(250);
const RESTART_MAX: Duration = Duration::from_secs(8);
const HEALTHY: Duration = Duration::from_secs(30);

/// Runs the product host for as long as the shell runs, restarting it when it exits.
fn supervise_product(command: Vec<String>, socket: std::ffi::OsString) {
    std::thread::Builder::new()
        .name("product-host".into())
        .spawn(move || {
            let (program, rest) = command.split_first().expect("a command");
            let mut backoff = RESTART_MIN;
            loop {
                let started = Instant::now();
                let outcome = Command::new(program).args(rest).env("WAYLAND_DISPLAY", &socket).spawn().and_then(|mut child| {
                    info!(pid = child.id(), %program, "product host started");
                    child.wait()
                });
                if started.elapsed() >= HEALTHY {
                    backoff = RESTART_MIN;
                }
                match outcome {
                    Ok(status) => warn!(%status, restart_in_ms = backoff.as_millis() as u64, "product host exited"),
                    Err(err) => error!(%err, %program, restart_in_ms = backoff.as_millis() as u64, "product host"),
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(RESTART_MAX);
            }
        })
        .expect("cannot start the product host's supervisor");
}
