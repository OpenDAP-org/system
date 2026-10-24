//! Gesture recognition, shared by the shell's debug UI mode and the UI primitives activity: what
//! a contact was, from where it started and ended and
//! how long it lasted, with the design system's thresholds (§8.3, §8.5).

/// How far a contact may move and still be a tap or a long press, in design pixels.
pub const SLOP_PX: f32 = 10.0;
/// Short against long (T1 controls spec §1.1, Android's long-press timeout).
pub const LONG_MS: u64 = 500;
/// The edge-gesture zones (design system §8.5), in design pixels.
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

/// Where two edge zones meet: a region of its own, so a contact there is never claimed by two
/// edges at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeading,
    TopTrailing,
    BottomLeading,
    BottomTrailing,
}

/// The region a contact starts in (design system §8.5): an edge zone, a corner where two meet, or
/// the content. Every gesture carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Content,
    Edge(Edge),
    Corner(Corner),
}

/// The region of a point (logical pixels) on a `size` screen, `unit` as in [`classify`].
pub fn region(at: (f32, f32), size: (f32, f32), unit: f32) -> Region {
    let e = EDGE_PX * unit;
    let (leading, trailing) = (at.0 < e, at.0 > size.0 - e);
    let (top, bottom) = (at.1 < e, at.1 > size.1 - e);
    match (top, bottom, leading, trailing) {
        (true, _, true, _) => Region::Corner(Corner::TopLeading),
        (true, _, _, true) => Region::Corner(Corner::TopTrailing),
        (_, true, true, _) => Region::Corner(Corner::BottomLeading),
        (_, true, _, true) => Region::Corner(Corner::BottomTrailing),
        (true, ..) => Region::Edge(Edge::Top),
        (_, true, ..) => Region::Edge(Edge::Bottom),
        (_, _, true, _) => Region::Edge(Edge::Leading),
        (_, _, _, true) => Region::Edge(Edge::Trailing),
        _ => Region::Content,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    Tap {
        region: Region,
    },
    LongPress {
        region: Region,
    },
    /// A swipe, with the region it started in (left to right layout).
    Swipe {
        direction: Direction,
        region: Region,
        distance: f32,
        ms: u64,
    },
}

/// A contact from `down` to `up` (logical pixels) lasting `ms`, on a `size` screen whose logical
/// pixel is `1 / unit` design pixels: the thresholds keep their physical size on any hardware.
pub fn classify(down: (f32, f32), up: (f32, f32), ms: u64, size: (f32, f32), unit: f32) -> Gesture {
    let (dx, dy) = (up.0 - down.0, up.1 - down.1);
    let distance = (dx * dx + dy * dy).sqrt();
    let region = region(down, size, unit);
    if distance < SLOP_PX * unit {
        return if ms >= LONG_MS {
            Gesture::LongPress { region }
        } else {
            Gesture::Tap { region }
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
    Gesture::Swipe {
        direction,
        region,
        distance,
        ms,
    }
}

impl Gesture {
    /// The edge of an edge swipe: one that starts in an edge zone (not a corner) and moves away
    /// from that edge; `None` for any other gesture.
    pub fn edge_swipe(&self) -> Option<Edge> {
        let Gesture::Swipe {
            direction,
            region: Region::Edge(edge),
            ..
        } = self
        else {
            return None;
        };
        let away = matches!(
            (edge, direction),
            (Edge::Leading, Direction::Right)
                | (Edge::Trailing, Direction::Left)
                | (Edge::Top, Direction::Down)
                | (Edge::Bottom, Direction::Up)
        );
        away.then_some(*edge)
    }
}

fn region_name(region: Region) -> &'static str {
    match region {
        Region::Content => "",
        Region::Edge(Edge::Leading) => " on the leading edge",
        Region::Edge(Edge::Trailing) => " on the trailing edge",
        Region::Edge(Edge::Top) => " on the top edge",
        Region::Edge(Edge::Bottom) => " on the bottom edge",
        Region::Corner(Corner::TopLeading) => " in the top-leading corner",
        Region::Corner(Corner::TopTrailing) => " in the top-trailing corner",
        Region::Corner(Corner::BottomLeading) => " in the bottom-leading corner",
        Region::Corner(Corner::BottomTrailing) => " in the bottom-trailing corner",
    }
}

impl Gesture {
    /// How the debug UI mode names it, with the action the design system gives it (§8.5).
    pub fn describe(&self) -> String {
        match self {
            Gesture::Tap { region } => format!("tap{}", region_name(*region)),
            Gesture::LongPress { region } => format!("long press{}", region_name(*region)),
            Gesture::Swipe {
                direction,
                region,
                distance,
                ms,
            } => {
                let dir = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                let from = match self.edge_swipe() {
                    Some(Edge::Leading) => " from the leading edge: back".to_string(),
                    Some(Edge::Trailing) => " from the trailing edge: companion screen".to_string(),
                    Some(Edge::Bottom) => " from the bottom: more".to_string(),
                    Some(Edge::Top) => " from the status bar: quick settings".to_string(),
                    // A corner's swipe has no action of its own yet; it is named, never an
                    // edge's.
                    None => match region {
                        Region::Corner(_) => format!(", starting{}", region_name(*region)),
                        Region::Edge(_) => format!(
                            ", along or out of{}",
                            region_name(*region).replacen(" on", "", 1)
                        ),
                        Region::Content => String::new(),
                    },
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
            classify((100.0, 100.0), (104.0, 103.0), 120, SCREEN, 1.0),
            Gesture::Tap {
                region: Region::Content
            }
        );
        assert_eq!(
            classify((100.0, 100.0), (101.0, 100.0), 600, SCREEN, 1.0),
            Gesture::LongPress {
                region: Region::Content
            }
        );
    }

    #[test]
    fn swipes_take_the_major_axis() {
        let g = classify((100.0, 300.0), (160.0, 290.0), 200, SCREEN, 1.0);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Right,
                region: Region::Content,
                ..
            }
        ));
        let g = classify((100.0, 300.0), (110.0, 200.0), 200, SCREEN, 1.0);
        assert!(matches!(
            g,
            Gesture::Swipe {
                direction: Direction::Up,
                ..
            }
        ));
    }

    #[test]
    fn thresholds_follow_the_hardware() {
        // At twice the density, a 12 logical px move is a 6 design px move: still a tap.
        assert!(matches!(
            classify((100.0, 100.0), (112.0, 100.0), 100, SCREEN, 2.0),
            Gesture::Tap { .. }
        ));
        // And the edge zone is 40 logical px wide.
        let g = classify((30.0, 240.0), (200.0, 240.0), 180, SCREEN, 2.0);
        assert_eq!(g.edge_swipe(), Some(Edge::Leading));
    }

    #[test]
    fn edge_swipes_start_in_an_edge_and_move_away_from_it() {
        assert_eq!(
            classify((8.0, 240.0), (120.0, 240.0), 180, SCREEN, 1.0).edge_swipe(),
            Some(Edge::Leading)
        );
        assert_eq!(
            classify((315.0, 240.0), (200.0, 240.0), 180, SCREEN, 1.0).edge_swipe(),
            Some(Edge::Trailing)
        );
        assert_eq!(
            classify((160.0, 475.0), (160.0, 360.0), 180, SCREEN, 1.0).edge_swipe(),
            Some(Edge::Bottom)
        );
        assert_eq!(
            classify((160.0, 6.0), (160.0, 120.0), 180, SCREEN, 1.0).edge_swipe(),
            Some(Edge::Top)
        );
        // Starting at an edge but moving along it, or outwards, is not an edge swipe.
        assert_eq!(
            classify((8.0, 240.0), (2.0, 120.0), 180, SCREEN, 1.0).edge_swipe(),
            None
        );
    }

    #[test]
    fn corners_are_regions_of_their_own() {
        assert_eq!(
            region((5.0, 5.0), SCREEN, 1.0),
            Region::Corner(Corner::TopLeading)
        );
        assert_eq!(
            region((315.0, 5.0), SCREEN, 1.0),
            Region::Corner(Corner::TopTrailing)
        );
        assert_eq!(
            region((5.0, 475.0), SCREEN, 1.0),
            Region::Corner(Corner::BottomLeading)
        );
        assert_eq!(
            region((315.0, 475.0), SCREEN, 1.0),
            Region::Corner(Corner::BottomTrailing)
        );
        assert_eq!(
            region((5.0, 240.0), SCREEN, 1.0),
            Region::Edge(Edge::Leading)
        );
        assert_eq!(region((160.0, 240.0), SCREEN, 1.0), Region::Content);
        // A swipe from a corner is never an edge swipe, whichever way it goes.
        let g = classify((6.0, 470.0), (6.0, 300.0), 180, SCREEN, 1.0);
        assert!(matches!(
            g,
            Gesture::Swipe {
                region: Region::Corner(Corner::BottomLeading),
                ..
            }
        ));
        assert_eq!(g.edge_swipe(), None);
        let g = classify((6.0, 470.0), (150.0, 470.0), 180, SCREEN, 1.0);
        assert_eq!(g.edge_swipe(), None);
        // Taps and long presses carry their corner too.
        assert_eq!(
            classify((314.0, 474.0), (314.0, 474.0), 700, SCREEN, 1.0),
            Gesture::LongPress {
                region: Region::Corner(Corner::BottomTrailing)
            }
        );
    }
}
