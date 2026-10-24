//! Editing text (input events §3.8): the text, the selection and an input method's composition,
//! changed by operations that every source shares (a key, the on-screen keyboard, an input
//! method's commit, a paste, a screen reader's action), each checked first by the field's filter
//! (`beforeinput`, cancelable) and recorded for undo.
//!
//! Positions are byte offsets that always sit on grapheme boundaries: the caret never splits a
//! character as the reader sees it (an emoji with its modifiers, a letter with its accents). The
//! boundaries come from a [`Boundaries`] port: Unicode segmentation (ICU4X) on the device, plain
//! characters where nothing more is needed.

/// Where text may be split: grapheme clusters for the caret, words for word moves and deletes.
pub trait Boundaries {
    /// Every grapheme boundary of `s`, from 0 to `s.len()`.
    fn graphemes(&self, s: &str) -> Vec<usize>;
    /// The start and end of every word-like segment of `s`.
    fn words(&self, s: &str) -> Vec<(usize, usize)>;
}

/// Unicode text segmentation (UAX #29) through ICU4X, the i18n stack's (text input research §7.1).
/// Words in scripts written without spaces (Chinese, Japanese, Thai…) need ICU4X's dictionary or
/// LSTM models, which this does not carry yet: there, a word move goes by script runs.
#[cfg(feature = "icu")]
#[derive(Debug, Clone, Copy, Default)]
pub struct Unicode;

#[cfg(feature = "icu")]
impl Boundaries for Unicode {
    fn graphemes(&self, s: &str) -> Vec<usize> {
        icu_segmenter::GraphemeClusterSegmenter::new().segment_str(s).collect()
    }
    fn words(&self, s: &str) -> Vec<(usize, usize)> {
        let seg = icu_segmenter::WordSegmenter::new_for_non_complex_scripts(Default::default());
        let mut it = seg.segment_str(s);
        let mut out = Vec::new();
        let mut prev = 0;
        while let Some(b) = it.next() {
            if b > prev && it.is_word_like() {
                out.push((prev, b));
            }
            prev = b;
        }
        out
    }
}

/// Characters as graphemes, words as runs of alphanumerics: enough for codes, numbers and tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chars;

impl Boundaries for Chars {
    fn graphemes(&self, s: &str) -> Vec<usize> {
        s.char_indices().map(|(i, _)| i).chain(std::iter::once(s.len())).collect()
    }
    fn words(&self, s: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = None;
        for (i, c) in s.char_indices() {
            match (c.is_alphanumeric(), start) {
                (true, None) => start = Some(i),
                (false, Some(st)) => {
                    out.push((st, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(st) = start {
            out.push((st, s.len()));
        }
        out
    }
}

/// The selection: `anchor` where it started, `focus` where the caret is. Collapsed, it is the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub focus: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Selection { anchor: at, focus: at }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.focus)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.focus)
    }
    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }
}

/// How far a move or a delete goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Grapheme,
    Word,
    /// To the start or the end of the text (a single-line field's line).
    All,
}

/// One change to the text, whatever produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Typed or committed text, replacing the selection.
    Insert(String),
    /// Pasted text: the same as typed, one undo step, filtered as a whole.
    Paste(String),
    /// Delete backward (Backspace) or forward (Delete) by a unit, or the selection.
    Delete { forward: bool, unit: Unit },
    /// Move the caret, or extend the selection (Shift).
    Move { forward: bool, unit: Unit, extend: bool },
    Select(Selection),
    SelectAll,
    /// An input method's composition, shown at the caret and not yet part of the text
    /// (text-input-v3 `preedit_string`); empty ends it.
    Preedit { text: String, cursor: usize },
    /// An input method's commit (text-input-v3 `commit_string`): ends the composition, inserts.
    Commit(String),
    /// An input method deletes around the caret (text-input-v3 `delete_surrounding_text`), bytes.
    DeleteSurrounding { before: usize, after: usize },
    Undo,
    Redo,
}

/// What an edit would make of the text, for the field's filter to accept or refuse
/// (`beforeinput`, cancelable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed<'a> {
    pub text: &'a str,
    /// What the edit inserts (empty for deletes and moves).
    pub inserted: &'a str,
}

/// What came of an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The text changed (an `input` event follows).
    Changed,
    /// Only the selection or the composition changed.
    Moved,
    /// The filter refused it: nothing changed.
    Refused,
    /// Nothing to do (a delete at the start, an undo with no history).
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    selection: Selection,
}

/// The longest undo history: bounded memory over a long session (T1-047).
pub const UNDO_DEPTH: usize = 100;

/// A field's text being edited.
#[derive(Debug, Clone, Default)]
pub struct Editor {
    text: String,
    selection: Selection,
    preedit: Option<(String, usize)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Typing coalesces into one undo step until a space, a move or another kind of edit.
    typing: bool,
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Editor { text: text.into(), selection: Selection::caret(text.len()), ..Default::default() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// The composition, and its cursor (bytes into it), while an input method composes.
    pub fn preedit(&self) -> Option<(&str, usize)> {
        self.preedit.as_ref().map(|(t, c)| (t.as_str(), *c))
    }

    /// What is shown: the text with the composition at the caret.
    pub fn display(&self) -> String {
        match &self.preedit {
            Some((p, _)) => {
                let at = self.selection.start();
                format!("{}{}{}", &self.text[..at], p, &self.text[self.selection.end()..])
            }
            None => self.text.clone(),
        }
    }

    /// Replaces the text from outside (the bound value changed, input events §3.8): no undo step,
    /// the caret at the end, the composition dropped.
    pub fn set(&mut self, text: &str) {
        if self.text != text {
            self.text = text.into();
            self.selection = Selection::caret(text.len());
            self.preedit = None;
            self.typing = false;
        }
    }

    pub fn apply(&mut self, b: &impl Boundaries, edit: Edit, filter: impl Fn(&Proposed) -> bool) -> Outcome {
        match edit {
            Edit::Insert(s) => {
                let coalesce = self.typing && !s.contains(char::is_whitespace) && self.selection.is_caret();
                self.replace(self.selection.start(), self.selection.end(), &s, coalesce, &filter)
            }
            Edit::Paste(s) => self.replace(self.selection.start(), self.selection.end(), &s, false, &filter),
            Edit::Commit(s) => {
                self.preedit = None;
                self.replace(self.selection.start(), self.selection.end(), &s, false, &filter)
            }
            Edit::Delete { forward, unit } => {
                let (a, z) = if !self.selection.is_caret() {
                    (self.selection.start(), self.selection.end())
                } else {
                    let to = self.step(b, self.selection.focus, forward, unit);
                    (to.min(self.selection.focus), to.max(self.selection.focus))
                };
                if a == z {
                    return Outcome::Nothing;
                }
                self.replace(a, z, "", false, &filter)
            }
            Edit::DeleteSurrounding { before, after } => {
                let g = b.graphemes(&self.text);
                let snap = |x: usize| *g.iter().rev().find(|&&y| y <= x).unwrap_or(&0);
                let a = snap(self.selection.start().saturating_sub(before));
                let z = snap((self.selection.end() + after).min(self.text.len()));
                if a == z {
                    return Outcome::Nothing;
                }
                self.replace(a, z, "", false, &filter)
            }
            Edit::Move { forward, unit, extend } => {
                self.typing = false;
                let from = if extend || self.selection.is_caret() {
                    self.selection.focus
                } else if forward {
                    // A selection collapses to its far side.
                    self.selection = Selection::caret(self.selection.end());
                    return Outcome::Moved;
                } else {
                    self.selection = Selection::caret(self.selection.start());
                    return Outcome::Moved;
                };
                let to = self.step(b, from, forward, unit);
                self.selection = if extend { Selection { anchor: self.selection.anchor, focus: to } } else { Selection::caret(to) };
                Outcome::Moved
            }
            Edit::Select(s) => {
                self.typing = false;
                let g = b.graphemes(&self.text);
                let snap = |x: usize| *g.iter().rev().find(|&&y| y <= x.min(self.text.len())).unwrap_or(&0);
                self.selection = Selection { anchor: snap(s.anchor), focus: snap(s.focus) };
                Outcome::Moved
            }
            Edit::SelectAll => {
                self.typing = false;
                self.selection = Selection { anchor: 0, focus: self.text.len() };
                Outcome::Moved
            }
            Edit::Preedit { text, cursor } => {
                // The composition's cursor on a character of it, whatever the input method sent.
                let cursor = (0..=cursor.min(text.len())).rev().find(|&c| text.is_char_boundary(c)).unwrap_or(0);
                self.preedit = if text.is_empty() { None } else { Some((text.clone(), cursor)) };
                Outcome::Moved
            }
            Edit::Undo => self.history(true),
            Edit::Redo => self.history(false),
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { text: self.text.clone(), selection: self.selection }
    }

    fn replace(&mut self, a: usize, z: usize, with: &str, coalesce: bool, filter: &impl Fn(&Proposed) -> bool) -> Outcome {
        let proposed = format!("{}{}{}", &self.text[..a], with, &self.text[z..]);
        if !filter(&Proposed { text: &proposed, inserted: with }) {
            return Outcome::Refused;
        }
        if !coalesce {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.text = proposed;
        self.selection = Selection::caret(a + with.len());
        // Typing one more character keeps the same undo step; anything else starts a new one.
        self.typing = !with.is_empty() && !with.contains(char::is_whitespace);
        Outcome::Changed
    }

    fn history(&mut self, back: bool) -> Outcome {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(s) = from.pop() else { return Outcome::Nothing };
        to.push(Snapshot { text: std::mem::take(&mut self.text), selection: self.selection });
        self.text = s.text;
        self.selection = s.selection;
        self.preedit = None;
        self.typing = false;
        Outcome::Changed
    }

    /// One unit from `at`, forward or back, landing on a boundary.
    fn step(&self, b: &impl Boundaries, at: usize, forward: bool, unit: Unit) -> usize {
        match unit {
            Unit::All => {
                if forward {
                    self.text.len()
                } else {
                    0
                }
            }
            Unit::Grapheme => {
                let g = b.graphemes(&self.text);
                if forward {
                    *g.iter().find(|&&x| x > at).unwrap_or(&self.text.len())
                } else {
                    *g.iter().rev().find(|&&x| x < at).unwrap_or(&0)
                }
            }
            // To the end of this or the next word going forward, to its start going back (every
            // platform's Ctrl/Option+arrow).
            Unit::Word => {
                let w = b.words(&self.text);
                if forward {
                    w.iter().map(|(_, e)| *e).find(|&e| e > at).unwrap_or(self.text.len())
                } else {
                    w.iter().rev().map(|(s, _)| *s).find(|&s| s < at).unwrap_or(0)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any(_: &Proposed) -> bool {
        true
    }

    fn typed(e: &mut Editor, b: &impl Boundaries, s: &str) {
        for c in s.chars() {
            e.apply(b, Edit::Insert(c.to_string()), any);
        }
    }

    #[test]
    fn typing_moving_and_deleting_by_grapheme_never_splits_a_character() {
        let b = Unicode;
        // "é" as e + combining acute, a family emoji (ZWJ sequence), a flag.
        let s = "ae\u{301}👨‍👩‍👧🇲🇽";
        let mut e = Editor::new(s);
        e.apply(&b, Edit::Delete { forward: false, unit: Unit::Grapheme }, any);
        assert_eq!(e.text(), "ae\u{301}👨‍👩‍👧", "the flag goes whole");
        e.apply(&b, Edit::Delete { forward: false, unit: Unit::Grapheme }, any);
        assert_eq!(e.text(), "ae\u{301}", "the family goes whole");
        e.apply(&b, Edit::Move { forward: false, unit: Unit::Grapheme, extend: false }, any);
        assert_eq!(e.selection().focus, 1, "before é, not inside it");
    }

    #[test]
    fn words_move_and_delete() {
        let b = Unicode;
        let mut e = Editor::new("Kind of Blue");
        e.apply(&b, Edit::Delete { forward: false, unit: Unit::Word }, any);
        assert_eq!(e.text(), "Kind of ");
        e.apply(&b, Edit::Move { forward: false, unit: Unit::Word, extend: false }, any);
        assert_eq!(e.selection().focus, 5);
        e.apply(&b, Edit::Move { forward: true, unit: Unit::Word, extend: true }, any);
        assert_eq!((e.selection().start(), e.selection().end()), (5, 7), "extends over 'of'");
        e.apply(&b, Edit::Insert("&".into()), any);
        assert_eq!(e.text(), "Kind & ", "typing replaces the selection");
    }

    #[test]
    fn the_filter_refuses_before_anything_changes() {
        let b = Chars;
        let mut e = Editor::new("12");
        let digits = |p: &Proposed| p.inserted.chars().all(|c| c.is_ascii_digit());
        assert_eq!(e.apply(&b, Edit::Insert("a".into()), digits), Outcome::Refused);
        assert_eq!(e.text(), "12");
        assert_eq!(e.apply(&b, Edit::Paste("34".into()), digits), Outcome::Changed);
        let max3 = |p: &Proposed| p.text.chars().count() <= 3;
        assert_eq!(e.apply(&b, Edit::Insert("5".into()), max3), Outcome::Refused, "a maximum length");
    }

    #[test]
    fn undo_takes_back_a_typed_word_at_a_time() {
        let b = Unicode;
        let mut e = Editor::new("");
        typed(&mut e, &b, "Blue Train");
        e.apply(&b, Edit::Undo, any);
        assert_eq!(e.text(), "Blue ", "the word typed after the space");
        e.apply(&b, Edit::Undo, any);
        e.apply(&b, Edit::Undo, any);
        assert_eq!(e.text(), "");
        e.apply(&b, Edit::Redo, any);
        assert_eq!(e.text(), "Blue");
        // History is bounded.
        let mut e = Editor::new("");
        for i in 0..300 {
            e.apply(&b, Edit::Paste(i.to_string()), any);
        }
        assert_eq!(e.undo.len(), UNDO_DEPTH);
    }

    #[test]
    fn an_input_method_composes_then_commits() {
        let b = Unicode;
        let mut e = Editor::new("Ki");
        e.apply(&b, Edit::Preedit { text: "ㄴ".into(), cursor: 3 }, any);
        assert_eq!(e.text(), "Ki", "a composition is not text yet");
        assert_eq!(e.display(), "Kiㄴ");
        e.apply(&b, Edit::Preedit { text: "나".into(), cursor: 3 }, any);
        assert_eq!(e.apply(&b, Edit::Commit("나".into()), any), Outcome::Changed);
        assert_eq!((e.text(), e.preedit()), ("Ki나", None));
        e.apply(&b, Edit::DeleteSurrounding { before: 3, after: 0 }, any);
        assert_eq!(e.text(), "Ki");
    }

    #[test]
    fn a_value_set_from_outside_is_no_undo_step() {
        let b = Chars;
        let mut e = Editor::new("a");
        e.set("bound value");
        assert_eq!(e.apply(&b, Edit::Undo, any), Outcome::Nothing);
        assert_eq!(e.selection(), Selection::caret(11));
    }
}
