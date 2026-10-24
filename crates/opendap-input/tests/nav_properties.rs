//! Navigation's invariants over generated screens and inputs (ADR 0014, level L1): trees of every
//! role nested at random, and long sequences of keys, touches, layers opening, the focus hiding
//! and the tree changing. Whatever the combination, these hold after every step; a failure is
//! shrunk to the smallest screen and sequence that breaks one.

use opendap_input::nav::{Effect, Key, NavNode, Navigator, Role};
use proptest::prelude::*;

/// A node of a generated screen, before names: its role and its children.
#[derive(Debug, Clone)]
enum Shape {
    Leaf { primary: bool, disabled: bool, leaving: bool },
    Adjustable { disabled: bool },
    Label,
    Stop { disabled: bool, children: Vec<Shape> },
    Group { disabled: bool, children: Vec<Shape> },
}

fn shape() -> impl Strategy<Value = Shape> {
    let leaf = prop_oneof![
        6 => (any::<bool>(), prop::bool::weighted(0.15), prop::bool::weighted(0.1)).prop_map(|(p, d, l)| Shape::Leaf { primary: p && !d, disabled: d, leaving: l }),
        2 => prop::bool::weighted(0.15).prop_map(|d| Shape::Adjustable { disabled: d }),
        1 => Just(Shape::Label),
    ];
    leaf.prop_recursive(3, 24, 5, |inner| {
        prop_oneof![
            (prop::bool::weighted(0.1), prop::collection::vec(inner.clone(), 1..5)).prop_map(|(d, c)| Shape::Stop { disabled: d, children: c }),
            (prop::bool::weighted(0.1), prop::collection::vec(inner, 1..5)).prop_map(|(d, c)| Shape::Group { disabled: d, children: c }),
        ]
    })
}

fn build(s: &Shape, n: &mut usize) -> NavNode {
    *n += 1;
    let name = format!("n{n}");
    match s {
        Shape::Leaf { primary, disabled, leaving } => {
            let mut x = NavNode::leaf(&name);
            if *primary {
                x = x.primary();
            }
            if *disabled {
                x = x.disabled();
            }
            if *leaving {
                x = x.leaving();
            }
            x
        }
        Shape::Adjustable { disabled } => {
            let x = NavNode::adjustable(&name);
            if *disabled { x.disabled() } else { x }
        }
        Shape::Label => NavNode::label(&name),
        Shape::Stop { disabled, children } => {
            let x = NavNode::stop(&name, children.iter().map(|c| build(c, n)).collect());
            if *disabled { x.disabled() } else { x }
        }
        Shape::Group { disabled, children } => {
            let x = NavNode::group(&name, children.iter().map(|c| build(c, n)).collect());
            if *disabled { x.disabled() } else { x }
        }
    }
}

/// A screen: its content, plus a modal and a popover it can open.
fn screen(content: &[Shape]) -> NavNode {
    let mut n = 0;
    let mut children: Vec<NavNode> = content.iter().map(|c| build(c, &mut n)).collect();
    children.push(NavNode::layer("modal", true, vec![NavNode::leaf("ok").primary(), NavNode::leaf("cancel")]));
    children.push(NavNode::layer("menu", false, vec![NavNode::leaf("m1"), NavNode::leaf("m2")]));
    NavNode::group("screen", children)
}

#[derive(Debug, Clone)]
enum Op {
    Key(Key),
    Touch(usize),
    ReleaseAll,
    Open(bool),
    Remove(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        12 => prop_oneof![
            Just(Key::Next), Just(Key::Previous), Just(Key::Action), Just(Key::Escape), Just(Key::Forward), Just(Key::Backward)
        ].prop_map(Op::Key),
        2 => (0usize..64).prop_map(Op::Touch),
        1 => Just(Op::ReleaseAll),
        1 => any::<bool>().prop_map(Op::Open),
        1 => (0usize..64).prop_map(Op::Remove),
    ]
}

fn find<'a>(n: &'a NavNode, name: &str) -> Option<&'a NavNode> {
    if n.name == name {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, name))
}

/// The ancestors of `name` (outermost first), not including it.
fn path<'a>(n: &'a NavNode, name: &str, out: &mut Vec<&'a NavNode>) -> bool {
    if n.name == name {
        return true;
    }
    out.push(n);
    for c in &n.children {
        if path(c, name, out) {
            return true;
        }
    }
    out.pop();
    false
}

fn all_names(n: &NavNode, out: &mut Vec<String>) {
    out.push(n.name.clone());
    for c in &n.children {
        all_names(c, out);
    }
}

/// A node the focus may be on: a leaf, an adjustable or a stop, not leaving, and neither it nor
/// anything around it disabled.
fn focusable(root: &NavNode, name: &str) -> bool {
    let Some(n) = find(root, name) else { return false };
    let mut anc = Vec::new();
    path(root, name, &mut anc);
    matches!(n.role, Role::Leaf | Role::Adjustable | Role::Stop) && !n.leaving && n.enabled && anc.iter().all(|a| a.enabled && !a.leaving)
}

/// Removes the node called `name` (and everything in it), except the screen and its layers.
fn remove(n: &mut NavNode, name: &str) {
    n.children.retain(|c| c.name != name || matches!(c.role, Role::Layer { .. }));
    for c in &mut n.children {
        remove(c, name);
    }
}

fn check(nav: &Navigator) -> Result<(), TestCaseError> {
    let root = &nav.root;
    if let Some(f) = nav.focus() {
        prop_assert!(find(root, f).is_some(), "the focus is on a node that exists: {f}");
        prop_assert!(focusable(root, f), "the focus is on something focusable: {f}");
    }
    if let Some(e) = nav.engaged() {
        let node = find(root, e);
        prop_assert!(node.is_some_and(|n| matches!(n.role, Role::Stop | Role::Adjustable)), "only stops and adjustables engage: {e}");
        let f = nav.focus().unwrap_or_default();
        if node.is_some_and(|n| n.role == Role::Stop) {
            let mut anc = Vec::new();
            path(root, f, &mut anc);
            prop_assert!(anc.iter().any(|a| a.name == e), "the focus is inside the engaged stop {e}: {f}");
        } else {
            prop_assert_eq!(f, e, "an engaged adjustable has the focus");
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 600, ..ProptestConfig::default() })]

    #[test]
    fn whatever_the_screen_and_the_input_the_focus_stays_sound(content in prop::collection::vec(shape(), 1..7), ops in prop::collection::vec(op(), 1..60)) {
        let mut nav = Navigator::new(screen(&content));
        check(&nav)?;
        for o in ops {
            let mut names = Vec::new();
            all_names(&nav.root, &mut names);
            match o {
                Op::Key(k) => {
                    let before = nav.focus().map(str::to_string);
                    let e = nav.key(k);
                    match &e {
                        // Activating never moves the focus.
                        Effect::Activated(n) => prop_assert_eq!(Some(n.as_str()), before.as_deref()),
                        // Refusing changes nothing.
                        Effect::Refused(_) => prop_assert_eq!(nav.focus(), before.as_deref()),
                        // Past an end the focus stays.
                        Effect::AtEnd => prop_assert_eq!(nav.focus(), before.as_deref()),
                        _ => {}
                    }
                }
                Op::Touch(i) => {
                    let name = &names[i % names.len()];
                    // Only what is drawn can be touched: nothing inside a layer that is closed.
                    let mut anc = Vec::new();
                    path(&nav.root, name, &mut anc);
                    let in_layer = anc.iter().any(|a| matches!(a.role, Role::Layer { .. }));
                    if focusable(&nav.root, name) && !in_layer {
                        nav.touched(name);
                        prop_assert_eq!(nav.focus(), Some(name.as_str()), "a touch gives the focus");
                    }
                }
                Op::ReleaseAll => {
                    nav.release_all();
                    prop_assert_eq!(nav.depth(), 0, "the focus hiding releases every level");
                }
                Op::Open(modal) => {
                    nav.open(if modal { "modal" } else { "menu" });
                    prop_assert_eq!(nav.depth(), 0, "a layer opening releases the levels below");
                }
                Op::Remove(i) => {
                    let name = names[i % names.len()].clone();
                    if name != "screen" {
                        let mut root = nav.root.clone();
                        remove(&mut root, &name);
                        nav.replace(root);
                    }
                }
            }
            check(&nav)?;
        }
    }

    #[test]
    fn next_then_previous_comes_back(content in prop::collection::vec(shape(), 1..7), steps in 0usize..12) {
        let mut nav = Navigator::new(screen(&content));
        for _ in 0..steps {
            nav.key(Key::Next);
        }
        let at = nav.focus().map(str::to_string);
        let depth = nav.depth();
        if let Effect::Moved(_) = nav.key(Key::Next) {
            nav.key(Key::Previous);
            prop_assert_eq!(nav.focus().map(str::to_string), at, "one step there and back");
            prop_assert_eq!(nav.depth(), depth);
        }
    }

    #[test]
    fn a_modal_never_lets_the_focus_out(content in prop::collection::vec(shape(), 1..5), keys in prop::collection::vec(prop_oneof![Just(Key::Next), Just(Key::Previous), Just(Key::Action)], 1..30)) {
        let mut nav = Navigator::new(screen(&content));
        nav.open("modal");
        for k in keys {
            let e = nav.key(k);
            prop_assert_ne!(e, Effect::AtEnd, "a modal wraps");
            prop_assert!(matches!(nav.focus(), Some("ok") | Some("cancel")), "inside the modal: {:?}", nav.focus());
        }
    }

    #[test]
    fn every_enabled_stop_of_the_screen_is_reached(content in prop::collection::vec(shape(), 1..7)) {
        let root = screen(&content);
        let mut nav = Navigator::new(root.clone());
        // From the first stop, Next reaches every stop of the top level once, then the end.
        for _ in 0..200 {
            if nav.key(Key::Previous) == Effect::AtEnd { break; }
        }
        let mut seen = vec![nav.focus().map(str::to_string)];
        for _ in 0..200 {
            match nav.key(Key::Next) {
                Effect::Moved(n) => seen.push(Some(n)),
                _ => break,
            }
        }
        fn level(n: &NavNode, out: &mut Vec<String>) {
            for c in &n.children {
                if c.leaving || !c.enabled { continue; }
                match c.role {
                    Role::Transparent => level(c, out),
                    Role::Leaf | Role::Adjustable | Role::Stop => out.push(c.name.clone()),
                    _ => {}
                }
            }
        }
        let mut expected = Vec::new();
        level(&root, &mut expected);
        let seen: Vec<String> = seen.into_iter().flatten().collect();
        prop_assert_eq!(seen, expected, "each stop once, in reading order");
    }
}
