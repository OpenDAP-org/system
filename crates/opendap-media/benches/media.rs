//! Baselines of the media primitives (ui-model §7.3), so optimisations are measured against them.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use opendap_media::{
    fit::{Fit, decode_size},
    plot::{Ring, min_max},
};

fn plot(c: &mut Criterion) {
    // About 23 s of 44.1 kHz audio, to the 300 columns of a waveform.
    let samples: Vec<f32> = (0..1_000_000).map(|i| ((i as f32) * 0.01).sin()).collect();
    c.bench_function("plot/a million samples to 300 columns", |b| b.iter(|| black_box(min_max(&samples, 300))));
    c.bench_function("plot/push into a meter's ring", |b| {
        let mut r = Ring::new(512);
        let mut v = 0.0;
        b.iter(|| {
            v += 0.001;
            r.push(black_box(v));
        })
    });
}

fn fit(c: &mut Criterion) {
    c.bench_function("fit/decode size of a cover", |b| {
        b.iter(|| black_box(decode_size(black_box((3000, 3000)), (96.0, 96.0), 1.0, Fit::Cover)))
    });
}

criterion_group!(benches, plot, fit);
criterion_main!(benches);
