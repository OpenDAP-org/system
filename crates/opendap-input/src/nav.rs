//! Navigation without touch (input events §3.7, design system §8.4): one model for where the
//! buttons and a keyboard move the focus, in every container, layer and level.
//!
//! The T1's buttons give one axis: Volume moves, Action takes. One axis cannot walk a long page
//! of nested content item by item, so navigation has **levels**, as a slider already does: the
//! focus moves among the stops of the current level, and Action **engages** a stop to work
//! inside it, one level down; leaving goes back up. Three roles carry it all:
//!
//! - **Leaf**: Action activates it (a button, a line, a toggle).
//! - **Adjustable**: Action engages it, and engaged, Volume changes its value instead of moving
//!   (a slider, an axis of stops, a range's knob); Action again, Escape, a touch elsewhere or the
//!   focus hiding release it.
//! - **Stop**: a composite that is one stop in its level (a horizontal shelf of albums in a
//!   page, a video's controls). Engaged, Volume walks its children; past either end the focus
//!   comes back to the stop itself, released, so the buttons alone always find the way out
//!   (tvOS focus sections, Android TV rows, ARIA composites with one tab stop).
//!
//! Everything else is **transparent**: layout containers (stack, row, wrap, grid, adaptive pair,
//! aspect box, groups) put their children in the level they are in, in reading order (rows
//! mirror right to left); a virtual list is transparent over its items, by key. **Layers** are
//! scopes: a modal traps (Volume wraps inside it, nothing outside is reachable), a popover takes
//! the focus when it opens, and both give it back to their opener when they close.
//!
//! Plain data over node names, like [`crate::focus`]: the components and an activity's runtime
//! apply it; tests run the combinations here.

/// What a node is to navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Leaf,
    Adjustable,
    /// One stop; engaged, its children are the level.
    Stop,
    /// No stop of its own: its children join the level it is in.
    Transparent,
    /// A modal or a popover: a scope of its own while it is open.
    Layer { trap: bool },
    /// Takes no focus and holds none (a label, a sticky header, an image, a toast).
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavNode {
    pub name: String,
    pub role: Role,
    pub children: Vec<NavNode>,
    /// The screen's primary action: the focus lands here on entry (§8.4).
    pub primary: bool,
    /// The header's way back: reached by moving to it, never landed on at entry.
    pub way_back: bool,
    /// Disabled keeps its place and shows its reason, but navigation skips it, and a disabled
    /// container's children with it (maintainers, 2026-10-24).
    pub enabled: bool,
    /// On its way out (an exit animation): no longer reachable, though still drawn.
    pub leaving: bool,
}

impl NavNode {
    fn new(name: &str, role: Role, children: Vec<NavNode>) -> Self {
        NavNode { name: name.into(), role, children, primary: false, way_back: false, enabled: true, leaving: false }
    }
    pub fn leaf(name: &str) -> Self {
        Self::new(name, Role::Leaf, vec![])
    }
    pub fn adjustable(name: &str) -> Self {
        Self::new(name, Role::Adjustable, vec![])
    }
    pub fn stop(name: &str, children: Vec<NavNode>) -> Self {
        Self::new(name, Role::Stop, children)
    }
    pub fn group(name: &str, children: Vec<NavNode>) -> Self {
        Self::new(name, Role::Transparent, children)
    }
    pub fn layer(name: &str, trap: bool, children: Vec<NavNode>) -> Self {
        Self::new(name, Role::Layer { trap }, children)
    }
    pub fn label(name: &str) -> Self {
        Self::new(name, Role::None, vec![])
    }
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
    pub fn way_back(mut self) -> Self {
        self.way_back = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
    pub fn leaving(mut self) -> Self {
        self.leaving = true;
        self
    }

    fn find(&self, name: &str) -> Option<&NavNode> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }

    /// The stops of the level this node opens: its descendants through transparent containers,
    /// in reading order; a stop, an adjustable or a leaf ends the descent; layers, what takes no
    /// focus, what is disabled (and all inside it) and what is leaving are not part of it.
    fn level(&self) -> Vec<&NavNode> {
        let mut out = Vec::new();
        for c in &self.children {
            if c.leaving || !c.enabled {
                continue;
            }
            match c.role {
                Role::Transparent => out.extend(c.level()),
                Role::Leaf | Role::Adjustable | Role::Stop => out.push(c),
                Role::Layer { .. } | Role::None => {}
            }
        }
        out
    }

    /// Every node that could be a stop of this level, enabled or not, in reading order (where a
    /// node disabled under the focus sits).
    fn order(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for c in &self.children {
            match c.role {
                Role::Transparent => out.extend(c.order()),
                Role::Leaf | Role::Adjustable | Role::Stop => out.push(c.name.as_str()),
                Role::Layer { .. } | Role::None => {}
            }
        }
        out
    }

    /// Where the focus lands entering this level: its primary action, always, where it has one;
    /// else its first stop that is not the way back (§8.4).
    fn entry(&self) -> Option<&NavNode> {
        fn primary(n: &NavNode) -> Option<&NavNode> {
            n.level().into_iter().find(|s| s.primary)
        }
        let level = self.level();
        primary(self).or_else(|| level.iter().copied().find(|s| !s.way_back)).or_else(|| level.first().copied())
    }
}

/// What the buttons and keys mean to navigation, after the control's role and the context
/// (`semantic`): Volume is `Next`/`Previous` while moving and `Increase`/`Decrease` engaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Volume −, Tab, ↓.
    Next,
    /// Volume +, Shift+Tab, ↑.
    Previous,
    /// Action, Enter, Space.
    Action,
    /// Escape, back.
    Escape,
    /// A keyboard's → (mirrored right to left by the caller: it is `End`-ward in reading order).
    Forward,
    /// A keyboard's ←.
    Backward,
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The focus moved to this node.
    Moved(String),
    /// This node was engaged: one level down.
    Engaged(String),
    /// This node was released: one level up, the focus on it.
    Released(String),
    Activated(String),
    /// An engaged adjustable's value: +1 more, −1 less.
    Adjusted(String, i32),
    /// The focused node was disabled while it had the focus: nothing happens.
    Refused(String),
    /// Past an end of a level that does not wrap: the focus stays (the next element outside is
    /// the screen's, at the root) and the caller may give a boundary cue.
    AtEnd,
    /// Escape at the top level: back, the screen's way out.
    Back,
    /// A layer closed: the focus went back to its opener.
    Closed { layer: String, to: String },
}

/// The navigation state of one screen: the tree, the focused node, the levels engaged and the
/// layers open (each remembering its opener).
#[derive(Debug, Clone)]
pub struct Navigator {
    pub root: NavNode,
    focus: Option<String>,
    /// Engaged nodes, outermost first: a stop, then perhaps an adjustable inside it.
    engaged: Vec<String>,
    /// Open layers and their openers, innermost last.
    layers: Vec<(String, Option<String>)>,
}

impl Navigator {
    pub fn new(root: NavNode) -> Self {
        let mut n = Navigator { root, focus: None, engaged: vec![], layers: vec![] };
        n.focus = n.root.entry().map(|e| e.name.clone());
        n
    }

    pub fn focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// How deep the focus is: 0 at the top level.
    pub fn depth(&self) -> usize {
        self.engaged.len()
    }

    pub fn engaged(&self) -> Option<&str> {
        self.engaged.last().map(String::as_str)
    }

    /// The node whose level the focus moves in: the innermost engaged stop, else the innermost
    /// open layer, else the screen.
    fn scope(&self) -> &NavNode {
        if let Some(s) = self.engaged.iter().rev().filter_map(|e| self.root.find(e)).find(|n| n.role == Role::Stop) {
            return s;
        }
        self.layers.last().and_then(|(l, _)| self.root.find(l)).unwrap_or(&self.root)
    }

    fn trapped(&self) -> bool {
        self.engaged.iter().all(|e| self.root.find(e).is_none_or(|n| n.role != Role::Stop))
            && self.layers.last().and_then(|(l, _)| self.root.find(l)).is_some_and(|l| l.role == Role::Layer { trap: true })
    }

    pub fn key(&mut self, key: Key) -> Effect {
        let Some(focus) = self.focus.clone() else { return Effect::AtEnd };
        let node = self.root.find(&focus).cloned();
        // An engaged adjustable takes Volume and its own keys.
        if self.engaged.last() == Some(&focus) && node.as_ref().is_some_and(|n| n.role == Role::Adjustable) {
            return match key {
                Key::Previous | Key::Forward => Effect::Adjusted(focus, 1),
                Key::Next | Key::Backward => Effect::Adjusted(focus, -1),
                Key::Action | Key::Escape => self.release(),
            };
        }
        match key {
            Key::Next => self.step(&focus, 1),
            Key::Previous => self.step(&focus, -1),
            // ← → change an adjustable at any time, and walk a stop's children without engaging it
            // first, as a keyboard's arrows do in a composite.
            Key::Forward | Key::Backward => {
                let d = if key == Key::Forward { 1 } else { -1 };
                match node.map(|n| n.role) {
                    Some(Role::Adjustable) => Effect::Adjusted(focus, d),
                    Some(Role::Stop) => {
                        let e = self.engage(&focus);
                        if d < 0 {
                            // From the far end, in reading order.
                            if let Some(last) = self.scope().level().last().map(|l| l.name.clone()) {
                                self.focus = Some(last.clone());
                                return Effect::Moved(last);
                            }
                        }
                        e
                    }
                    // Inside an engaged stop, the arrows walk its children like Volume.
                    _ if self.scope().role == Role::Stop => self.step(&focus, d),
                    _ => Effect::AtEnd,
                }
            }
            Key::Action => match node {
                Some(n) if !n.enabled => Effect::Refused(focus),
                Some(n) if n.role == Role::Leaf => Effect::Activated(focus),
                Some(n) if matches!(n.role, Role::Adjustable | Role::Stop) => self.engage(&focus),
                _ => Effect::AtEnd,
            },
            Key::Escape => {
                if !self.engaged.is_empty() {
                    self.release()
                } else if let Some((layer, opener)) = self.layers.pop() {
                    // Back to the opener, else the screen's entry; on a screen with nothing to
                    // focus, nowhere.
                    // The opener may have gone while the layer was open.
                    let alive = |o: &String| self.root.find(o).is_some_and(|n| n.enabled && !n.leaving);
                    let to = opener.filter(alive).or_else(|| self.root.entry().map(|e| e.name.clone()));
                    self.focus = to.clone();
                    Effect::Closed { layer, to: to.unwrap_or_default() }
                } else {
                    Effect::Back
                }
            }
        }
    }

    fn step(&mut self, from: &str, d: i32) -> Effect {
        let scope = self.scope();
        let level = scope.level();
        let n = level.len() as i32;
        // Where `from` sits: its index in the level, or, when it is not in it (disabled under the
        // focus), the gap between the stops around it, so the buttons go on from its place.
        let at = level.iter().position(|s| s.name == from).map(|i| i as f32).or_else(|| {
            let order = scope.order();
            let k = order.iter().position(|o| *o == from)?;
            let before = order[..k].iter().filter(|o| level.iter().any(|l| l.name == **o)).count();
            Some(before as f32 - 0.5)
        });
        let Some(at) = at else {
            // The focus is not in this level (its node left): enter it again, or, when the level
            // has nowhere to put it, leave it where it is.
            return match scope.entry().map(|e| e.name.clone()) {
                Some(to) => {
                    self.focus = Some(to.clone());
                    Effect::Moved(to)
                }
                None => Effect::AtEnd,
            };
        };
        let j = if d > 0 { (at + 1.0).floor() } else { (at - 1.0).ceil() } as i32;
        if (0..n).contains(&j) {
            let to = level[j as usize].name.clone();
            self.focus = Some(to.clone());
            return Effect::Moved(to);
        }
        if self.trapped() {
            let to = level[j.rem_euclid(n) as usize].name.clone();
            self.focus = Some(to.clone());
            return Effect::Moved(to);
        }
        // Past an end of an engaged stop: back to the stop, released.
        if scope.role == Role::Stop {
            return self.release();
        }
        Effect::AtEnd
    }

    fn engage(&mut self, name: &str) -> Effect {
        let Some(n) = self.root.find(name) else { return Effect::AtEnd };
        if !n.enabled {
            return Effect::Refused(name.into());
        }
        let role = n.role;
        let entry = n.entry().map(|e| e.name.clone());
        // A stop with nothing to walk (everything in it disabled or leaving) cannot be entered.
        if role == Role::Stop && entry.is_none() {
            return Effect::Refused(name.into());
        }
        self.engaged.push(name.into());
        if role == Role::Stop
            && let Some(e) = entry
        {
            self.focus = Some(e);
        }
        Effect::Engaged(name.into())
    }

    fn release(&mut self) -> Effect {
        let Some(e) = self.engaged.pop() else { return Effect::AtEnd };
        self.focus = Some(e.clone());
        Effect::Released(e)
    }

    /// Releases every level: the focus hid, the screen was touched elsewhere, the display went
    /// off, a layer opened over it. The focus stays where it is (on the stop of the outermost
    /// level, when it was inside one).
    pub fn release_all(&mut self) {
        if let Some(first) = self.engaged.first().cloned() {
            let outer_is_stop = self.root.find(&first).is_some_and(|n| n.role == Role::Stop);
            if outer_is_stop {
                self.focus = Some(first);
            }
        }
        self.engaged.clear();
    }

    /// A touch gave `name` the focus (hidden): the levels it is inside stay or become engaged
    /// (the stops around it, so the buttons continue from there), every other one is released.
    pub fn touched(&mut self, name: &str) {
        fn stops_to(n: &NavNode, name: &str, path: &mut Vec<String>) -> bool {
            if n.name == name {
                return true;
            }
            for c in &n.children {
                if c.role == Role::Stop {
                    path.push(c.name.clone());
                }
                if stops_to(c, name, path) {
                    return true;
                }
                if c.role == Role::Stop {
                    path.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        if stops_to(&self.root, name, &mut path) {
            // The node itself is not a level it is inside.
            if path.last().is_some_and(|p| p == name) {
                path.pop();
            }
            self.engaged = path;
            self.focus = Some(name.into());
        }
    }

    /// A layer opens: the levels below it are released, the focus enters it, and its opener is
    /// remembered.
    pub fn open(&mut self, layer: &str) -> Effect {
        // The levels below are released first, so the focus comes back to where the buttons can
        // continue: the stop, when it was inside one.
        self.release_all();
        let opener = self.focus.clone();
        self.layers.push((layer.into(), opener));
        let to = self.root.find(layer).and_then(|l| l.entry()).map(|e| e.name.clone());
        self.focus = to.clone();
        to.map_or(Effect::AtEnd, Effect::Moved)
    }

    /// The tree changed (data arrived, items went away, a branch left): the focus follows its
    /// node by name; when that node is gone, the level is entered again, and the levels whose
    /// node is gone are released.
    pub fn replace(&mut self, root: NavNode) {
        self.root = root;
        self.engaged.retain(|e| self.root.find(e).is_some_and(|n| !n.leaving));
        self.layers.retain(|(l, _)| self.root.find(l).is_some());
        let alive = self.focus.as_ref().and_then(|f| self.root.find(f)).is_some_and(|n| !n.leaving);
        if !alive {
            self.focus = self.scope().entry().map(|e| e.name.clone());
            // The level emptied (its last stop went): released, the focus on the stop itself, and
            // outward until a level has somewhere to put it.
            while self.focus.is_none() {
                let Some(e) = self.engaged.pop() else { break };
                if self.root.find(&e).is_some_and(|n| !n.leaving) {
                    self.focus = Some(e);
                } else {
                    self.focus = self.scope().entry().map(|x| x.name.clone());
                }
            }
        }
        // Disabled while it had the focus: it keeps it (a press refuses) until the buttons move
        // it, from where it is (Android keeps the focus on a view disabled under it).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page: header back, a row of two buttons (transparent), a slider, a shelf of albums (a
    /// stop), a toggle, a disabled one, a label, the primary action, a dialog and a menu.
    fn page() -> NavNode {
        NavNode::group(
            "page",
            vec![
                NavNode::leaf("back").way_back(),
                NavNode::group("row", vec![NavNode::leaf("sync"), NavNode::leaf("like")]),
                NavNode::adjustable("volume"),
                NavNode::stop("shelf", vec![NavNode::leaf("a1"), NavNode::leaf("a2"), NavNode::leaf("a3")]),
                NavNode::leaf("crossfeed"),
                NavNode::leaf("eq").disabled(),
                NavNode::label("caption"),
                NavNode::leaf("continue").primary(),
                NavNode::layer("dialog", true, vec![NavNode::leaf("ok").primary(), NavNode::leaf("cancel")]),
                NavNode::layer("menu", false, vec![NavNode::leaf("m1"), NavNode::leaf("m2")]),
            ],
        )
    }

    fn walk(n: &mut Navigator, keys: &[Key]) -> Vec<Effect> {
        keys.iter().map(|k| n.key(*k)).collect()
    }

    #[test]
    fn transparent_containers_join_the_level_and_a_stop_is_one_stop() {
        let mut n = Navigator::new(page());
        assert_eq!(n.focus(), Some("continue"), "the primary action on entry");
        let up: Vec<_> = walk(&mut n, &[Key::Previous; 6]);
        assert_eq!(
            up,
            ["crossfeed", "shelf", "volume", "like", "sync", "back"].map(|s| Effect::Moved(s.into())).to_vec(),
            "the row's buttons in order, the shelf once, the label and the disabled skipped"
        );
        assert_eq!(n.key(Key::Previous), Effect::AtEnd, "the top of the screen");
    }

    #[test]
    fn a_stop_engaged_walks_its_children_and_lets_go_past_either_end() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        assert_eq!(n.key(Key::Action), Effect::Engaged("shelf".into()));
        assert_eq!((n.focus(), n.depth()), (Some("a1"), 1));
        assert_eq!(walk(&mut n, &[Key::Next, Key::Next]), vec![Effect::Moved("a2".into()), Effect::Moved("a3".into())]);
        assert_eq!(n.key(Key::Action), Effect::Activated("a3".into()), "Action inside takes the child");
        assert_eq!(n.key(Key::Next), Effect::Released("shelf".into()), "past the end: back to the stop");
        assert_eq!((n.focus(), n.depth()), (Some("shelf"), 0));
        assert_eq!(n.key(Key::Next), Effect::Moved("crossfeed".into()), "and on outward");
        // Coming back up into it and out the top.
        n.key(Key::Previous);
        n.key(Key::Action);
        assert_eq!(n.key(Key::Previous), Effect::Released("shelf".into()));
        // Escape from inside, too.
        n.key(Key::Action);
        n.key(Key::Next);
        assert_eq!(n.key(Key::Escape), Effect::Released("shelf".into()));
        assert_eq!(n.key(Key::Escape), Effect::Back, "and then back");
    }

    #[test]
    fn an_adjustable_engaged_takes_volume_as_its_value() {
        let mut n = Navigator::new(page());
        for _ in 0..3 {
            n.key(Key::Previous);
        }
        assert_eq!(n.focus(), Some("volume"));
        assert_eq!(n.key(Key::Next), Effect::Moved("shelf".into()), "not engaged: Volume moves");
        n.key(Key::Previous);
        assert_eq!(n.key(Key::Forward), Effect::Adjusted("volume".into(), 1), "→ at any time");
        n.key(Key::Action);
        assert_eq!(walk(&mut n, &[Key::Previous, Key::Next]), vec![Effect::Adjusted("volume".into(), 1), Effect::Adjusted("volume".into(), -1)]);
        assert_eq!(n.key(Key::Action), Effect::Released("volume".into()));
        n.key(Key::Action);
        assert_eq!(n.key(Key::Escape), Effect::Released("volume".into()), "Escape releases before it goes back");
    }

    #[test]
    fn an_adjustable_inside_a_stop_is_a_second_level() {
        let tree = NavNode::group(
            "page",
            vec![NavNode::stop("player", vec![NavNode::leaf("play"), NavNode::adjustable("seek")]), NavNode::leaf("after")],
        );
        let mut n = Navigator::new(tree);
        assert_eq!(n.focus(), Some("player"));
        n.key(Key::Action);
        n.key(Key::Next);
        assert_eq!(n.key(Key::Action), Effect::Engaged("seek".into()));
        assert_eq!(n.depth(), 2);
        assert_eq!(n.key(Key::Next), Effect::Adjusted("seek".into(), -1));
        assert_eq!(n.key(Key::Escape), Effect::Released("seek".into()));
        assert_eq!(n.key(Key::Escape), Effect::Released("player".into()));
        assert_eq!(n.key(Key::Next), Effect::Moved("after".into()));
    }

    #[test]
    fn arrows_walk_a_stop_from_the_side_they_come_from() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        assert_eq!(n.key(Key::Backward), Effect::Moved("a3".into()), "← enters at the far end");
        n.key(Key::Escape);
        assert_eq!(n.key(Key::Forward), Effect::Engaged("shelf".into()));
        assert_eq!(n.focus(), Some("a1"));
        assert_eq!(n.key(Key::Forward), Effect::Moved("a2".into()), "and walk it once inside");
        assert_eq!(n.key(Key::Backward), Effect::Moved("a1".into()));
        assert_eq!(n.key(Key::Backward), Effect::Released("shelf".into()), "out past its start");
        assert_eq!(n.key(Key::Forward), Effect::Engaged("shelf".into()));
    }

    #[test]
    fn disabled_is_skipped_with_all_inside_it() {
        let mut n = Navigator::new(page());
        assert_eq!(n.key(Key::Previous), Effect::Moved("crossfeed".into()), "eq, disabled, skipped");
        // A disabled group: none of its children is reached; a disabled primary is not landed on.
        let tree = NavNode::group(
            "p",
            vec![
                NavNode::leaf("first"),
                NavNode::group("off", vec![NavNode::leaf("x"), NavNode::stop("y", vec![NavNode::leaf("y1")])]).disabled(),
                NavNode::leaf("go").primary().disabled(),
                NavNode::leaf("last"),
            ],
        );
        let mut n = Navigator::new(tree);
        assert_eq!(n.focus(), Some("first"), "the disabled primary is not the entry");
        assert_eq!(n.key(Key::Next), Effect::Moved("last".into()));
        assert_eq!(n.key(Key::Next), Effect::AtEnd);
    }

    #[test]
    fn disabled_under_the_focus_keeps_it_and_the_buttons_go_on_from_there() {
        let mut n = Navigator::new(page());
        for _ in 0..3 {
            n.key(Key::Previous); // crossfeed, shelf, volume
        }
        let mut t = page();
        t.children[2] = NavNode::adjustable("volume").disabled();
        n.replace(t);
        assert_eq!(n.focus(), Some("volume"));
        assert_eq!(n.key(Key::Action), Effect::Refused("volume".into()));
        assert_eq!(n.key(Key::Next), Effect::Moved("shelf".into()), "on from its place");
        n.key(Key::Previous);
        assert_eq!(n.focus(), Some("like"), "and it is skipped from then on");
    }

    #[test]
    fn a_modal_traps_and_gives_the_focus_back_to_its_opener() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        n.key(Key::Action); // in the shelf
        assert_eq!(n.open("dialog"), Effect::Moved("ok".into()), "its primary; the shelf released");
        assert_eq!(n.depth(), 0);
        assert_eq!(walk(&mut n, &[Key::Next, Key::Next, Key::Previous]), ["cancel", "ok", "cancel"].map(|s| Effect::Moved(s.into())).to_vec(), "wraps inside");
        assert_eq!(n.key(Key::Escape), Effect::Closed { layer: "dialog".into(), to: "shelf".into() });
    }

    #[test]
    fn a_popover_does_not_wrap() {
        let mut n = Navigator::new(page());
        n.open("menu");
        assert_eq!(n.focus(), Some("m1"));
        assert_eq!(n.key(Key::Next), Effect::Moved("m2".into()));
        assert_eq!(n.key(Key::Next), Effect::AtEnd);
        assert_eq!(n.key(Key::Escape), Effect::Closed { layer: "menu".into(), to: "continue".into() });
    }

    #[test]
    fn a_touch_releases_the_levels_it_is_outside_of() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        n.key(Key::Action);
        n.touched("a2");
        assert_eq!((n.focus(), n.depth()), (Some("a2"), 1), "inside the shelf: still engaged");
        n.touched("sync");
        assert_eq!((n.focus(), n.depth()), (Some("sync"), 0));
        n.touched("a3");
        assert_eq!((n.focus(), n.depth(), n.engaged()), (Some("a3"), 1, Some("shelf")), "a touch inside a stop engages it");
        assert_eq!(n.key(Key::Next), Effect::Released("shelf".into()), "and the buttons continue from there");
        n.touched("shelf");
        assert_eq!(n.depth(), 0, "the stop itself: not inside it");
    }

    #[test]
    fn hiding_the_focus_or_the_display_releases_everything() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        n.key(Key::Action);
        n.key(Key::Next);
        n.release_all();
        assert_eq!((n.focus(), n.depth()), (Some("shelf"), 0), "back on the stop");
        n.key(Key::Previous);
        n.key(Key::Action);
        n.release_all();
        assert_eq!((n.focus(), n.depth()), (Some("volume"), 0), "an adjustable keeps the focus");
    }

    #[test]
    fn the_focus_follows_the_tree_as_it_changes() {
        let mut n = Navigator::new(page());
        n.key(Key::Previous);
        n.key(Key::Previous);
        n.key(Key::Action);
        n.key(Key::Next); // a2
        // a2 leaves (an exit animation): no longer reachable, the level is entered again.
        let mut t = page();
        t.children[3].children[1] = NavNode::leaf("a2").leaving();
        n.replace(t.clone());
        assert_eq!((n.focus(), n.depth()), (Some("a1"), 1));
        assert_eq!(n.key(Key::Next), Effect::Moved("a3".into()), "skips what is leaving");
        // The whole shelf goes: released, the screen's entry.
        t.children.remove(3);
        n.replace(t);
        assert_eq!((n.focus(), n.depth()), (Some("continue"), 0));
    }
}
