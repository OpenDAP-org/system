//! The forms primitives' invariants over generated input (ADR 0014, level L1): any sequence of
//! edits on any text, in any script; every kind's text and value forms; a field and its model
//! talking to each other in any order. A failure is shrunk to the smallest case that breaks one.

use opendap_forms::binding::Binding;
use opendap_forms::kind::{Kind, NumberStyle, Value};
use opendap_forms::text::{Edit, Editor, Outcome, Selection, Unicode, Unit};
use proptest::prelude::*;

/// Text in the scripts a field meets: Latin, accents (precomposed and combining), CJK, Arabic,
/// emoji with joiners and skin tones, spaces.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop_oneof![Just("a"), Just("Z"), Just("é"), Just("e\u{301}"), Just("日本"), Just("مرحبا"), Just("👩‍💻"), Just("👍🏽"), Just(" "), Just("1"), Just(".")], 0..8).prop_map(|v| v.concat())
}

fn unit() -> impl Strategy<Value = Unit> {
    prop_oneof![Just(Unit::Grapheme), Just(Unit::Word), Just(Unit::All)]
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        4 => text().prop_map(Edit::Insert),
        1 => text().prop_map(Edit::Paste),
        3 => (any::<bool>(), unit()).prop_map(|(forward, unit)| Edit::Delete { forward, unit }),
        3 => (any::<bool>(), unit(), any::<bool>()).prop_map(|(forward, unit, extend)| Edit::Move { forward, unit, extend }),
        1 => (0usize..40, 0usize..40).prop_map(|(a, f)| Edit::Select(Selection { anchor: a, focus: f })),
        1 => Just(Edit::SelectAll),
        1 => (text(), 0usize..20).prop_map(|(text, cursor)| Edit::Preedit { text, cursor }),
        1 => text().prop_map(Edit::Commit),
        1 => (0usize..12, 0usize..12).prop_map(|(before, after)| Edit::DeleteSurrounding { before, after }),
        2 => Just(Edit::Undo),
        1 => Just(Edit::Redo),
    ]
}

fn boundary(s: &str, at: usize) -> bool {
    at <= s.len() && s.is_char_boundary(at)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    /// Whatever is done to it, the text stays valid and the selection and the composition's cursor
    /// sit on boundaries inside it; what is refused changes nothing.
    #[test]
    fn the_editor_stays_sound(start in text(), edits in prop::collection::vec(edit(), 1..40), refuse in prop::bool::weighted(0.2)) {
        let mut e = Editor::new(&start);
        for ed in edits {
            let before = (e.text().to_string(), e.selection());
            let out = e.apply(&Unicode, ed.clone(), |_| !refuse);
            let s = e.selection();
            prop_assert!(boundary(e.text(), s.anchor) && boundary(e.text(), s.focus), "selection {s:?} inside {:?} after {ed:?}", e.text());
            if let Some((p, c)) = e.preedit() {
                prop_assert!(boundary(p, c), "composition cursor {c} on a boundary of {p:?}");
            }
            let _ = e.display();
            match out {
                Outcome::Refused | Outcome::Nothing | Outcome::Moved => prop_assert_eq!(e.text(), before.0.as_str(), "{:?} leaves the text", out),
                Outcome::Changed => {}
            }
            if out == Outcome::Refused {
                prop_assert_eq!(e.selection(), before.1, "refused: the selection stays");
            }
        }
    }

    /// Undo walks back to where editing began, and redo forward to where it ended (within the
    /// history's depth: 40 edits are under it).
    #[test]
    fn undo_goes_back_to_the_start_and_redo_returns(start in text(), edits in prop::collection::vec(edit(), 1..40)) {
        let mut e = Editor::new(&start);
        for ed in edits.into_iter().filter(|x| !matches!(x, Edit::Undo | Edit::Redo)) {
            e.apply(&Unicode, ed, |_| true);
        }
        let end = e.text().to_string();
        let mut steps = 0;
        while e.apply(&Unicode, Edit::Undo, |_| true) == Outcome::Changed {
            steps += 1;
            prop_assert!(steps <= 200);
        }
        prop_assert_eq!(e.text(), start.as_str(), "all the way back");
        for _ in 0..steps {
            prop_assert_eq!(e.apply(&Unicode, Edit::Redo, |_| true), Outcome::Changed);
        }
        prop_assert_eq!(e.text(), end.as_str(), "and forward again");
    }

    /// The history is bounded (T1-047): however long the session, at most the depth is kept.
    #[test]
    fn the_history_is_bounded(words in prop::collection::vec("[a-z]{1,3}", 150..220)) {
        let mut e = Editor::new("");
        for w in &words {
            e.apply(&Unicode, Edit::Paste(w.clone()), |_| true);
        }
        let mut steps = 0;
        while e.apply(&Unicode, Edit::Undo, |_| true) == Outcome::Changed {
            steps += 1;
        }
        prop_assert_eq!(steps, opendap_forms::text::UNDO_DEPTH);
    }
}

/// The kinds whose text form is open-ended, each with an alphabet that reaches its edges.
fn kind_and_text() -> impl Strategy<Value = (Kind, String)> {
    let number = (any::<bool>(), 0u8..4).prop_map(|(comma, decimals)| Kind::Number {
        min: None,
        max: None,
        step: None,
        decimals,
        style: if comma { NumberStyle { decimal: ',', group: Some('.') } } else { NumberStyle::default() },
    });
    prop_oneof![
        (number, "[-0-9.,]{0,9}").prop_map(|(k, t)| (k, t)),
        "[0-9-]{0,11}".prop_map(|t| (Kind::Date, t)),
        "[0-9:]{0,6}".prop_map(|t| (Kind::Time, t)),
        "[0-9:]{0,9}".prop_map(|t| (Kind::Duration, t)),
        "#?[0-9a-fA-F]{0,7}".prop_map(|t| (Kind::Color, t)),
        "[0-9]{0,6}".prop_map(|t| (Kind::Pin { len: 4 }, t)),
        "[a-z@.]{0,12}".prop_map(|t| (Kind::Email, t)),
        "[a-z ,]{0,14}".prop_map(|t| (Kind::Tags { max: None }, t)),
        "[-0-9.]{0,7}".prop_map(|t| (Kind::Gain { min: -12.0, max: 12.0, step: 0.5 }, t)),
        "[0-9.k]{0,7}".prop_map(|t| (Kind::Frequency { min: 10.0, max: 24000.0 }, t)),
        "[0-9]{0,4}".prop_map(|t| (Kind::Milliseconds { min: 0.0, max: 10000.0 }, t)),
        "[0-9A-Z-]{0,8}".prop_map(|t| (Kind::Masked { pattern: "AA-####".into() }, t)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3000, ..ProptestConfig::default() })]

    /// A value's own text means that value again: whatever text was understood, its formatted form
    /// parses to the same value, and formatting is stable (the commit's "1.50" → "1.5" happens once).
    #[test]
    fn every_kind_reads_back_what_it_writes((kind, t) in kind_and_text()) {
        if let Ok(v) = kind.parse(&t) {
            let shown = kind.format(&v);
            prop_assert_eq!(kind.parse(&shown), Ok(v.clone()), "{:?}: {:?} → {:?} → {:?}", kind, t, v, shown);
            prop_assert_eq!(kind.format(&kind.parse(&shown).unwrap()), shown);
        }
    }

    /// The loop every two-way binding must break: whatever is typed, the model taking the value and
    /// sending it back never rewrites what the person typed.
    #[test]
    fn the_model_echoing_back_never_rewrites_the_draft((kind, typed) in kind_and_text(), steps in 1usize..6) {
        let mut b = Binding::new(kind.clone(), Value::Empty);
        for i in 0..steps {
            let t: String = typed.chars().take(typed.chars().count() * (i + 1) / steps).collect();
            let before = b.version();
            if let Some(v) = b.edited(&t) {
                prop_assert_eq!(b.version(), before + 1);
                prop_assert_eq!(b.changed(v), None, "the echo of {:?} changes nothing", t);
                prop_assert_eq!(b.version(), before + 1, "and counts once");
            }
            prop_assert_eq!(b.draft(), t.as_str(), "typing is kept as typed");
        }
    }

    /// A value from elsewhere shows: after the model changes, the draft means the model's value.
    #[test]
    fn a_value_from_the_model_shows((kind, a) in kind_and_text(), (_, b) in kind_and_text()) {
        let mut x = Binding::new(kind.clone(), Value::Empty);
        x.edited(&a);
        if let Ok(v) = kind.parse(&b) {
            x.changed(v.clone());
            prop_assert_eq!(x.value(), &v);
            prop_assert_eq!(kind.parse(x.draft()), Ok(v.clone()), "the draft {:?} means {:?}", x.draft(), v);
            prop_assert!(x.malformed().is_none());
        }
    }
}
