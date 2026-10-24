//! The thresholds of input events §7, in design pixels and milliseconds: defaults, adjustable
//! (SYS-066), lengthened and never shortened by accessibility settings.

pub use crate::gesture::{EDGE_PX, FLING_PX_S, LONG_MS, SLOP_PX, STATUS_BAR_PX};

/// Held for deliberate system actions (T1 controls spec §1.1; proposed).
pub const HOLD_MS: u64 = 2000;
/// A held control's repeat: after the delay, the first step, then each interval this fraction of
/// the last, down to the fastest; it stops where it is released (shell spec §2, maintainers).
pub const REPEAT_DELAY_MS: u64 = 400;
pub const REPEAT_FIRST_MS: f64 = 250.0;
pub const REPEAT_FACTOR: f64 = 0.8;
pub const REPEAT_FASTEST_MS: f64 = 50.0;
/// A double tap, only where an element opts in (§3.2).
pub const DOUBLE_TAP_MS: u64 = 300;
/// An attached keyboard's repeat (X11's and GNOME's defaults).
pub const KEY_REPEAT_DELAY_MS: u64 = 600;
pub const KEY_REPEAT_EVERY_MS: u64 = 40;
