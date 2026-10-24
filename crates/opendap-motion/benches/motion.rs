//! Baselines of the motion primitives, sampled once a frame (ui-model §8), so optimisations are
//! measured against them.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use opendap_motion::{
    animator::{Animator, Kind},
    easing::Bezier,
    spring::Spring,
};

fn motion(c: &mut Criterion) {
    c.bench_function("spring/sample", |b| b.iter(|| black_box(Spring::SETTLE.at(0.0, 1800.0, 100.0, black_box(0.12)))));
    c.bench_function("easing/sample the standard curve", |b| b.iter(|| black_box(Bezier::STANDARD.at(black_box(0.42)))));
    c.bench_function("animator/retarget and sample", |b| {
        let mut a = Animator::new(0.0, Kind::Timed { duration: 0.18, curve: Bezier::STANDARD });
        let mut t = 0.0;
        b.iter(|| {
            t += 0.016;
            a.retarget(black_box(t * 10.0), t, None);
            black_box(a.sample(t + 0.008))
        })
    });
}

criterion_group!(benches, motion);
criterion_main!(benches);
