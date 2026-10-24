//! Data on demand (ui-model §7.2): what to load for what the window needs, when, and what to let
//! go, for libraries of any size (T1-046) in bounded memory (T1-047). Planners only: they say
//! what to fetch, cancel and evict; the caller runs the fetches however it runs work (a thread,
//! an async runtime, the bus), so the plans are plain data and test with a virtual clock.
//!
//! - [`pages`]: items in pages (or cursor chunks): placeholders for what is not loaded, the
//!   visible pages first, prefetch in the scroll's direction, cancel what went far, evict past
//!   a budget.
//! - [`requests`]: one request per key in flight, and generations, so a stale answer never
//!   overwrites a newer one.
//! - [`cache`]: least recently used, within a byte budget.
//! - [`gate`]: what loads during a fling (the cheap) and what waits until the scroll slows (the
//!   costly: covers, waveforms).
//! - [`stream`]: results that arrive in parts (a search), appended without moving what is shown.

pub mod cache;
pub mod gate;
pub mod pages;
pub mod requests;
pub mod stream;
