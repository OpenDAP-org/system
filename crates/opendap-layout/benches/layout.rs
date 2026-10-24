//! Baselines of the layout primitives at the sizes T1-046 asks for (a library of a million items),
//! so optimisations are measured against them (input events §3.7, ui-model §7.1).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use opendap_layout::{
    data::{
        cache::Cache,
        pages::{Pages, Paging},
    },
    extents::{Extents, Measured},
    focus,
    recycle::Pool,
    window::{Overscan, window},
};

const ITEMS: usize = 1_000_000;

fn extents(c: &mut Criterion) {
    let mut e = Measured::new(ITEMS, 48.0);
    for i in (0..ITEMS).step_by(7) {
        e.measure(i, 72.0);
    }
    c.bench_function("extents/offset of an item (1 M)", |b| b.iter(|| e.offset(black_box(654_321))));
    c.bench_function("extents/item at an offset (1 M)", |b| b.iter(|| e.index_at(black_box(31_000_000.0))));
    c.bench_function("extents/measure an item (1 M)", |b| {
        let mut i = 0;
        b.iter(|| {
            i = (i + 7919) % ITEMS;
            e.measure(black_box(i), 60.0);
        })
    });
}

fn windowing(c: &mut Criterion) {
    let e = Measured::new(ITEMS, 48.0);
    c.bench_function("window/one frame's window (1 M, flinging)", |b| {
        b.iter(|| window(&e, black_box(24_000_000.0), 260.0, black_box(3000.0), Overscan::default()).len())
    });
    c.bench_function("recycle/slide the window by one item", |b| {
        let mut pool: Pool<usize> = Pool::new();
        let mut start = 0;
        b.iter(|| {
            start += 1;
            let keys: Vec<usize> = (start..start + 20).collect();
            black_box(pool.update(&keys));
        })
    });
}

fn data(c: &mut Criterion) {
    c.bench_function("pages/plan a window (1 M)", |b| {
        let mut p = Pages::new(Paging::default(), Some(ITEMS));
        let mut at = 0;
        b.iter(|| {
            at = (at + 37) % (ITEMS - 40);
            black_box(p.plan(at..at + 30, true));
        })
    });
    c.bench_function("cache/put within a budget of 60", |b| {
        let mut cache: Cache<usize, ()> = Cache::new(60);
        let mut k = 0;
        b.iter(|| {
            k += 1;
            black_box(cache.put(k, (), 1));
        })
    });
}

fn focus_steps(c: &mut Criterion) {
    let e = Measured::new(ITEMS, 48.0);
    c.bench_function("focus/step and keep in view (1 M)", |b| {
        let mut at = 0.0;
        let mut i = 0;
        b.iter(|| {
            if let focus::Step::Item(n) = focus::step(Some(i), 1, ITEMS) {
                i = n % (ITEMS - 1);
            }
            at = focus::keep_in_view(&e, i, at, 260.0);
            black_box(at)
        })
    });
}

criterion_group!(benches, extents, windowing, data, focus_steps);
criterion_main!(benches);
