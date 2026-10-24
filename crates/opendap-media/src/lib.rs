//! Media primitives (ui-model §7.3): how images, icons, video, music, metadata and plots get
//! from storage to the screen or the output at the right size, in the right order, within a
//! budget, and on whatever the hardware offers. Planners and pure math: the components (a
//! `Cover`, a waveform, a player) and the services run the decoding and the I/O.
//!
//! - [`resource`]: a resource's identity and its variants; the variant for a target size.
//! - [`fit`]: contain, cover, fill, none, scale-down: where content lands in its box, and the
//!   size to decode it at (the box's physical pixels, never more than the source).
//! - [`decode`]: a decode queue by priority (visible, prefetch, background), bounded in
//!   parallel, deduplicated, cancellable.
//! - [`progressive`]: a placeholder chain (dominant colour, thumbnail, full), stopped early
//!   while the content flies past.
//! - [`tiers`]: where a resource is: decoded in memory, encoded on disk, or to fetch.
//! - [`audio`]: buffer watermarks, and the next track prepared ahead for gapless playback.
//! - [`blur`]: a frosted backdrop: shrunk, blurred once (three box passes ≈ Gaussian).
//! - [`video`]: which frame to show at each refresh, and what to drop or repeat.
//! - [`plot`]: millions of samples to a few hundred pixels (min/max per column), and a ring
//!   buffer for live meters.
//! - [`render`]: the render path the hardware allows: GPU when present, software otherwise
//!   (SYS-040).

pub mod audio;
pub mod blur;
pub mod decode;
pub mod fit;
pub mod plot;
pub mod progressive;
pub mod render;
pub mod resource;
pub mod tiers;
pub mod video;
