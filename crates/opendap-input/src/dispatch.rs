//! Dispatch (input events §4, §5): an event travels its path, down in the capture phase, at its
//! target, up in the bubble phase; handlers declare directives, and defaults happen unless one
//! prevents them (W3C DOM events, Vue's modifiers). On top, intents and actions: what an input
//! means is an intent, handled by the first enabled action found up the path from the focus
//! (Flutter's Actions and Intents).
//!
//! Pure data and functions over [`NodePath`]s: a runtime owns the handlers' code; this decides
//! who runs, in which order, and what the directives let through.

use crate::node::{NodePath, Selector};

/// The phase a handler runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Capture,
    Target,
    Bubble,
}

/// What a handler declares (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Directives {
    /// Runs once, then is removed.
    pub once: bool,
    /// Stops the event's travel after it.
    pub stop: bool,
    /// Prevents the default action.
    pub prevent: bool,
    /// Runs in the capture phase instead of the bubble phase.
    pub capture: bool,
    /// Runs only when its node is the target.
    pub self_only: bool,
    /// Promises not to prevent (scrolling never waits for it); a `prevent` with it is ignored.
    pub passive: bool,
    /// At most one call per this many ms, the first of each interval run.
    pub throttle_ms: Option<u64>,
    /// One call after the events stop for this many ms (the caller runs it at `debounce_due`).
    pub debounce_ms: Option<u64>,
}

/// A handler: where it listens (a node, by selector), for which event type, with its directives.
#[derive(Debug, Clone)]
pub struct Listener {
    pub id: u32,
    pub at: Selector,
    pub event: String,
    pub directives: Directives,
    last_run: Option<u64>,
    removed: bool,
}

impl Listener {
    pub fn new(id: u32, at: &str, event: &str, directives: Directives) -> Option<Self> {
        Some(Listener {
            id,
            at: Selector::parse(at)?,
            event: event.to_string(),
            directives,
            last_run: None,
            removed: false,
        })
    }
}

/// What a dispatch decided: the handlers to run in order, with their node depth and phase, and
/// whether the default action still happens.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Route {
    pub runs: Vec<(u32, usize, Phase)>,
    pub default_prevented: bool,
    /// Debounced handlers to run later, at this time, unless the event comes again first.
    pub debounced: Vec<(u32, u64)>,
}

/// Dispatches an event of type `event` at `target` (its path), at `ms`, through `listeners`:
/// capture from the outermost node down to the target's parent, the target, then bubble back
/// up. Applies `once`, `stop`, `prevent` (not when `passive`), `capture`, `self`, `throttle`
/// and `debounce`.
pub fn dispatch(listeners: &mut [Listener], event: &str, target: &NodePath, ms: u64) -> Route {
    let mut route = Route::default();
    let depth = target.0.len();
    if depth == 0 {
        return route;
    }
    let prefix = |d: usize| NodePath(target.0[..=d].to_vec());
    // The order: capture listeners top-down (excluding the target), the target's listeners,
    // bubble listeners bottom-up (excluding the target).
    let mut order: Vec<(usize, Phase, bool)> = Vec::new();
    for d in 0..depth - 1 {
        order.push((d, Phase::Capture, true));
    }
    order.push((depth - 1, Phase::Target, true));
    order.push((depth - 1, Phase::Target, false));
    for d in (0..depth - 1).rev() {
        order.push((d, Phase::Bubble, false));
    }
    for (d, phase, capture_list) in order {
        let node_path = prefix(d);
        let mut stopped = false;
        for l in listeners.iter_mut() {
            if l.removed || l.event != event || l.directives.capture != capture_list || !l.at.matches(&node_path) {
                continue;
            }
            if l.directives.self_only && d != depth - 1 {
                continue;
            }
            if let Some(every) = l.directives.throttle_ms
                && l.last_run.is_some_and(|t| ms.saturating_sub(t) < every)
            {
                continue;
            }
            if let Some(wait) = l.directives.debounce_ms {
                route.debounced.push((l.id, ms + wait));
                continue;
            }
            l.last_run = Some(ms);
            route.runs.push((l.id, d, phase));
            if l.directives.prevent && !l.directives.passive {
                route.default_prevented = true;
            }
            if l.directives.once {
                l.removed = true;
            }
            if l.directives.stop {
                stopped = true;
            }
        }
        if stopped {
            break;
        }
    }
    route
}

/// An intent: what an input means (a semantic action, input events §3.4), as plain data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Intent(pub String);

/// An action that handles an intent at a node: found up the path from the focus, the first
/// enabled one wins; a disabled one lets the intent pass to the next ancestor.
#[derive(Debug, Clone)]
pub struct Action {
    pub at: Selector,
    pub intent: Intent,
    pub enabled: bool,
    pub name: String,
}

/// The action that handles `intent` for the focused node `focus`: the nearest enabled one from
/// the focus up to the outermost node; `None` when nobody handles it (the system's default).
pub fn resolve<'a>(actions: &'a [Action], intent: &Intent, focus: &NodePath) -> Option<&'a Action> {
    for d in (0..focus.0.len()).rev() {
        let at = NodePath(focus.0[..=d].to_vec());
        if let Some(a) = actions.iter().find(|a| &a.intent == intent && a.enabled && a.at.matches(&at)) {
            return Some(a);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::Frame;
    use crate::node::{Node, NodeId, NodeKind};

    fn path() -> NodePath {
        let n = |k, name: &str| Node::new(NodeId::new(k, name), Frame::new((10.0, 10.0), 1.0));
        NodePath(vec![
            n(NodeKind::Activity, "music"),
            n(NodeKind::Container, "queue"),
            n(NodeKind::Element, "track-12"),
        ])
    }

    fn l(id: u32, at: &str, d: Directives) -> Listener {
        Listener::new(id, at, "tap", d).unwrap()
    }

    #[test]
    fn capture_then_target_then_bubble() {
        let mut ls = vec![
            l(1, "activity:music", Directives::default()),
            l(2, "activity:music", Directives { capture: true, ..Default::default() }),
            l(3, "**/element:track-12", Directives::default()),
            l(4, "activity:music/container:queue", Directives::default()),
        ];
        let r = dispatch(&mut ls, "tap", &path(), 0);
        let ids: Vec<u32> = r.runs.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec![2, 3, 4, 1]);
        assert_eq!(r.runs[0].2, Phase::Capture);
        assert_eq!(r.runs[1].2, Phase::Target);
    }

    #[test]
    fn stop_once_self_and_prevent_but_not_when_passive() {
        let mut ls = vec![
            l(1, "activity:music", Directives::default()),
            l(2, "activity:music/container:queue", Directives { stop: true, prevent: true, ..Default::default() }),
            l(3, "**/element:*", Directives { once: true, ..Default::default() }),
            l(4, "activity:music", Directives { self_only: true, ..Default::default() }),
        ];
        let r = dispatch(&mut ls, "tap", &path(), 0);
        assert_eq!(r.runs.iter().map(|r| r.0).collect::<Vec<_>>(), vec![3, 2], "stopped at the queue");
        assert!(r.default_prevented);
        let r = dispatch(&mut ls, "tap", &path(), 10);
        assert_eq!(r.runs.iter().map(|r| r.0).collect::<Vec<_>>(), vec![2], "once: gone");
        let mut ls = vec![l(1, "**", Directives { prevent: true, passive: true, ..Default::default() })];
        assert!(!dispatch(&mut ls, "tap", &path(), 0).default_prevented, "passive never prevents");
    }

    #[test]
    fn throttle_and_debounce() {
        let mut ls = vec![
            l(1, "**/element:*", Directives { throttle_ms: Some(100), ..Default::default() }),
            l(2, "**/element:*", Directives { debounce_ms: Some(300), ..Default::default() }),
        ];
        assert_eq!(dispatch(&mut ls, "tap", &path(), 0).runs.len(), 1);
        assert_eq!(dispatch(&mut ls, "tap", &path(), 50).runs.len(), 0, "throttled");
        let r = dispatch(&mut ls, "tap", &path(), 120);
        assert_eq!(r.runs.len(), 1);
        assert_eq!(r.debounced, vec![(2, 420)]);
    }

    #[test]
    fn an_intent_goes_to_the_nearest_enabled_action_up_from_the_focus() {
        let a = |at: &str, enabled, name: &str| Action {
            at: Selector::parse(at).unwrap(),
            intent: Intent("next-track".into()),
            enabled,
            name: name.into(),
        };
        let actions = vec![
            a("activity:music", true, "player"),
            a("activity:music/container:queue", false, "queue (disabled)"),
        ];
        let got = resolve(&actions, &Intent("next-track".into()), &path()).unwrap();
        assert_eq!(got.name, "player", "a disabled action lets it pass up");
        assert!(resolve(&actions, &Intent("volume-up".into()), &path()).is_none(), "the system's default");
    }
}
