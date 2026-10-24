//! What a field holds (input events §3.8): its kind says how text becomes a value and back, what
//! may be typed, which keyboard the system shows and what makes a value valid. The catalogue
//! covers the kinds of the platforms people know (HTML's input types, Android's input types,
//! text-input-v3's content purposes) and the audio values a player edits; an activity adds its own
//! through [`Custom`].

use std::fmt;
use std::sync::Arc;

/// The keyboard and the input method a field asks for (text-input-v3 `content_purpose`, which the
/// system keyboard reads to show its variant, text input research §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Normal,
    Alpha,
    Digits,
    Number,
    Phone,
    Url,
    Email,
    Name,
    Password,
    Pin,
    Date,
    Time,
    Datetime,
    Terminal,
}

/// Hints to the keyboard and the input method (text-input-v3 `content_hint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hints {
    pub completion: bool,
    pub spellcheck: bool,
    pub auto_capitalization: bool,
    pub sensitive: bool,
    pub multiline: bool,
}

/// A field's value, once its text is understood.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    Text(String),
    Number(f64),
    Bool(bool),
    Date { year: i32, month: u8, day: u8 },
    Time { hour: u8, minute: u8 },
    /// Milliseconds.
    Duration(u64),
    /// One option, by its index.
    Choice(usize),
    Choices(Vec<usize>),
    Tags(Vec<String>),
    /// 0xRRGGBB.
    Color(u32),
}

/// Why a value is not accepted: data, so the message is the interface's, in its language.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    Required,
    /// The text cannot be understood as this kind (a letter in a number, half a date).
    Malformed,
    BelowMin(f64),
    AboveMax(f64),
    /// Not on the step from the minimum.
    OffStep(f64),
    TooShort(usize),
    TooLong(usize),
    /// A custom kind's or a form's own reason, by key.
    Custom(String),
}

/// A number's locale: its decimal and group separators (from ICU4X's decimal data on the device).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberStyle {
    pub decimal: char,
    pub group: Option<char>,
}

impl Default for NumberStyle {
    fn default() -> Self {
        NumberStyle { decimal: '.', group: None }
    }
}

/// A kind the activity defines (SDK): its own parse, format and checks, the same contract as the
/// built-in kinds.
pub trait Custom: Send + Sync {
    fn parse(&self, text: &str) -> Result<Value, Issue>;
    fn format(&self, value: &Value) -> String;
    fn allows(&self, _inserted: &str) -> bool {
        true
    }
    fn purpose(&self) -> Purpose {
        Purpose::Normal
    }
    fn check(&self, _value: &Value) -> Vec<Issue> {
        Vec::new()
    }
}

/// The kinds of field.
#[derive(Clone)]
pub enum Kind {
    Text { min: usize, max: Option<usize> },
    Multiline { max: Option<usize> },
    Password,
    /// A numeric code of `len` digits (a pairing code, a PIN).
    Pin { len: usize },
    Number { min: Option<f64>, max: Option<f64>, step: Option<f64>, decimals: u8, style: NumberStyle },
    Email,
    Url,
    Phone,
    /// ISO 8601 typed (YYYY-MM-DD); shown however the interface's locale writes dates.
    Date,
    /// HH:MM, 24 h.
    Time,
    /// m:ss or h:mm:ss (a track position, a sleep timer).
    Duration,
    Choice { options: Vec<String> },
    MultiChoice { options: Vec<String>, min: usize, max: Option<usize> },
    Toggle,
    Tags { max: Option<usize> },
    Search,
    /// A pattern: `#` a digit, `A` a letter, `*` either; anything else is a literal typed for you.
    Masked { pattern: String },
    /// #RRGGBB.
    Color,
    Rating { max: u8 },
    /// Hz, from 10 to the sample rate's half: edited on a logarithmic scale.
    Frequency { min: f64, max: f64 },
    /// dB, with its step (0.1 for EQ gain, 0.5 for a preamp).
    Gain { min: f64, max: f64, step: f64 },
    /// A filter's Q.
    Q { min: f64, max: f64 },
    /// Milliseconds (a crossfade, a delay).
    Milliseconds { min: f64, max: f64 },
    Custom(Arc<dyn Custom>),
}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Custom(_) => f.write_str("Custom"),
            k => write!(f, "{}", k.name()),
        }
    }
}

fn number(text: &str, style: NumberStyle) -> Result<f64, Issue> {
    let t: String = text
        .trim()
        .chars()
        .filter(|c| Some(*c) != style.group)
        .map(|c| if c == style.decimal { '.' } else { c })
        .collect();
    t.parse::<f64>().map_err(|_| Issue::Malformed)
}

fn range(v: f64, min: Option<f64>, max: Option<f64>) -> Vec<Issue> {
    let mut out = Vec::new();
    if let Some(m) = min
        && v < m
    {
        out.push(Issue::BelowMin(m));
    }
    if let Some(m) = max
        && v > m
    {
        out.push(Issue::AboveMax(m));
    }
    out
}

fn trimmed_number(v: f64, decimals: u8) -> String {
    let s = format!("{v:.prec$}", prec = decimals as usize);
    if decimals > 0 { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Text { .. } => "text",
            Kind::Multiline { .. } => "multiline",
            Kind::Password => "password",
            Kind::Pin { .. } => "pin",
            Kind::Number { .. } => "number",
            Kind::Email => "email",
            Kind::Url => "url",
            Kind::Phone => "phone",
            Kind::Date => "date",
            Kind::Time => "time",
            Kind::Duration => "duration",
            Kind::Choice { .. } => "choice",
            Kind::MultiChoice { .. } => "multi-choice",
            Kind::Toggle => "toggle",
            Kind::Tags { .. } => "tags",
            Kind::Search => "search",
            Kind::Masked { .. } => "masked",
            Kind::Color => "color",
            Kind::Rating { .. } => "rating",
            Kind::Frequency { .. } => "frequency",
            Kind::Gain { .. } => "gain",
            Kind::Q { .. } => "q",
            Kind::Milliseconds { .. } => "milliseconds",
            Kind::Custom(_) => "custom",
        }
    }

    /// The keyboard it asks for.
    pub fn purpose(&self) -> Purpose {
        match self {
            Kind::Text { .. } | Kind::Multiline { .. } | Kind::Search | Kind::Tags { .. } => Purpose::Normal,
            Kind::Password => Purpose::Password,
            Kind::Pin { .. } => Purpose::Pin,
            Kind::Number { .. } | Kind::Gain { .. } | Kind::Q { .. } | Kind::Frequency { .. } | Kind::Milliseconds { .. } => Purpose::Number,
            Kind::Email => Purpose::Email,
            Kind::Url => Purpose::Url,
            Kind::Phone => Purpose::Phone,
            Kind::Date => Purpose::Date,
            Kind::Time => Purpose::Time,
            Kind::Duration | Kind::Masked { .. } | Kind::Rating { .. } => Purpose::Digits,
            Kind::Color => Purpose::Alpha,
            Kind::Choice { .. } | Kind::MultiChoice { .. } | Kind::Toggle => Purpose::Normal,
            Kind::Custom(c) => c.purpose(),
        }
    }

    pub fn hints(&self) -> Hints {
        match self {
            Kind::Text { .. } => Hints { completion: true, spellcheck: true, auto_capitalization: true, ..Default::default() },
            Kind::Multiline { .. } => Hints { completion: true, spellcheck: true, auto_capitalization: true, multiline: true, ..Default::default() },
            Kind::Password | Kind::Pin { .. } => Hints { sensitive: true, ..Default::default() },
            Kind::Search => Hints { completion: true, ..Default::default() },
            _ => Hints::default(),
        }
    }

    /// Whether `inserted` may be typed into this field at all (`beforeinput`): a letter never
    /// reaches a number. A paste is checked as a whole.
    pub fn allows(&self, inserted: &str) -> bool {
        let all = |f: fn(char) -> bool| inserted.chars().all(f);
        match self {
            Kind::Pin { .. } | Kind::Rating { .. } => all(|c| c.is_ascii_digit()),
            Kind::Number { style, .. } => inserted.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '+' || c == style.decimal || Some(c) == style.group),
            Kind::Gain { .. } | Kind::Q { .. } | Kind::Frequency { .. } | Kind::Milliseconds { .. } => all(|c| c.is_ascii_digit() || c == '-' || c == '.' || c == 'k'),
            Kind::Phone => all(|c| c.is_ascii_digit() || "+-() ".contains(c)),
            Kind::Date => all(|c| c.is_ascii_digit() || c == '-'),
            Kind::Time | Kind::Duration => all(|c| c.is_ascii_digit() || c == ':'),
            Kind::Color => all(|c| c.is_ascii_hexdigit() || c == '#'),
            Kind::Text { .. } | Kind::Password | Kind::Email | Kind::Url | Kind::Search | Kind::Tags { .. } => !inserted.contains('\n'),
            Kind::Masked { .. } => all(|c| c.is_alphanumeric()),
            Kind::Custom(c) => c.allows(inserted),
            _ => true,
        }
    }

    /// The text understood as a value; empty text is `Value::Empty` (required or not is the
    /// form's to say).
    pub fn parse(&self, text: &str) -> Result<Value, Issue> {
        let v = self.read(text)?;
        // A value means what the field shows, no more: a number typed finer than the field writes
        // it (0.01 dB on a 0.5 dB gain, 1000.4 Hz) is the number it is shown as, so what is stored
        // and what is seen never differ.
        Ok(match (self, &v) {
            (Kind::Number { .. } | Kind::Gain { .. } | Kind::Q { .. } | Kind::Milliseconds { .. } | Kind::Frequency { .. }, Value::Number(_)) => self.read(&self.format(&v))?,
            _ => v,
        })
    }

    fn read(&self, text: &str) -> Result<Value, Issue> {
        let t = text.trim();
        if t.is_empty() && !matches!(self, Kind::Toggle | Kind::Custom(_)) {
            return Ok(Value::Empty);
        }
        match self {
            Kind::Text { .. } | Kind::Multiline { .. } | Kind::Password | Kind::Search => Ok(Value::Text(text.to_string())),
            Kind::Pin { len } => {
                if t.len() == *len && t.chars().all(|c| c.is_ascii_digit()) { Ok(Value::Text(t.into())) } else { Err(Issue::Malformed) }
            }
            Kind::Number { style, .. } => number(t, *style).map(Value::Number),
            Kind::Gain { .. } | Kind::Q { .. } | Kind::Milliseconds { .. } => number(t, NumberStyle::default()).map(Value::Number),
            // "1k" and "1.2k" are kilohertz, as an equaliser writes them.
            Kind::Frequency { .. } => match t.strip_suffix('k') {
                Some(k) => number(k, NumberStyle::default()).map(|v| Value::Number(v * 1000.0)),
                None => number(t, NumberStyle::default()).map(Value::Number),
            },
            Kind::Email => {
                let ok = t.split_once('@').is_some_and(|(u, d)| !u.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.') && !t.contains(' '));
                if ok { Ok(Value::Text(t.into())) } else { Err(Issue::Malformed) }
            }
            Kind::Url => {
                let ok = t.split_once("://").is_some_and(|(s, rest)| !s.is_empty() && !rest.is_empty()) && !t.contains(' ');
                if ok { Ok(Value::Text(t.into())) } else { Err(Issue::Malformed) }
            }
            Kind::Phone => {
                let digits = t.chars().filter(char::is_ascii_digit).count();
                if (4..=15).contains(&digits) { Ok(Value::Text(t.into())) } else { Err(Issue::Malformed) }
            }
            Kind::Date => {
                let p: Vec<&str> = t.split('-').collect();
                let [y, m, d] = p.as_slice() else { return Err(Issue::Malformed) };
                let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u8>(), d.parse::<u8>()) else { return Err(Issue::Malformed) };
                let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
                let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
                if (1..=12).contains(&m) && d >= 1 && d <= days[m as usize - 1] {
                    Ok(Value::Date { year: y, month: m, day: d })
                } else {
                    Err(Issue::Malformed)
                }
            }
            Kind::Time => {
                let (h, m) = t.split_once(':').ok_or(Issue::Malformed)?;
                let (Ok(h), Ok(m)) = (h.parse::<u8>(), m.parse::<u8>()) else { return Err(Issue::Malformed) };
                if h < 24 && m < 60 { Ok(Value::Time { hour: h, minute: m }) } else { Err(Issue::Malformed) }
            }
            Kind::Duration => {
                let parts: Vec<u64> = t.split(':').map(|p| p.parse::<u64>()).collect::<Result<_, _>>().map_err(|_| Issue::Malformed)?;
                let ms = match parts.as_slice() {
                    [s] => *s * 1000,
                    [m, s] if *s < 60 => (m * 60 + s) * 1000,
                    [h, m, s] if *m < 60 && *s < 60 => ((h * 60 + m) * 60 + s) * 1000,
                    _ => return Err(Issue::Malformed),
                };
                Ok(Value::Duration(ms))
            }
            Kind::Choice { options } => options.iter().position(|o| o.eq_ignore_ascii_case(t)).map(Value::Choice).ok_or(Issue::Malformed),
            Kind::MultiChoice { options, .. } => t
                .split(',')
                .map(|p| options.iter().position(|o| o.eq_ignore_ascii_case(p.trim())).ok_or(Issue::Malformed))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Choices),
            Kind::Toggle => Ok(Value::Bool(matches!(t, "on" | "true" | "1" | "yes"))),
            Kind::Tags { .. } => {
                let mut tags: Vec<String> = Vec::new();
                for tag in t.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    // The same tag twice is one tag.
                    if !tags.iter().any(|x| x.eq_ignore_ascii_case(tag)) {
                        tags.push(tag.to_string());
                    }
                }
                // Separators alone are no tags: empty, as an untouched field is (and required).
                Ok(if tags.is_empty() { Value::Empty } else { Value::Tags(tags) })
            }
            Kind::Masked { pattern } => {
                let slots: Vec<char> = pattern.chars().filter(|c| matches!(c, '#' | 'A' | '*')).collect();
                let raw: Vec<char> = t.chars().filter(|c| c.is_alphanumeric()).collect();
                let fits = raw.len() == slots.len()
                    && raw.iter().zip(&slots).all(|(c, s)| match s {
                        '#' => c.is_ascii_digit(),
                        'A' => c.is_alphabetic(),
                        _ => true,
                    });
                if fits { Ok(Value::Text(raw.into_iter().collect())) } else { Err(Issue::Malformed) }
            }
            Kind::Color => {
                let h = t.trim_start_matches('#');
                if h.len() == 6 { u32::from_str_radix(h, 16).map(Value::Color).map_err(|_| Issue::Malformed) } else { Err(Issue::Malformed) }
            }
            Kind::Rating { max } => t.parse::<u8>().ok().filter(|r| r <= max).map(|r| Value::Number(r as f64)).ok_or(Issue::Malformed),
            Kind::Custom(c) => c.parse(text),
        }
    }

    /// The value written as text, the way this field shows it.
    pub fn format(&self, value: &Value) -> String {
        match (self, value) {
            (_, Value::Empty) => String::new(),
            (Kind::Custom(c), v) => c.format(v),
            (Kind::Number { decimals, style, .. }, Value::Number(n)) => trimmed_number(*n, *decimals).replace('.', &style.decimal.to_string()),
            (Kind::Gain { step, .. }, Value::Number(n)) => {
                let decimals = if *step < 1.0 { (-step.log10()).ceil() as u8 } else { 0 };
                format!("{}{}", if *n > 0.0 { "+" } else { "" }, trimmed_number(*n, decimals))
            }
            (Kind::Frequency { .. }, Value::Number(n)) if *n >= 1000.0 => format!("{}k", trimmed_number(n / 1000.0, 3)),
            (Kind::Frequency { .. } | Kind::Q { .. } | Kind::Milliseconds { .. } | Kind::Rating { .. }, Value::Number(n)) => trimmed_number(*n, 2),
            (Kind::Date, Value::Date { year, month, day }) => format!("{year:04}-{month:02}-{day:02}"),
            (Kind::Time, Value::Time { hour, minute }) => format!("{hour:02}:{minute:02}"),
            (Kind::Duration, Value::Duration(ms)) => {
                let s = ms / 1000;
                if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
            }
            (Kind::Choice { options }, Value::Choice(i)) => options.get(*i).cloned().unwrap_or_default(),
            (Kind::MultiChoice { options, .. }, Value::Choices(is)) => is.iter().filter_map(|i| options.get(*i).cloned()).collect::<Vec<_>>().join(", "),
            (Kind::Toggle, Value::Bool(b)) => if *b { "on" } else { "off" }.into(),
            (Kind::Tags { .. }, Value::Tags(t)) => t.join(", "),
            (Kind::Masked { pattern }, Value::Text(raw)) => {
                let mut chars = raw.chars();
                let mut out = String::new();
                for p in pattern.chars() {
                    match p {
                        '#' | 'A' | '*' => match chars.next() {
                            Some(c) => out.push(c),
                            None => break,
                        },
                        lit => out.push(lit),
                    }
                }
                out
            }
            (Kind::Color, Value::Color(c)) => format!("#{c:06X}"),
            (_, Value::Text(t)) => t.clone(),
            (_, Value::Number(n)) => trimmed_number(*n, 6),
            (_, v) => format!("{v:?}"),
        }
    }

    /// What is wrong with a value of this kind (beyond its form: range, step, length, count).
    pub fn check(&self, value: &Value) -> Vec<Issue> {
        match (self, value) {
            (_, Value::Empty) => Vec::new(),
            (Kind::Text { min, max }, Value::Text(t)) => {
                let n = t.chars().count();
                let mut out = Vec::new();
                if n < *min {
                    out.push(Issue::TooShort(*min));
                }
                if let Some(m) = max
                    && n > *m
                {
                    out.push(Issue::TooLong(*m));
                }
                out
            }
            (Kind::Multiline { max: Some(m) }, Value::Text(t)) if t.chars().count() > *m => vec![Issue::TooLong(*m)],
            (Kind::Number { min, max, step, .. }, Value::Number(v)) => {
                let mut out = range(*v, *min, *max);
                if let Some(s) = step {
                    let from = min.unwrap_or(0.0);
                    let k = (v - from) / s;
                    if (k - k.round()).abs() > 1e-9 {
                        out.push(Issue::OffStep(*s));
                    }
                }
                out
            }
            (Kind::Gain { min, max, step }, Value::Number(v)) => {
                let mut out = range(*v, Some(*min), Some(*max));
                let k = v / step;
                if (k - k.round()).abs() > 1e-6 {
                    out.push(Issue::OffStep(*step));
                }
                out
            }
            (Kind::Frequency { min, max } | Kind::Q { min, max } | Kind::Milliseconds { min, max }, Value::Number(v)) => range(*v, Some(*min), Some(*max)),
            (Kind::MultiChoice { min, max, .. }, Value::Choices(c)) => {
                let mut out = Vec::new();
                if c.len() < *min {
                    out.push(Issue::TooShort(*min));
                }
                if let Some(m) = max
                    && c.len() > *m
                {
                    out.push(Issue::TooLong(*m));
                }
                out
            }
            (Kind::Tags { max: Some(m) }, Value::Tags(t)) if t.len() > *m => vec![Issue::TooLong(*m)],
            (Kind::Custom(c), v) => c.check(v),
            _ => Vec::new(),
        }
    }

    /// A step up or down from `value` (the buttons on a locked field, a stepper, a scroll), within
    /// range: linear for numbers and gains, a twelfth of an octave for a frequency.
    pub fn nudge(&self, value: &Value, up: bool) -> Option<Value> {
        let sign = if up { 1.0 } else { -1.0 };
        match (self, value) {
            (Kind::Number { min, max, step, .. }, Value::Number(v)) => {
                let s = step.unwrap_or(1.0);
                Some(Value::Number((v + sign * s).clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX))))
            }
            (Kind::Gain { min, max, step }, Value::Number(v)) => Some(Value::Number(((v + sign * step) / step).round() * step).map_num(|n| n.clamp(*min, *max))),
            (Kind::Frequency { min, max }, Value::Number(v)) => Some(Value::Number((v * 2f64.powf(sign / 12.0)).clamp(*min, *max))),
            (Kind::Q { min, max }, Value::Number(v)) => Some(Value::Number((v + sign * 0.1).clamp(*min, *max))),
            (Kind::Milliseconds { min, max }, Value::Number(v)) => Some(Value::Number((v + sign * 10.0).clamp(*min, *max))),
            (Kind::Rating { max }, Value::Number(v)) => Some(Value::Number((v + sign).clamp(0.0, *max as f64))),
            (Kind::Choice { options }, Value::Choice(i)) => {
                let n = options.len() as i64;
                Some(Value::Choice((*i as i64 + sign as i64).clamp(0, n - 1) as usize))
            }
            (Kind::Toggle, Value::Bool(_)) => Some(Value::Bool(up)),
            _ => None,
        }
    }
}

trait MapNum {
    fn map_num(self, f: impl Fn(f64) -> f64) -> Self;
}

impl MapNum for Value {
    fn map_num(self, f: impl Fn(f64) -> f64) -> Self {
        match self {
            Value::Number(n) => Value::Number(f(n)),
            v => v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(k: &Kind, text: &str, value: Value, shown: &str) {
        assert_eq!(k.parse(text), Ok(value.clone()), "{} parses {text:?}", k.name());
        assert_eq!(k.format(&value), shown, "{} formats", k.name());
    }

    #[test]
    fn every_kind_reads_and_writes_its_values() {
        let es = NumberStyle { decimal: ',', group: Some('.') };
        round_trip(&Kind::Number { min: None, max: None, step: None, decimals: 2, style: es }, "1.234,5", Value::Number(1234.5), "1234,5");
        round_trip(&Kind::Pin { len: 6 }, "042317", Value::Text("042317".into()), "042317");
        round_trip(&Kind::Email, "dj@opendap.cc", Value::Text("dj@opendap.cc".into()), "dj@opendap.cc");
        round_trip(&Kind::Url, "https://opendap.cc", Value::Text("https://opendap.cc".into()), "https://opendap.cc");
        round_trip(&Kind::Date, "2024-02-29", Value::Date { year: 2024, month: 2, day: 29 }, "2024-02-29");
        round_trip(&Kind::Time, "7:05", Value::Time { hour: 7, minute: 5 }, "07:05");
        round_trip(&Kind::Duration, "1:02:03", Value::Duration(3_723_000), "1:02:03");
        round_trip(&Kind::Duration, "4:33", Value::Duration(273_000), "4:33");
        let opts = vec!["FLAC".to_string(), "ALAC".into(), "WAV".into()];
        round_trip(&Kind::Choice { options: opts.clone() }, "alac", Value::Choice(1), "ALAC");
        round_trip(&Kind::MultiChoice { options: opts, min: 0, max: None }, "flac, wav", Value::Choices(vec![0, 2]), "FLAC, WAV");
        round_trip(&Kind::Tags { max: None }, "jazz, Jazz, live", Value::Tags(vec!["jazz".into(), "live".into()]), "jazz, live");
        round_trip(&Kind::Masked { pattern: "###-###".into() }, "123456", Value::Text("123456".into()), "123-456");
        round_trip(&Kind::Color, "#ff6a2a", Value::Color(0xFF6A2A), "#FF6A2A");
        round_trip(&Kind::Frequency { min: 10.0, max: 24000.0 }, "1.2k", Value::Number(1200.0), "1.2k");
        round_trip(&Kind::Gain { min: -12.0, max: 12.0, step: 0.1 }, "3.5", Value::Number(3.5), "+3.5");
        assert_eq!(Kind::Text { min: 0, max: None }.parse("  "), Ok(Value::Empty), "empty is empty, not an error");
    }

    #[test]
    fn malformed_text_is_an_issue_not_a_value() {
        for (k, t) in [
            (Kind::Email, "dj@opendap"),
            (Kind::Url, "opendap.cc"),
            (Kind::Date, "2023-02-29"),
            (Kind::Time, "24:00"),
            (Kind::Pin { len: 4 }, "123"),
            (Kind::Color, "#12345"),
            (Kind::Masked { pattern: "AA-##".into() }, "1234"),
            (Kind::Phone, "12"),
        ] {
            assert_eq!(k.parse(t), Err(Issue::Malformed), "{} {t:?}", k.name());
        }
    }

    #[test]
    fn checks_range_step_length_and_count() {
        let n = Kind::Number { min: Some(0.0), max: Some(100.0), step: Some(5.0), decimals: 0, style: NumberStyle::default() };
        assert_eq!(n.check(&Value::Number(105.0)), vec![Issue::AboveMax(100.0)]);
        assert_eq!(n.check(&Value::Number(42.0)), vec![Issue::OffStep(5.0)]);
        assert!(n.check(&Value::Number(45.0)).is_empty());
        let t = Kind::Text { min: 2, max: Some(4) };
        assert_eq!(t.check(&Value::Text("abcde".into())), vec![Issue::TooLong(4)]);
        let g = Kind::Gain { min: -12.0, max: 12.0, step: 0.5 };
        assert_eq!(g.check(&Value::Number(1.3)), vec![Issue::OffStep(0.5)]);
    }

    #[test]
    fn what_may_be_typed_is_filtered_by_kind() {
        let n = Kind::Number { min: None, max: None, step: None, decimals: 2, style: NumberStyle { decimal: ',', group: None } };
        assert!(n.allows("12,5") && !n.allows("1.5") && !n.allows("a"));
        assert!(Kind::Pin { len: 4 }.allows("12") && !Kind::Pin { len: 4 }.allows("1a"));
        assert!(!Kind::Text { min: 0, max: None }.allows("a\nb"), "one line");
        assert!(Kind::Multiline { max: None }.allows("a\nb"));
        assert_eq!(Kind::Password.purpose(), Purpose::Password);
        assert!(Kind::Password.hints().sensitive);
        assert_eq!(Kind::Gain { min: -1.0, max: 1.0, step: 0.1 }.purpose(), Purpose::Number);
    }

    #[test]
    fn values_step_within_range() {
        let f = Kind::Frequency { min: 20.0, max: 20000.0 };
        let Some(Value::Number(up)) = f.nudge(&Value::Number(1000.0), true) else { panic!() };
        assert!((up - 1059.46).abs() < 0.01, "a semitone: {up}");
        let g = Kind::Gain { min: -12.0, max: 12.0, step: 0.5 };
        assert_eq!(g.nudge(&Value::Number(12.0), true), Some(Value::Number(12.0)), "clamped");
        let c = Kind::Choice { options: vec!["a".into(), "b".into()] };
        assert_eq!(c.nudge(&Value::Choice(1), true), Some(Value::Choice(1)));
    }

    struct Isrc;
    impl Custom for Isrc {
        fn parse(&self, text: &str) -> Result<Value, Issue> {
            let t = text.trim().to_uppercase().replace('-', "");
            if t.len() == 12 && t[..2].chars().all(|c| c.is_ascii_alphabetic()) { Ok(Value::Text(t)) } else { Err(Issue::Custom("isrc".into())) }
        }
        fn format(&self, value: &Value) -> String {
            match value {
                Value::Text(t) if t.len() == 12 => format!("{}-{}-{}-{}", &t[..2], &t[2..5], &t[5..7], &t[7..]),
                _ => String::new(),
            }
        }
    }

    #[test]
    fn an_activity_adds_its_own_kind() {
        let k = Kind::Custom(Arc::new(Isrc));
        assert_eq!(k.format(&k.parse("usrc17607839").unwrap()), "US-RC1-76-07839");
        assert_eq!(k.parse("12"), Err(Issue::Custom("isrc".into())));
    }
}
