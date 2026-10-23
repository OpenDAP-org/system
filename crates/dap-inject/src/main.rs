//! `dap-inject`: touch contacts for automated tests on a running instance (development only;
//! shipped in the development system extension, never in images).
//!
//! It creates a direct-touch multi-touch device through uinput, the size of the display in
//! pixels, so a contact at (x, y) lands on that pixel. Unlike host-side injection (QEMU 11.1.2's
//! virtio-multitouch passes no contact slot or tracking id from QMP), the contact goes through the
//! same path as a real panel: evdev, libinput, `dap-shell`, the client.
//!
//! Usage: dap-inject tap X Y [HOLD_MS]
//!        dap-inject swipe X1 Y1 X2 Y2 [DURATION_MS]
//!        (--size WxH, default 320x480: the display in pixels)

#[cfg(target_os = "linux")]
mod uinput;

fn usage() -> ! {
    eprintln!("usage: dap-inject [--size WxH] tap X Y [HOLD_MS] | swipe X1 Y1 X2 Y2 [DURATION_MS]");
    std::process::exit(2);
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut size = (320, 480);
    if args.first().map(String::as_str) == Some("--size") {
        let spec = args.get(1).cloned().unwrap_or_else(|| usage());
        let (w, h) = spec.split_once('x').unwrap_or_else(|| usage());
        size = (w.parse().unwrap_or_else(|_| usage()), h.parse().unwrap_or_else(|_| usage()));
        args.drain(..2);
    }
    let n = |i: usize| -> i32 { args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()) };
    let opt = |i: usize, d: i32| -> i32 { args.get(i).map(|v| v.parse().unwrap_or_else(|_| usage())).unwrap_or(d) };
    let points: Vec<(i32, i32)> = match args.first().map(String::as_str) {
        Some("tap") => vec![(n(1), n(2))],
        Some("swipe") => {
            let (x1, y1, x2, y2) = (n(1), n(2), n(3), n(4));
            let steps = 20;
            (0..=steps).map(|i| (x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)).collect()
        }
        _ => usage(),
    };
    let duration = if args[0] == "tap" { opt(3, 80) } else { opt(5, 250) };
    run(size, &points, duration);
}

#[cfg(target_os = "linux")]
fn run(size: (i32, i32), points: &[(i32, i32)], duration_ms: i32) {
    if let Err(err) = uinput::contact(size, points, duration_ms) {
        eprintln!("dap-inject: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_: (i32, i32), _: &[(i32, i32)], _: i32) {
    eprintln!("dap-inject runs on a Blackbird OS instance (Linux uinput)");
    std::process::exit(2);
}
