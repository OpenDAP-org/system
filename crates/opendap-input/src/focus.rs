//! Focus scopes (design system §8.4, input events §6.5): which element has the focus when a
//! scope comes to the foreground, where the buttons move it, what a modal keeps inside, and
//! where it goes back to (Flutter's FocusScopeNode, Compose's focusRestorer, SwiftUI's
//! defaultFocus). Plain data over node names: the components or an activity's runtime apply it.

/// One focusable element of a scope, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Focusable {
    pub name: String,
    /// The screen's primary action: the focus lands here on entry, always.
    pub primary: bool,
    /// The header's way back: reached by moving to it, never landed on at entry.
    pub way_back: bool,
}

impl Focusable {
    pub fn new(name: impl Into<String>) -> Self {
        Focusable { name: name.into(), primary: false, way_back: false }
    }
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
    pub fn way_back(mut self) -> Self {
        self.way_back = true;
        self
    }
}

/// How a scope gives the focus back when it comes to the foreground again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Restore {
    /// Always the default (the primary action, else the first element): the maintainers' rule
    /// for screens (design system §8.4).
    #[default]
    Default,
    /// The element that had it last, or the default when that one is gone (a list returned to
    /// mid-way, a modal's opener).
    LastFocused,
}

/// A focus scope: a screen, a group of it, a modal.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub items: Vec<Focusable>,
    pub restore: Restore,
    /// Moving past the last element goes back to the first (and the way round), and the focus
    /// never leaves the scope: a modal, the power menu.
    pub trap: bool,
    last: Option<String>,
}

impl Scope {
    pub fn new(items: Vec<Focusable>) -> Self {
        Scope { items, ..Default::default() }
    }

    /// Where the focus lands when the scope comes to the foreground: its primary action, always,
    /// where it has one; otherwise its first element that is not the way back (§8.4).
    pub fn default_focus(&self) -> Option<&str> {
        self.items
            .iter()
            .find(|i| i.primary)
            .or_else(|| self.items.iter().find(|i| !i.way_back))
            .or_else(|| self.items.first())
            .map(|i| i.name.as_str())
    }

    /// Entering (or returning to) the scope: the element to focus.
    pub fn enter(&self) -> Option<&str> {
        if self.restore == Restore::LastFocused
            && let Some(last) = &self.last
            && let Some(i) = self.items.iter().find(|i| &i.name == last)
        {
            return Some(i.name.as_str());
        }
        self.default_focus()
    }

    /// The focus moved to `name` (a tap, the buttons): remembered for `Restore::LastFocused`.
    pub fn focused(&mut self, name: &str) {
        self.last = Some(name.to_string());
    }

    /// The next (`step` 1) or previous (`step` -1) element from `from`; `None` when the focus
    /// leaves the scope (not trapped, at an end).
    pub fn step(&self, from: &str, step: i32) -> Option<&str> {
        let n = self.items.len() as i32;
        let i = self.items.iter().position(|it| it.name == from)? as i32 + step;
        let i = if self.trap {
            i.rem_euclid(n)
        } else if (0..n).contains(&i) {
            i
        } else {
            return None;
        };
        Some(self.items[i as usize].name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Scope {
        Scope::new(vec![
            Focusable::new("back").way_back(),
            Focusable::new("title-field"),
            Focusable::new("continue").primary(),
            Focusable::new("cancel"),
        ])
    }

    #[test]
    fn the_primary_action_always_takes_the_focus_on_entry() {
        let mut s = screen();
        assert_eq!(s.enter(), Some("continue"));
        s.focused("cancel");
        assert_eq!(s.enter(), Some("continue"), "screens restore the default, always");
    }

    #[test]
    fn without_a_primary_action_the_first_element_but_never_the_way_back() {
        let s = Scope::new(vec![Focusable::new("back").way_back(), Focusable::new("calibration"), Focusable::new("targets")]);
        assert_eq!(s.enter(), Some("calibration"));
    }

    #[test]
    fn last_focused_restores_or_falls_back_to_the_default() {
        let mut s = screen();
        s.restore = Restore::LastFocused;
        s.focused("cancel");
        assert_eq!(s.enter(), Some("cancel"));
        s.items.retain(|i| i.name != "cancel");
        assert_eq!(s.enter(), Some("continue"), "gone: the default");
    }

    #[test]
    fn a_trapped_scope_wraps_and_an_open_one_lets_the_focus_leave() {
        let mut s = screen();
        assert_eq!(s.step("cancel", 1), None);
        assert_eq!(s.step("back", -1), None);
        s.trap = true;
        assert_eq!(s.step("cancel", 1), Some("back"));
        assert_eq!(s.step("back", -1), Some("cancel"));
        assert_eq!(s.step("title-field", 1), Some("continue"));
    }
}
