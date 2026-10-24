//! A form (input events §3.8): fields together, with their state (touched, dirty, issues), the
//! rules between them, checks that answer later, submitting and resetting, and the order Enter's
//! "next" follows. Issues show once a field has been left or a submit was tried, never while the
//! person is still typing it the first time (the platforms' and WCAG's practice: error
//! identification without interrupting).

use crate::binding::Binding;
use crate::kind::{Issue, Kind, Value};

/// What happens to a field, as the dispatch layer names it (W3C's input events, input events §4).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Focus,
    Blur,
    /// Before text goes in: cancelable (a field's filter, text.rs).
    BeforeInput(String),
    /// The text changed.
    Input(String),
    /// A value reached the model.
    Change(Value),
    /// Issues to show.
    Invalid(Vec<Issue>),
    Submit,
    Reset,
}

pub struct Field {
    pub name: String,
    pub binding: Binding,
    pub required: bool,
    initial: Value,
    /// Typed into since it was last reset: only then does leaving it show its issues.
    edited: bool,
    touched: bool,
    /// Issues from a check that answers later (a name already taken), with its generation.
    later: (u64, Vec<Issue>),
}

/// A rule over several fields: the issues it finds, by field name (an end before its start).
pub type Rule = Box<dyn Fn(&Form) -> Vec<(String, Issue)>>;

#[derive(Default)]
pub struct Form {
    pub fields: Vec<Field>,
    rules: Vec<Rule>,
    /// A change to the first field resets the others to their initial value (a country's region).
    resets: Vec<(String, Vec<String>)>,
    submitted: bool,
}

impl Form {
    pub fn new() -> Self {
        Form::default()
    }

    pub fn field(mut self, name: &str, kind: Kind, value: Value, required: bool) -> Self {
        self.fields.push(Field {
            name: name.into(),
            binding: Binding::new(kind, value.clone()),
            required,
            initial: value,
            edited: false,
            touched: false,
            later: (0, Vec::new()),
        });
        self
    }

    pub fn rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    /// When `field` changes, `dependents` go back to their initial value.
    pub fn resets(mut self, field: &str, dependents: &[&str]) -> Self {
        self.resets.push((field.into(), dependents.iter().map(|s| s.to_string()).collect()));
        self
    }

    pub fn get(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.name == name)
    }

    pub fn value(&self, name: &str) -> Option<&Value> {
        self.get(name).map(|f| f.binding.value())
    }

    /// Text typed into a field: its value when it changed, the dependents reset with it.
    pub fn input(&mut self, name: &str, text: &str) -> Vec<Event> {
        let Some(f) = self.get_mut(name) else { return vec![] };
        f.edited = true;
        let mut out = vec![Event::Input(text.into())];
        if let Some(v) = f.binding.edited(text) {
            out.push(Event::Change(v));
            self.reset_dependents(name);
        }
        out
    }

    /// The field is left: its value committed and, once it has been typed into, its issues shown
    /// from now on. A field the focus only passed through (focus on entry lands on the first) shows
    /// nothing until a submit.
    pub fn blur(&mut self, name: &str) -> Vec<Event> {
        let Some(f) = self.get_mut(name) else { return vec![] };
        f.touched |= f.edited;
        let (v, _) = f.binding.commit();
        let mut out = vec![Event::Blur];
        if let Some(v) = v {
            out.push(Event::Change(v));
            self.reset_dependents(name);
        }
        let issues = self.shown(name);
        if !issues.is_empty() {
            out.push(Event::Invalid(issues));
        }
        out
    }

    fn reset_dependents(&mut self, name: &str) {
        let deps: Vec<String> = self.resets.iter().filter(|(f, _)| f == name).flat_map(|(_, d)| d.clone()).collect();
        for d in deps {
            if let Some(f) = self.get_mut(&d) {
                let init = f.initial.clone();
                f.binding.changed(init);
                f.touched = false;
                f.edited = false;
            }
        }
    }

    /// Every issue of a field now, shown or not.
    pub fn issues(&self, name: &str) -> Vec<Issue> {
        let Some(f) = self.get(name) else { return vec![] };
        let mut out = Vec::new();
        if let Some(m) = f.binding.malformed() {
            out.push(m);
        } else if *f.binding.value() == Value::Empty {
            if f.required {
                out.push(Issue::Required);
            }
        } else {
            out.extend(f.binding.kind.check(f.binding.value()));
        }
        out.extend(f.later.1.iter().cloned());
        for rule in &self.rules {
            out.extend(rule(self).into_iter().filter(|(n, _)| n == name).map(|(_, i)| i));
        }
        out
    }

    /// The issues to show: once the field was left, or a submit was tried.
    pub fn shown(&self, name: &str) -> Vec<Issue> {
        match self.get(name) {
            Some(f) if f.touched || self.submitted => self.issues(name),
            _ => Vec::new(),
        }
    }

    pub fn dirty(&self, name: &str) -> bool {
        self.get(name).is_some_and(|f| *f.binding.value() != f.initial)
    }

    /// A check that answers later starts: its generation, so an older answer is ignored.
    pub fn check_later(&mut self, name: &str) -> u64 {
        let Some(f) = self.get_mut(name) else { return 0 };
        f.later.0 += 1;
        f.later.1.clear();
        f.later.0
    }

    /// Its answer: kept only when it is the latest one asked.
    pub fn answered(&mut self, name: &str, generation: u64, issues: Vec<Issue>) -> bool {
        match self.get_mut(name) {
            Some(f) if f.later.0 == generation => {
                f.later.1 = issues;
                true
            }
            _ => false,
        }
    }

    /// Submitting: every field committed; the values when nothing is wrong, else the first field
    /// with an issue, in order (where the focus goes, its issue announced).
    pub fn submit(&mut self) -> Result<Vec<(String, Value)>, String> {
        self.submitted = true;
        let names: Vec<String> = self.fields.iter().map(|f| f.name.clone()).collect();
        for n in &names {
            if let Some(f) = self.get_mut(n) {
                f.binding.commit();
            }
        }
        if let Some(bad) = names.iter().find(|n| !self.issues(n).is_empty()) {
            return Err(bad.clone());
        }
        Ok(self.fields.iter().map(|f| (f.name.clone(), f.binding.value().clone())).collect())
    }

    pub fn reset(&mut self) {
        self.submitted = false;
        for f in &mut self.fields {
            let init = f.initial.clone();
            f.binding.changed(init);
            f.touched = false;
            f.edited = false;
            f.later = (f.later.0 + 1, Vec::new());
        }
    }

    /// Enter's "next" (text-input-v3's submit action on a field that is not the last): the next
    /// field in order, or `None` on the last, where Enter submits.
    pub fn next(&self, name: &str) -> Option<&str> {
        let i = self.fields.iter().position(|f| f.name == name)?;
        self.fields.get(i + 1).map(|f| f.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Form {
        Form::new()
            .field("name", Kind::Text { min: 1, max: Some(24) }, Value::Empty, true)
            .field("preamp", Kind::Gain { min: -12.0, max: 0.0, step: 0.5 }, Value::Number(0.0), false)
            .field("from", Kind::Time, Value::Empty, false)
            .field("to", Kind::Time, Value::Empty, false)
            .field("output", Kind::Choice { options: vec!["Headphones".into(), "Line".into()] }, Value::Choice(0), true)
            .field("gain", Kind::Choice { options: vec!["Low".into(), "High".into()] }, Value::Choice(0), true)
            .resets("output", &["gain"])
            .rule(Box::new(|f: &Form| match (f.value("from"), f.value("to")) {
                (Some(Value::Time { hour: h1, minute: m1 }), Some(Value::Time { hour: h2, minute: m2 })) if (h2, m2) <= (h1, m1) => {
                    vec![("to".into(), Issue::Custom("ends-before-start".into()))]
                }
                _ => vec![],
            }))
    }

    #[test]
    fn issues_show_once_the_field_is_left_not_while_typing() {
        let mut f = profile();
        assert_eq!(f.input("preamp", "+3"), vec![Event::Input("+3".into()), Event::Change(Value::Number(3.0))]);
        assert!(f.shown("preamp").is_empty(), "typing: nothing shown yet");
        assert_eq!(f.blur("preamp"), vec![Event::Blur, Event::Invalid(vec![Issue::AboveMax(0.0)])]);
    }

    #[test]
    fn passing_through_a_field_shows_nothing() {
        let mut f = profile();
        assert_eq!(f.blur("name"), vec![Event::Blur], "the focus only passed: no Required yet");
        assert!(f.shown("name").is_empty());
        f.input("name", "a");
        f.input("name", "");
        f.blur("name");
        assert_eq!(f.shown("name"), vec![Issue::Required], "typed, then emptied");
    }

    #[test]
    fn a_rule_across_fields_and_a_dependent_reset() {
        let mut f = profile();
        f.input("from", "22:00");
        f.input("to", "06:00");
        f.blur("to");
        assert_eq!(f.shown("to"), vec![Issue::Custom("ends-before-start".into())]);
        f.input("gain", "High");
        assert!(f.dirty("gain"));
        f.input("output", "Line");
        assert_eq!(f.value("gain"), Some(&Value::Choice(0)), "a new output resets the gain");
    }

    #[test]
    fn submit_goes_to_the_first_field_with_an_issue_then_succeeds() {
        let mut f = profile();
        assert_eq!(f.submit(), Err("name".into()), "required and empty");
        assert_eq!(f.shown("name"), vec![Issue::Required], "a submit shows every issue");
        f.input("name", "Late night");
        let values = f.submit().unwrap();
        assert_eq!(values[0], ("name".into(), Value::Text("Late night".into())));
        f.reset();
        assert_eq!(f.value("name"), Some(&Value::Empty));
        assert!(f.shown("name").is_empty());
    }

    #[test]
    fn a_later_answer_is_kept_only_when_it_is_the_latest() {
        let mut f = profile();
        let first = f.check_later("name");
        let second = f.check_later("name");
        assert!(!f.answered("name", first, vec![Issue::Custom("taken".into())]), "stale");
        assert!(f.answered("name", second, vec![Issue::Custom("taken".into())]));
        f.input("name", "x");
        f.blur("name");
        assert_eq!(f.shown("name"), vec![Issue::Custom("taken".into())]);
    }

    #[test]
    fn enter_moves_to_the_next_field() {
        let f = profile();
        assert_eq!(f.next("name"), Some("preamp"));
        assert_eq!(f.next("gain"), None, "the last: Enter submits");
    }
}
