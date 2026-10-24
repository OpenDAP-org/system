//! Fields and forms of Blackbird OS (input events §3.8): editing text, the kinds of field and their
//! values, two-way binding without loops, validation and forms; platform-agnostic, so the composer,
//! activities through the SDK, OpenDAP Desktop and the CLI share them.
//!
//! - [`text`]: the text, its selection and an input method's composition, edited by grapheme and
//!   word, filtered before each change, with a bounded undo.
//! - [`kind`]: what a field holds: parse, format, filter, the keyboard it asks for, checks, steps;
//!   the platforms' kinds, a player's audio values, and the activity's own.
//! - [`binding`]: a field's text and the model's value in step, with no echo.
//! - [`form`]: fields together: touched, dirty, rules between fields, checks that answer later,
//!   submit, reset, Enter's "next".

pub mod binding;
pub mod form;
pub mod kind;
pub mod text;
