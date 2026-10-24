//! Gesture recognition for the testbed: what a contact was, from where it started and ended and
//! how long it lasted, with the design system's thresholds (§8.3, §8.5).

/// How far a contact may move and still be a tap or a long press.
pub const SLOP_PX: f32 = 10.0;
/// Short against long (T1 controls spec §1.1, Android's long-press timeout).
pub const LONG_MS: u64 = 500;
/// The edge-gesture zones (design system §8.5).
pub const EDGE_PX: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Leading,
    Trailing,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    Tap,
    LongPress,
    /// A swipe, with the edge zone it started in, if any (left to right layout).
    Swipe {
        direction: Direction,
        edge: Option<Edge>,
        distance: f32,
        ms: u64,
    },
}

/// A contact from `down` to `up` (logical pixels) lasting `ms`, on a `size` screen.
pub fn classify(down: (f32, f32), up: (f32, f32), ms: u64, size: (f32, f32)) -> Gesture {
    let (dx, dy) = (up.0 - down.0, up.1 - down.1);
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < SLOP_PX {
        return if ms >= LONG_MS {
            Gesture::LongPress
        } else {
            Gesture::Tap
        };
    }
    let direction = if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        }
    } else if dy > 0.0 {
        Direction::Down
    } else {
        Direction::Up
    };
    // An edge swipe starts in the zone and moves away from that edge.
    let edge = match direction {
        Direction::Right if down.0 < EDGE_PX => Some(Edge::Leading),
        Direction::Left if down.0 > size.0 - EDGE_PX => Some(Edge::Trailing),
        Direction::Up if down.1 > size.1 - EDGE_PX => Some(Edge::Bottom),
        Direction::Down if down.1 < EDGE_PX => Some(Edge::Top),
        _ => None,
    };
    Gesture::Swipe {
        direction,
        edge,
        distance,
        ms,
    }
}

impl Gesture {
    /// How the testbed names it, with the action the design system gives it (§8.5).
    pub fn describe(&self) -> String {
        match self {
            Gesture::Tap => "tap".into(),
            Gesture::LongPress => "long press".into(),
            Gesture::Swipe {
                direction,
                edge,
                distance,
                ms,
            } => {
                let dir = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                let from = match edge {
                    Some(Edge::Leading) => " from the leading edge: back",
                    Some(Edge::Trailing) => " from the trailing edge: companion screen",
                    Some(Edge::Bottom) => " from the bottom: more",
                    Some(Edge::Top) => " from the status bar: quick settings",
                    None => "",
                };
                format!("swipe {dir}{from} · {distance:.0} px in {ms} ms")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f32, f32) = (320.0, 480.0);

    #[test]
    fn taps_and_long_presses_stay_within_the_slop() {
        assert_eq!(
            classify((100.0, 100.0), (104.0, 103.0), 120, SCREEN),
            Gesture::Tap
        );
        assert_eq!(
            classify((100.0, 100.0), (101.0, 100.0), 600, SCREEN),
            Gesture::LongPress
        );
    }

    #[test]
    fn swipes_take_the_major_axis() {
        let g = classify((100.0, 300.0), (160.0, 290.0), 200, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Right,
                edge: None,
                ..
            }
        ));
        let g = classify((100.0, 300.0), (110.0, 200.0), 200, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Up,
                edge: None,
                ..
            }
        ));
    }

    #[test]
    fn edge_swipes_start_in_the_20_px_zone_and_move_inwards() {
        let g = classify((8.0, 240.0), (120.0, 240.0), 180, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                edge: Some(Edge::Leading),
                ..
            }
        ));
        let g = classify((315.0, 240.0), (200.0, 240.0), 180, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                edge: Some(Edge::Trailing),
                ..
            }
        ));
        let g = classify((160.0, 475.0), (160.0, 360.0), 180, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                edge: Some(Edge::Bottom),
                ..
            }
        ));
        let g = classify((160.0, 6.0), (160.0, 120.0), 180, SCREEN);
        assert!(matches!(
            g,
            Gesture::Swipe {
                edge: Some(Edge::Top),
                ..
            }
        ));
        // Starting at an edge but moving along it, or outwards, is not an edge swipe.
        let g = classify((8.0, 240.0), (2.0, 120.0), 180, SCREEN);
        assert!(matches!(g, Gesture::Swipe { edge: None, .. }));
    }
}
