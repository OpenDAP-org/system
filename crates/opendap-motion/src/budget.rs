//! Frames only while something moves (shell spec §5.1: a static screen costs no frames and no
//! wakeups): the animations register while they run, and the next frame is asked for only when
//! one does; at rest, none.

/// The running animations, by id, each with the time it stops (or none: until told).
#[derive(Debug, Clone, Default)]
pub struct Budget {
    running: Vec<(u32, Option<f32>)>,
}

impl Budget {
    pub fn start(&mut self, id: u32, until: Option<f32>) {
        self.running.retain(|(i, _)| *i != id);
        self.running.push((id, until));
    }

    pub fn stop(&mut self, id: u32) {
        self.running.retain(|(i, _)| *i != id);
    }

    /// Whether a frame is needed at `now`; forgets what has ended.
    pub fn needs_frame(&mut self, now: f32) -> bool {
        self.running.retain(|(_, until)| until.is_none_or(|u| now < u));
        !self.running.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_only_while_something_moves() {
        let mut b = Budget::default();
        assert!(!b.needs_frame(0.0), "at rest: none");
        b.start(1, Some(0.2));
        b.start(2, None);
        assert!(b.needs_frame(0.1));
        b.stop(2);
        assert!(b.needs_frame(0.15));
        assert!(!b.needs_frame(0.25));
    }
}
