//! Baselines of the input primitives on every contact and key (input events §3–§6), so
//! optimisations are measured against them.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use opendap_input::{
    arena::{Arena, DoubleTap, LongPress, Priority, Tap},
    dispatch::{Directives, Listener, dispatch},
    gesture::Frame,
    harness::{Script, play},
    nav::{Key, NavNode, Navigator},
    nested::{RevealArea, Scroller, reveal},
    node::{Node, NodeId, NodeKind, NodePath},
};

fn arena(c: &mut Criterion) {
    let script = Script::tap((10.0, 10.0), 0, 80).then(Script::drag((10.0, 10.0), (200.0, 10.0), 400, 250, 20));
    c.bench_function("arena/a tap then a drag through three recognizers", |b| {
        b.iter(|| {
            let mut a = Arena::new()
                .add(Tap::new(1.0), Priority::Normal)
                .add(DoubleTap::new(1.0), Priority::Normal)
                .add(LongPress::new(1.0), Priority::Normal);
            black_box(play(&mut a, &script))
        })
    });
}

fn dispatching(c: &mut Criterion) {
    let n = |k, name: &str| Node::new(NodeId::new(k, name), Frame::new((10.0, 10.0), 1.0));
    let path = NodePath(vec![
        n(NodeKind::Activity, "music"),
        n(NodeKind::Layer, "content"),
        n(NodeKind::Container, "queue"),
        n(NodeKind::Container, "list"),
        n(NodeKind::Element, "track-12"),
    ]);
    let mut listeners: Vec<Listener> = (0..32)
        .map(|i| {
            let at = match i % 4 {
                0 => "activity:music",
                1 => "**/element:*",
                2 => "activity:music/layer:content/container:queue",
                _ => "**/container:list",
            };
            Listener::new(i, at, "tap", Directives { capture: i % 3 == 0, ..Default::default() }).unwrap()
        })
        .collect();
    c.bench_function("dispatch/a tap down a five-node path, 32 listeners", |b| {
        b.iter(|| black_box(dispatch(&mut listeners, "tap", &path, 0)))
    });
}

fn navigation(c: &mut Criterion) {
    // A level of 1000 stops: the model walks a level by building it, so this is its cost.
    let tree = NavNode::group("screen", (0..1000).map(|i| NavNode::leaf(&format!("item-{i}"))).collect());
    c.bench_function("nav/a step in a level of 1000", |b| {
        let mut n = Navigator::new(tree.clone());
        b.iter(|| {
            if n.key(Key::Next) == opendap_input::nav::Effect::AtEnd {
                n = Navigator::new(tree.clone());
            }
        })
    });
    let chain = [
        RevealArea { scroller: Scroller::new(4800.0, 300.0), viewport: 300.0, at: 200.0, inset_start: 28.0, inset_end: 0.0 },
        RevealArea { scroller: Scroller::new(1200.0, 400.0), viewport: 400.0, at: 0.0, inset_start: 0.0, inset_end: 64.0 },
    ];
    c.bench_function("nested/reveal through two scroll areas", |b| {
        b.iter(|| {
            let mut ch = chain;
            black_box(reveal(&mut ch, black_box(2336.0), 2384.0))
        })
    });
}

criterion_group!(benches, arena, dispatching, navigation);
criterion_main!(benches);
