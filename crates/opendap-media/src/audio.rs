//! Audio's side of loading (the audio engine runs it; these say when): a playback buffer kept
//! between two watermarks, so reads come in few large bursts (the storage sleeps between them:
//! battery, T1-009) and never run dry; and the next track opened and buffered ahead, so the
//! change is gapless, sample-exact (T1-008: audio first).

/// The buffer's watermarks, in seconds of audio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Watermarks {
    /// Below this, read (and above `low`, until `high`).
    pub low: f32,
    pub high: f32,
}

impl Default for Watermarks {
    fn default() -> Self {
        // Proposed, to measure on the T1: refill at 10 s, up to 60 s of audio buffered.
        Watermarks { low: 10.0, high: 60.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// Read until the high watermark.
    Fill,
    /// Enough: let the storage sleep.
    Idle,
}

impl Watermarks {
    /// What to do with `buffered` seconds, `reading` or not (it keeps reading up to high once it
    /// starts: few long bursts, not many short ones).
    pub fn decide(&self, buffered: f32, reading: bool) -> Read {
        if buffered < self.low || (reading && buffered < self.high) { Read::Fill } else { Read::Idle }
    }
}

/// Whether to open and buffer the next track now: `lead` seconds before the current one ends
/// (enough to open a file on a slow card and decode its start).
pub fn prepare_next(position: f32, length: f32, lead: f32) -> bool {
    length - position <= lead
}

/// Frames left in the current track: where the next one starts, sample-exact (gapless: the next
/// track's first frame follows the current one's last, no silence, no overlap).
pub fn frames_left(position_frames: u64, length_frames: u64) -> u64 {
    length_frames.saturating_sub(position_frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_in_bursts_between_the_watermarks() {
        let w = Watermarks::default();
        assert_eq!(w.decide(5.0, false), Read::Fill);
        assert_eq!(w.decide(30.0, true), Read::Fill, "keeps reading to the top");
        assert_eq!(w.decide(30.0, false), Read::Idle, "then lets the card sleep");
        assert_eq!(w.decide(61.0, true), Read::Idle);
    }

    #[test]
    fn the_next_track_is_ready_before_the_current_one_ends() {
        assert!(!prepare_next(100.0, 240.0, 20.0));
        assert!(prepare_next(221.0, 240.0, 20.0));
        assert_eq!(frames_left(10_584_000, 10_584_123), 123);
    }
}
