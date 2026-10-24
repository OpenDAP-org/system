//! The input domain of Blackbird OS (system input events spec): what an input means, apart from
//! the hardware that produced it, platform-agnostic so the composer, the CLI and OpenDAP Desktop
//! share it.
//!
//! - [`anchor`]: an overlay placed at its anchor, flipped and kept inside (§6.5).
//! - [`arena`]: one owner per contact, and the compositions of gestures (§3.2.1, §6.5).
//! - [`gesture`]: pointer streams to gestures, with their regions and frames (§3.2, §6.1).
//! - [`control`]: a control used by its role, to press, long press, hold and repeat (§3.3).
//! - [`semantic`]: roles and context to semantic actions (§3.4).
//! - [`space`]: positions, motion and regions in any frame, shared spaces included (§6.3).
//! - [`node`]: who is who: node identities, addresses, paths and selectors (§6.4).
//! - [`dispatch`]: capture, target, bubble with directives; intents resolved to actions up the path (§4, §5).
//! - [`drag`]: a drag's bounds, snap and drop targets (§3.2).
//! - [`focus`]: focus scopes: default focus, restore, trap, traversal (§6.5).
//! - [`nav`]: navigation without touch: levels, stops, adjustables, layers (§3.7).
//! - [`multi`]: two contacts: pinch and rotation about their centre (§3.2).
//! - [`nested`]: nested scrolling: inner first, the rest to the parent, a parent that takes first.
//! - [`hit`]: which areas a contact reaches, by behaviour.
//! - [`harness`]: a virtual clock, scripted and recorded contacts replayed through the arena.
//! - [`thresholds`]: the defaults of §7.

pub mod anchor;
pub mod arena;
pub mod control;
pub mod dispatch;
pub mod drag;
pub mod focus;
pub mod harness;
pub mod hit;
pub mod multi;
pub mod nav;
pub mod nested;
pub mod gesture;
pub mod node;
pub mod semantic;
pub mod space;
pub mod thresholds;
