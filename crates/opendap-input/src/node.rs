//! Who is who in the interface (input events §6.4): every node of the tree an event goes through
//! has a kind and a name, and an address from the activity down to it, so a handler, a test, a
//! tool or the debug UI mode can name it, find it and convert between it and any other node.
//!
//! - A [`NodeId`] is `kind:name`: `activity:music`, `layer:overlay`, `group:queue`,
//!   `element:track-12`. A name is stable across frames and data changes (an item's own key, never
//!   its index), unique among its siblings.
//! - A [`NodePath`] is the chain from the activity to a node: its address
//!   `activity:music/layer:overlay/group:queue/element:track-12`, and its frames, so a point
//!   converts down and up it (W3C's `composedPath`, the accessibility tree's ancestry).
//! - A [`Selector`] matches paths: a segment `kind:name`, `kind:*` any name of a kind, `*` any
//!   node, `**` any depth (`activity:music/**/element:track-*` is every track of the activity).
//! - Shared spaces are singletons with reserved names, not nodes: `@screen`, `@content-area`,
//!   `@surface`, `@viewport`, `@content` ([`crate::space::Space`]).

use std::fmt;

use crate::gesture::{Frame, FramePath};
use crate::space::Space;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// An activity (its whole process's interface).
    Activity,
    /// A window of it, placed by the composer.
    Surface,
    /// A band of a screen: content, raised, overlay, modal (ui-model §9).
    Layer,
    /// Elements that belong together (ui-model §9).
    Group,
    /// A named area that is not an element: a screen's thumb zone, a sheet's handle.
    Region,
    /// A scroll area, a pager, a sheet: something with a viewport and content.
    Container,
    /// One control or one item.
    Element,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Activity => "activity",
            NodeKind::Surface => "surface",
            NodeKind::Layer => "layer",
            NodeKind::Group => "group",
            NodeKind::Region => "region",
            NodeKind::Container => "container",
            NodeKind::Element => "element",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "activity" => NodeKind::Activity,
            "surface" => NodeKind::Surface,
            "layer" => NodeKind::Layer,
            "group" => NodeKind::Group,
            "region" => NodeKind::Region,
            "container" => NodeKind::Container,
            "element" => NodeKind::Element,
            _ => return None,
        })
    }
}

/// `kind:name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub kind: NodeKind,
    pub name: String,
}

impl NodeId {
    pub fn new(kind: NodeKind, name: impl Into<String>) -> Self {
        NodeId { kind, name: name.into() }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.as_str(), self.name)
    }
}

/// A node with its frame in its parent.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub frame: Frame,
    /// A stable identifier for tests and tools, never shown, apart from the accessible label
    /// (which is localised and may change): XCUITest's accessibilityIdentifier, Compose's testTag.
    pub test_id: Option<String>,
}

impl Node {
    pub fn new(id: NodeId, frame: Frame) -> Self {
        Node { id, frame, test_id: None }
    }
}

/// The chain from the activity to a node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodePath(pub Vec<Node>);

impl NodePath {
    /// The address, `kind:name/kind:name/…`.
    pub fn address(&self) -> String {
        self.0.iter().map(|n| n.id.to_string()).collect::<Vec<_>>().join("/")
    }

    /// The frames from the outermost node in, to convert points along the path.
    pub fn frames(&self) -> FramePath {
        FramePath(self.0.iter().map(|n| n.frame).collect())
    }

    /// The node itself.
    pub fn target(&self) -> Option<&Node> {
        self.0.last()
    }

    /// The path up to the nearest ancestor (or the node itself) of `kind`, and with `name` when
    /// given: the group an element belongs to, the container it scrolls in, its layer.
    pub fn up_to(&self, kind: NodeKind, name: Option<&str>) -> Option<NodePath> {
        let i = self
            .0
            .iter()
            .rposition(|n| n.id.kind == kind && name.is_none_or(|w| n.id.name == w))?;
        Some(NodePath(self.0[..=i].to_vec()))
    }

    /// The longest path both share: their common ancestor.
    pub fn common(&self, other: &NodePath) -> NodePath {
        NodePath(
            self.0
                .iter()
                .zip(&other.0)
                .take_while(|(a, b)| a.id == b.id)
                .map(|(a, _)| a.clone())
                .collect(),
        )
    }

    /// A point in this node, in another node of the same tree (through their common ancestor).
    pub fn convert(&self, local: (f32, f32), to: &NodePath) -> (f32, f32) {
        self.frames().convert(local, &to.frames())
    }

    /// Whether this path is `other` or inside it.
    pub fn within(&self, other: &NodePath) -> bool {
        other.0.len() <= self.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| a.id == b.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `kind:name`, with `*` at the end of the name as a prefix wildcard (`element:track-*`).
    Node { kind: Option<NodeKind>, name: String },
    /// `**`: any number of nodes, none included.
    Deep,
}

/// Matches node paths: `activity:music/**/element:track-*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    segments: Vec<Segment>,
}

impl Selector {
    pub fn parse(text: &str) -> Option<Self> {
        let mut segments = Vec::new();
        for part in text.split('/') {
            segments.push(match part {
                "**" => Segment::Deep,
                "*" => Segment::Node { kind: None, name: "*".into() },
                _ => {
                    let (kind, name) = part.split_once(':')?;
                    let kind = if kind == "*" { None } else { Some(NodeKind::parse(kind)?) };
                    Segment::Node { kind, name: name.into() }
                }
            });
        }
        Some(Selector { segments })
    }

    pub fn matches(&self, path: &NodePath) -> bool {
        fn name_ok(pattern: &str, name: &str) -> bool {
            match pattern.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => pattern == name,
            }
        }
        fn go(seg: &[Segment], ids: &[&NodeId]) -> bool {
            match (seg.first(), ids.first()) {
                (None, None) => true,
                (Some(Segment::Deep), _) => {
                    go(&seg[1..], ids) || (!ids.is_empty() && go(seg, &ids[1..]))
                }
                (Some(Segment::Node { kind, name }), Some(id)) => {
                    kind.is_none_or(|k| k == id.kind) && name_ok(name, &id.name) && go(&seg[1..], &ids[1..])
                }
                _ => false,
            }
        }
        let ids: Vec<&NodeId> = path.0.iter().map(|n| &n.id).collect();
        go(&self.segments, &ids)
    }
}

/// A shared space by its reserved name (`@screen`, `@viewport`…).
pub fn space_named(name: &str) -> Option<Space> {
    Some(match name {
        "@screen" => Space::Screen,
        "@content-area" => Space::ContentArea,
        "@surface" => Space::Surface,
        "@viewport" => Space::Viewport,
        "@content" => Space::Content,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: NodeKind, name: &str, origin: (f32, f32), size: (f32, f32)) -> Node {
        Node::new(NodeId::new(kind, name), Frame::new(size, 1.0).at(origin))
    }

    fn track_12() -> NodePath {
        NodePath(vec![
            node(NodeKind::Activity, "music", (0.0, 0.0), (320.0, 452.0)),
            node(NodeKind::Layer, "content", (0.0, 0.0), (320.0, 452.0)),
            node(NodeKind::Container, "queue", (0.0, 48.0), (320.0, 340.0)),
            node(NodeKind::Element, "track-12", (0.0, 120.0), (320.0, 56.0)),
        ])
    }

    #[test]
    fn addresses_and_ancestors() {
        let p = track_12();
        assert_eq!(p.address(), "activity:music/layer:content/container:queue/element:track-12");
        let queue = p.up_to(NodeKind::Container, None).unwrap();
        assert_eq!(queue.address(), "activity:music/layer:content/container:queue");
        assert!(p.within(&queue));
        assert!(!queue.within(&p));
        // A point in the track, in its container: 120 px down it.
        assert_eq!(p.convert((10.0, 5.0), &queue), (10.0, 125.0));
    }

    #[test]
    fn selectors_match_by_kind_name_and_depth() {
        let p = track_12();
        assert!(Selector::parse("activity:music/**/element:track-*").unwrap().matches(&p));
        assert!(Selector::parse("**/container:queue/element:*").unwrap().matches(&p));
        assert!(Selector::parse("*/*/*/*").unwrap().matches(&p));
        assert!(!Selector::parse("**/group:*/element:*").unwrap().matches(&p));
        assert!(!Selector::parse("activity:music/element:track-12").unwrap().matches(&p));
        assert_eq!(Selector::parse("widget:x"), None);
    }

    #[test]
    fn the_common_ancestor_of_two_nodes() {
        let a = track_12();
        let mut b = track_12();
        b.0[3] = node(NodeKind::Element, "track-13", (0.0, 176.0), (320.0, 56.0));
        assert_eq!(a.common(&b).address(), "activity:music/layer:content/container:queue");
        // Track 12's top left, in track 13: 56 px above it.
        assert_eq!(a.convert((0.0, 0.0), &b), (0.0, -56.0));
        assert_eq!(space_named("@viewport"), Some(Space::Viewport));
    }
}
