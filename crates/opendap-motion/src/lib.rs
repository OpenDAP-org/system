//! Motion primitives (ui-model §8): what moves and how, as pure functions of time, so a
//! component animates the same on the T1, on OpenDAP Desktop and in a test with a virtual clock.
//!
//! - [`spring`]: springs for what a hand moves, carrying the gesture's velocity; critically
//!   damped by default (no overshoot), bounce only where asked.
//! - [`easing`]: the motion tokens' curves (cubic-bezier, solved exactly), for what the system
//!   starts.
//! - [`decay`]: a fling's deceleration, and where it stops.
//! - [`animator`]: an interruptible animation: a new target takes over from wherever it is, with
//!   its velocity; reduce motion settles at once.
//! - [`budget`]: frames only while something moves (0 wakeups at rest, shell spec §5.1).

pub mod animator;
pub mod budget;
pub mod decay;
pub mod easing;
pub mod spring;
