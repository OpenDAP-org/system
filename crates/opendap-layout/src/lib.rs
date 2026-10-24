//! Layout primitives for content of any size (ui-model §6, §7; T1-046: no limits, T1-047: no
//! slowdown after hours): what to build, keep and draw of a list or a grid of a million items,
//! in bounded memory and time. Pure data and functions: a component (a `Scroll`, a `CoverGrid`)
//! or an activity's runtime applies them to Slint's models.
//!
//! - [`extents`]: item sizes along the scroll axis, fixed or measured (prefix sums in a Fenwick
//!   tree: O(log n) offset and index lookups, with an estimate for items not measured yet).
//! - [`window`]: the range to build for a viewport, with overscan and prefetch ahead by velocity.
//! - [`anchor`]: scroll anchoring: what the user looks at stays put when items above change.
//! - [`recycle`]: stable slots by key, so moving the window reuses views instead of rebuilding.
//! - [`grid`]: auto-fit columns and rows over a list (a cover grid), windowed by row.
//! - [`sticky`]: where a section's header sits while its section scrolls.

pub mod anchor;
pub mod extents;
pub mod grid;
pub mod recycle;
pub mod sticky;
pub mod window;
