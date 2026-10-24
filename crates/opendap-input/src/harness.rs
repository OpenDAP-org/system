//! Testing input without hardware (input events §6.5): a virtual clock, scripted contacts, and
//! recordings that replay through the same recognizers as live input, so a device's bug report
//! becomes a test (Flutter's WidgetTester, Compose's performTouchInput, a fake clock).

use crate::arena::{Arena, Input, Outcome};

/// Time that moves only when a test says so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    pub ms: u64,
}

impl Clock {
    pub fn advance(&mut self, ms: u64) -> u64 {
        self.ms += ms;
        self.ms
    }
}

/// A recorded or scripted contact: its inputs with their times.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Script {
    pub inputs: Vec<Input>,
}

impl Script {
    /// A tap at `at`, starting at `ms`, lasting `hold` ms.
    pub fn tap(at: (f32, f32), ms: u64, hold: u64) -> Self {
        Script { inputs: vec![Input::Down { at, ms }, Input::Up { at, ms: ms + hold }] }
    }

    /// A straight drag from `from` to `to`, starting at `ms`, over `duration` ms in `steps`.
    pub fn drag(from: (f32, f32), to: (f32, f32), ms: u64, duration: u64, steps: u32) -> Self {
        let mut inputs = vec![Input::Down { at: from, ms }];
        for i in 1..=steps {
            let f = i as f32 / steps as f32;
            inputs.push(Input::Move {
                at: (from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f),
                ms: ms + duration * u64::from(i) / u64::from(steps),
            });
        }
        inputs.push(Input::Up { at: to, ms: ms + duration });
        Script { inputs }
    }

    pub fn then(mut self, other: Script) -> Self {
        self.inputs.extend(other.inputs);
        self
    }

    /// One line per input: `down x y ms`, `move x y ms`, `up x y ms`, `tick ms`, `cancel`: a
    /// recording a person can read and a bug report can carry.
    pub fn to_text(&self) -> String {
        self.inputs
            .iter()
            .map(|i| match *i {
                Input::Down { at, ms } => format!("down {} {} {ms}", at.0, at.1),
                Input::Move { at, ms } => format!("move {} {} {ms}", at.0, at.1),
                Input::Up { at, ms } => format!("up {} {} {ms}", at.0, at.1),
                Input::Tick { ms } => format!("tick {ms}"),
                Input::Cancel => "cancel".to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn from_text(text: &str) -> Option<Self> {
        let mut inputs = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let w: Vec<&str> = line.split_whitespace().collect();
            let f = |i: usize| w.get(i).and_then(|v| v.parse::<f32>().ok());
            let t = |i: usize| w.get(i).and_then(|v| v.parse::<u64>().ok());
            inputs.push(match *w.first()? {
                "down" => Input::Down { at: (f(1)?, f(2)?), ms: t(3)? },
                "move" => Input::Move { at: (f(1)?, f(2)?), ms: t(3)? },
                "up" => Input::Up { at: (f(1)?, f(2)?), ms: t(3)? },
                "tick" => Input::Tick { ms: t(1)? },
                "cancel" => Input::Cancel,
                _ => return None,
            });
        }
        Some(Script { inputs })
    }
}

/// Plays a script into an arena, ticking it at its deadlines between inputs as live time would,
/// and once more after the last input until nothing is pending: what won, in order.
pub fn play(arena: &mut Arena, script: &Script) -> Vec<String> {
    let mut won = Vec::new();
    let take = |o: Outcome, won: &mut Vec<String>| won.extend(o.won);
    for input in &script.inputs {
        let at = match *input {
            Input::Down { ms, .. } | Input::Move { ms, .. } | Input::Up { ms, .. } | Input::Tick { ms } => Some(ms),
            Input::Cancel => None,
        };
        if let Some(at) = at {
            while let Some(d) = arena.deadline().filter(|d| *d <= at) {
                take(arena.feed(Input::Tick { ms: d }), &mut won);
                if arena.deadline() == Some(d) {
                    break;
                }
            }
        }
        take(arena.feed(*input), &mut won);
    }
    let mut guard = 0;
    while let Some(d) = arena.deadline() {
        take(arena.feed(Input::Tick { ms: d }), &mut won);
        guard += 1;
        if guard > 16 {
            break;
        }
    }
    won
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{DoubleTap, LongPress, Priority, Tap};

    #[test]
    fn scripts_replay_through_the_arena_with_virtual_time() {
        let mut clock = Clock::default();
        let start = clock.advance(1000);
        let mut a = Arena::new().add(Tap::new(1.0), Priority::Normal).add(LongPress::new(1.0), Priority::Normal);
        assert_eq!(play(&mut a, &Script::tap((10.0, 10.0), start, 80)), vec!["tap"]);
        let mut a = Arena::new().add(Tap::new(1.0), Priority::Normal).add(LongPress::new(1.0), Priority::Normal);
        assert_eq!(play(&mut a, &Script::tap((10.0, 10.0), start, 900)), vec!["long press"]);
        let mut a = Arena::new().add(DoubleTap::new(1.0), Priority::Normal).add(Tap::new(1.0), Priority::Normal);
        let two = Script::tap((10.0, 10.0), start, 60).then(Script::tap((10.0, 10.0), start + 180, 60));
        assert_eq!(play(&mut a, &two), vec!["double tap"]);
    }

    #[test]
    fn recordings_read_back_as_they_were() {
        let s = Script::drag((0.0, 0.0), (0.0, 90.0), 0, 90, 3);
        let text = s.to_text();
        assert!(text.starts_with("down 0 0 0\nmove 0 30 30"));
        assert_eq!(Script::from_text(&text), Some(s));
        assert_eq!(Script::from_text("wobble 1"), None);
    }
}
