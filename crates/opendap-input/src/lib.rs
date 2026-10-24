//! The input domain of Blackbird OS (system input events spec): what an input means, apart from
//! the hardware that produced it, platform-agnostic so the shell, the CLI and OpenDAP Desktop
//! share it.
//!
//! - [`gesture`]: pointer streams to gestures, with their regions and frames (§3.2, §6.1).
//! - [`control`]: a control used by its role, to press, long press, hold and repeat (§3.3).
//! - [`semantic`]: roles and context to semantic actions (§3.4).
//! - [`space`]: positions, motion and regions in any frame, shared spaces included (§6.3).
//! - [`node`]: who is who: node identities, addresses, paths and selectors (§6.4).
//! - [`thresholds`]: the defaults of §7.

pub mod control;
pub mod gesture;
pub mod node;
pub mod semantic;
pub mod space;
pub mod thresholds;
