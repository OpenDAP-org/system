//! Hit testing (input events §6.5, ui-model §9): which areas a contact reaches, from the top
//! down, by each area's behaviour (Flutter's HitTestBehavior, SwiftUI's allowsHitTesting, the
//! web's pointer-events). One order for drawing and for hits (never iOS's zPosition trap).

use crate::gesture::FramePath;

/// What an area does with a contact inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Takes it, and nothing below gets it (a button, a scrim).
    Opaque,
    /// Takes it, and lets the areas below have it too (a debug overlay that observes).
    Translucent,
    /// Only where a child of it would: its own empty parts let it through (a group, a layer
    /// with `hit: through`).
    DeferToChildren,
    /// Never takes it (a hidden or decorative element: nothing invisible takes touches).
    None,
}

/// An area on screen: its path (frames from the surface) and its behaviour.
#[derive(Debug, Clone)]
pub struct Area {
    pub name: String,
    pub path: FramePath,
    pub behaviour: Behaviour,
}

/// The areas a point of the surface reaches, `areas` listed from the top down (the drawing
/// order reversed): the first opaque one ends the list.
pub fn hits(areas: &[Area], surface: (f32, f32)) -> Vec<&str> {
    let mut out = Vec::new();
    for a in areas {
        let Some(frame) = a.path.0.last() else { continue };
        if !frame.contains(a.path.to_inner(surface)) {
            continue;
        }
        match a.behaviour {
            Behaviour::None | Behaviour::DeferToChildren => {}
            Behaviour::Translucent => out.push(a.name.as_str()),
            Behaviour::Opaque => {
                out.push(a.name.as_str());
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::Frame;

    fn area(name: &str, origin: (f32, f32), size: (f32, f32), b: Behaviour) -> Area {
        Area { name: name.into(), path: FramePath(vec![Frame::new(size, 1.0).at(origin)]), behaviour: b }
    }

    #[test]
    fn the_top_opaque_area_takes_it_and_translucent_ones_watch() {
        let areas = [
            area("debug", (0.0, 0.0), (320.0, 480.0), Behaviour::Translucent),
            area("hidden-panel", (0.0, 0.0), (320.0, 200.0), Behaviour::None),
            area("group", (0.0, 100.0), (320.0, 200.0), Behaviour::DeferToChildren),
            area("button", (16.0, 120.0), (100.0, 48.0), Behaviour::Opaque),
            area("page", (0.0, 0.0), (320.0, 480.0), Behaviour::Opaque),
        ];
        assert_eq!(hits(&areas, (50.0, 140.0)), vec!["debug", "button"]);
        assert_eq!(hits(&areas, (250.0, 140.0)), vec!["debug", "page"], "a group's empty part lets it through");
        assert_eq!(hits(&areas, (50.0, 50.0)), vec!["debug", "page"], "a hidden panel takes nothing");
    }
}
