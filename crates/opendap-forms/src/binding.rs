//! Two-way binding without loops (input events §3.8): a field's text and the model's value stay in
//! step in both directions, and neither echoes back to the other.
//!
//! The field keeps what the person typed (the draft) apart from the value it means. Typing updates
//! the value when the draft is understood, and never rewrites the draft: the caret does not jump
//! and "1." stays "1." while a decimal is on its way. A value from the model replaces the draft only
//! when it means something else than the draft already does, so the model echoing back the value
//! the field just gave it changes nothing (the loop every two-way binding has to break: SwiftUI's
//! and Compose's state hoisting, React's controlled inputs). On commit (leaving the field,
//! submitting) the draft takes the value's own form.

use crate::kind::{Issue, Kind, Value};

/// When typing reaches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Update {
    /// At every keystroke whose text is understood (a search, a live preview).
    #[default]
    OnInput,
    /// Only on commit: leaving the field, Enter, submitting (a value that is costly to apply).
    OnCommit,
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub kind: Kind,
    pub update: Update,
    value: Value,
    draft: String,
    /// What the draft fails on, if it cannot be understood.
    malformed: bool,
    /// Counts every value change, for whoever observes it.
    version: u64,
}

impl Binding {
    pub fn new(kind: Kind, value: Value) -> Self {
        let draft = kind.format(&value);
        Binding { kind, update: Update::OnInput, value, draft, malformed: false, version: 0 }
    }

    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn draft(&self) -> &str {
        &self.draft
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    /// The draft is not understood as this kind (shown once the field is left, form.rs).
    pub fn malformed(&self) -> Option<Issue> {
        self.malformed.then_some(Issue::Malformed)
    }

    /// The person typed: the draft is kept as typed. Returns the new value when it reaches the
    /// model now (`OnInput`, understood, and different).
    pub fn edited(&mut self, text: &str) -> Option<Value> {
        self.draft = text.to_string();
        match self.kind.parse(text) {
            Ok(v) => {
                self.malformed = false;
                if self.update == Update::OnInput { self.set_value(v) } else { None }
            }
            Err(_) => {
                self.malformed = true;
                None
            }
        }
    }

    /// The model's value changed (from anywhere, this field's own update included). Returns the
    /// text to show when the draft must change: never for the value the draft already means.
    pub fn changed(&mut self, value: Value) -> Option<String> {
        let same = self.kind.parse(&self.draft).is_ok_and(|v| v == value);
        if value != self.value {
            self.value = value;
            self.version += 1;
        }
        if same {
            return None;
        }
        self.malformed = false;
        self.draft = self.kind.format(&self.value);
        Some(self.draft.clone())
    }

    /// Leaving the field, Enter, submitting: an `OnCommit` value reaches the model, and the draft
    /// takes the value's own form ("1.50" becomes "1.5"). Returns (the new value if it changed, the
    /// text to show if it changed).
    pub fn commit(&mut self) -> (Option<Value>, Option<String>) {
        let Ok(v) = self.kind.parse(&self.draft) else {
            return (None, None);
        };
        let reached = self.set_value(v);
        let shown = self.kind.format(&self.value);
        let text = (shown != self.draft).then(|| {
            self.draft = shown.clone();
            shown
        });
        (reached, text)
    }

    /// A step up or down (a locked field under the buttons): the value and the draft both move.
    pub fn nudge(&mut self, up: bool) -> Option<Value> {
        let v = self.kind.nudge(&self.value, up)?;
        self.draft = self.kind.format(&v);
        self.malformed = false;
        self.set_value(v)
    }

    fn set_value(&mut self, v: Value) -> Option<Value> {
        if v == self.value {
            return None;
        }
        self.value = v.clone();
        self.version += 1;
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::NumberStyle;

    fn number() -> Kind {
        Kind::Number { min: None, max: None, step: None, decimals: 2, style: NumberStyle::default() }
    }

    #[test]
    fn typing_never_rewrites_what_is_typed() {
        let mut b = Binding::new(number(), Value::Number(1.0));
        assert_eq!(b.edited("1."), None, "1. means 1: no change, no echo");
        assert_eq!(b.draft(), "1.", "the dot stays while the decimal is on its way");
        assert_eq!(b.edited("1.5"), Some(Value::Number(1.5)));
        // The model echoes the value back: nothing changes in the field.
        assert_eq!(b.changed(Value::Number(1.5)), None);
        assert_eq!(b.draft(), "1.5");
        assert_eq!(b.version(), 1);
    }

    #[test]
    fn a_value_from_elsewhere_replaces_the_draft_only_when_it_means_something_else() {
        let mut b = Binding::new(number(), Value::Number(2.0));
        b.edited("2.0");
        assert_eq!(b.changed(Value::Number(2.0)), None, "2.0 already means 2");
        assert_eq!(b.changed(Value::Number(3.25)), Some("3.25".into()));
        assert_eq!(b.draft(), "3.25");
    }

    #[test]
    fn malformed_text_does_not_reach_the_model() {
        let mut b = Binding::new(Kind::Date, Value::Empty);
        assert_eq!(b.edited("2026-02-3"), Some(Value::Date { year: 2026, month: 2, day: 3 }));
        assert_eq!(b.edited("2026-02-"), None);
        assert_eq!(b.malformed(), Some(Issue::Malformed));
        assert_eq!(b.value(), &Value::Date { year: 2026, month: 2, day: 3 }, "the last good value stays");
    }

    #[test]
    fn commit_normalises_and_on_commit_waits_for_it() {
        let mut b = Binding::new(number(), Value::Number(0.0));
        b.update = Update::OnCommit;
        assert_eq!(b.edited("1.50"), None, "nothing reaches the model while typing");
        assert_eq!(b.commit(), (Some(Value::Number(1.5)), Some("1.5".into())));
        assert_eq!(b.commit(), (None, None), "a second commit is nothing");
    }

    #[test]
    fn a_nudge_moves_the_value_and_the_text() {
        let mut b = Binding::new(Kind::Gain { min: -12.0, max: 12.0, step: 0.5 }, Value::Number(0.0));
        assert_eq!(b.nudge(true), Some(Value::Number(0.5)));
        assert_eq!(b.draft(), "+0.5");
    }
}
