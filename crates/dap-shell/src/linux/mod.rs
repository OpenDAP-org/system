mod backend;
mod input;
mod state;

use std::{path::PathBuf, process::Command};

use opendap_device::Description;
use smithay::reexports::{
    calloop::{EventLoop, channel},
    wayland_server::Display,
};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, prelude::*};

use state::Shell;

const DEFAULT_DESCRIPTION: &str = "/usr/share/opendap/device/description.toml";

struct Args {
    description: PathBuf,
    /// The product host, started once the display is up, until the orchestrator starts it.
    client: Vec<String>,
}

fn args() -> Args {
    let mut description = PathBuf::from(DEFAULT_DESCRIPTION);
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
                eprintln!("dap-shell: unknown argument {other}\nusage: dap-shell [--description FILE] [-- COMMAND...]");
                std::process::exit(2);
            }
        }
    }
    Args { description, client }
}

fn logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    // Native journal fields on the device (ADR 0008); plain stderr elsewhere.
    match tracing_journald::layer() {
        Ok(journald) if std::env::var_os("JOURNAL_STREAM").is_some() => registry.with(journald).init(),
        _ => registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr)).init(),
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
    let mut shell = Shell::new(event_loop.handle(), event_loop.get_signal(), display, description);

    if let Err(err) = backend::init(&mut shell) {
        error!(%err, "display");
        std::process::exit(1);
    }
    info!(socket = ?shell.socket_name, "listening");

    // Until the orchestrator owns activities, the shell's lifetime is the product host's: if it
    // exits, the shell exits too and systemd restarts the session (dap-shell.service).
    if let Some((program, rest)) = args.client.split_first() {
        match Command::new(program).args(rest).env("WAYLAND_DISPLAY", &shell.socket_name).spawn() {
            Ok(mut child) => {
                info!(pid = child.id(), %program, "product host started");
                let (sender, exited) = channel::channel::<std::process::ExitStatus>();
                std::thread::spawn(move || {
                    if let Ok(status) = child.wait() {
                        let _ = sender.send(status);
                    }
                });
                event_loop
                    .handle()
                    .insert_source(exited, |event, _, shell| {
                        if let channel::Event::Msg(status) = event {
                            error!(%status, "product host exited");
                            shell.signal.stop();
                        }
                    })
                    .expect("cannot watch the product host");
            }
            Err(err) => {
                error!(%err, %program, "product host");
                std::process::exit(1);
            }
        }
    }

    event_loop
        .run(None, &mut shell, |shell| {
            let _ = shell.display_handle.flush_clients();
        })
        .expect("event loop");
    std::process::exit(1);
}
