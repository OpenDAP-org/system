//! Motion's invariants over generated input (ADR 0014, level L1): a transition under any sequence
//! of system starts, grabs, drags and releases at any times; presence under any sequence of lists;
//! interpolation at any progress. A failure is shrunk to the smallest case that breaks one.

use opendap_motion::interpolate::{Frame, color, fade_through, frame};
use opendap_motion::presence::{Phase, Presence};
use opendap_motion::transition::{End, Transition};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Go(bool),
    Grab,
    Drag(f32),
    Release(f32),
    Wait(f32),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<bool>().prop_map(Op::Go),
        Just(Op::Grab),
        (-600.0f32..600.0).prop_map(Op::Drag),
        (-4000.0f32..4000.0).prop_map(Op::Release),
        (0.0f32..0.3).prop_map(Op::Wait),
    ]
}

fn end(to: bool) -> End {
    if to { End::To } else { End::From }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3000, ..ProptestConfig::default() })]

    /// Whatever hands and the system do: the progress stays between its ends, nothing jumps when a
    /// hand or the system takes over, and left alone it comes to rest exactly at the end it was
    /// sent to.
    #[test]
    fn a_transition_never_jumps_and_always_lands(start in any::<bool>(), distance in 40.0f32..800.0, reduce in any::<bool>(), ops in prop::collection::vec(op(), 1..30)) {
        let mut t = Transition::new(end(start), distance);
        t.reduce_motion = reduce;
        let mut now = 0.0f32;
        let mut sent = end(start);
        let mut held = false;
        for o in ops {
            let before = t.progress(now);
            prop_assert!((0.0..=1.0).contains(&before));
            match o {
                Op::Go(to) => {
                    t.go(end(to), now);
                    sent = end(to);
                    held = false;
                    if !reduce {
                        prop_assert!((t.progress(now) - before).abs() < 1e-4, "a start from where it is: {} → {}", before, t.progress(now));
                    }
                }
                Op::Grab => {
                    t.grab(now);
                    held = true;
                    prop_assert!((t.progress(now) - before).abs() < 1e-6, "caught where it is");
                }
                Op::Drag(d) => {
                    t.drag(d);
                    if held {
                        prop_assert!((t.progress(now) - (before + d / distance).clamp(0.0, 1.0)).abs() < 1e-4, "under the finger");
                    } else {
                        prop_assert_eq!(t.progress(now), before, "no hand, no drag");
                    }
                }
                Op::Release(v) => {
                    sent = t.release(v, now);
                    held = false;
                    if !reduce {
                        prop_assert!((t.progress(now) - before).abs() < 1e-4, "let go where it is");
                    }
                }
                Op::Wait(s) => now += s,
            }
            if reduce && !held {
                prop_assert!(!t.running(now), "reduce motion: no frames");
            }
        }
        if held {
            sent = t.release(0.0, now);
        }
        now += 5.0;
        prop_assert!(!t.running(now), "it rests");
        prop_assert_eq!(t.settled(now), Some(sent));
        prop_assert_eq!(t.progress(now), if sent == End::To { 1.0 } else { 0.0 }, "exactly at its end");
    }

    /// A release settles where the hand meant: a flick toward an end goes there, a slow release
    /// goes to the nearer end.
    #[test]
    fn a_release_goes_where_the_hand_meant(at in 0.0f32..1.0, v in -4000.0f32..4000.0) {
        let mut t = Transition::new(End::From, 320.0);
        t.grab(0.0);
        t.drag(at * 320.0);
        let e = t.release(v, 0.0);
        if v.abs() < 1.0 && (at - 0.5).abs() > 0.01 {
            prop_assert_eq!(e, if at > 0.5 { End::To } else { End::From });
        }
        if v > 1500.0 { prop_assert_eq!(e, End::To); }
        if v < -1500.0 { prop_assert_eq!(e, End::From); }
    }
}

fn keys() -> impl Strategy<Value = Vec<u8>> {
    // Distinct keys, in some order.
    prop::collection::btree_set(0u8..10, 0..8).prop_flat_map(|s| Just(s.into_iter().collect::<Vec<_>>()).prop_shuffle())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3000, ..ProptestConfig::default() })]

    /// Whatever lists come in, at whatever times: what is wanted is drawn, in the wanted order, never
    /// as leaving; what leaves is drawn only while it animates out; progress stays between 0 and 1;
    /// left alone, exactly the last list remains, fully present.
    #[test]
    fn presence_draws_what_is_wanted_in_order(lists in prop::collection::vec((keys(), 0.0f32..0.3), 1..12), reduce in any::<bool>()) {
        let mut p = Presence::default();
        p.reduce_motion = reduce;
        let mut now = 0.0;
        let mut last = Vec::new();
        for (ks, wait) in lists {
            p.update(&ks, now);
            let f = p.frame(now);
            let drawn: Vec<u8> = f.iter().filter(|(_, ph, _)| *ph != Phase::Leaving).map(|(k, _, _)| *k).collect();
            prop_assert_eq!(&drawn, &ks, "the wanted keys, in their order");
            for (k, ph, x) in &f {
                prop_assert!((0.0..=1.0).contains(x), "{k}: {x}");
                if reduce {
                    prop_assert_eq!(*ph, Phase::Present, "reduce motion: nothing animates");
                }
            }
            let mut seen = std::collections::HashSet::new();
            prop_assert!(f.iter().all(|(k, _, _)| seen.insert(*k)), "each key once");
            last = ks;
            now += wait;
        }
        now += 2.0;
        prop_assert!(!p.running(now));
        let f = p.frame(now);
        prop_assert_eq!(f.iter().map(|(k, _, _)| *k).collect::<Vec<_>>(), last);
        prop_assert!(f.iter().all(|(_, ph, x)| *ph == Phase::Present && *x == 1.0));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 5000, ..ProptestConfig::default() })]

    /// A shared element's frame is at its ends at 0 and 1, and between them in between.
    #[test]
    fn a_frame_moves_between_its_places(a in prop::array::uniform5(-500.0f32..500.0), b in prop::array::uniform5(-500.0f32..500.0), t in 0.0f32..=1.0) {
        let fa = Frame::new(a[0], a[1], a[2], a[3], a[4]);
        let fb = Frame::new(b[0], b[1], b[2], b[3], b[4]);
        prop_assert_eq!(frame(fa, fb, 0.0), fa);
        let one = frame(fa, fb, 1.0);
        for (x, y) in [(one.x, fb.x), (one.y, fb.y), (one.w, fb.w), (one.h, fb.h), (one.radius, fb.radius)] {
            prop_assert!((x - y).abs() < 1e-3);
        }
        let m = frame(fa, fb, t);
        for (x, lo, hi) in [(m.x, fa.x, fb.x), (m.w, fa.w, fb.w), (m.radius, fa.radius, fb.radius)] {
            prop_assert!(x >= lo.min(hi) - 1e-3 && x <= lo.max(hi) + 1e-3);
        }
    }

    /// Colours: the ends are the ends, and every channel of a mix lies between the two ends'.
    #[test]
    fn a_colour_mix_stays_between_its_ends(a in 0u32..0x1000000, b in 0u32..0x1000000, t in 0.0f32..=1.0) {
        prop_assert_eq!(color(a, b, 0.0), a);
        prop_assert_eq!(color(a, b, 1.0), b);
        let m = color(a, b, t);
        for s in [16, 8, 0] {
            let (x, y, z) = ((a >> s) & 0xFF, (b >> s) & 0xFF, (m >> s) & 0xFF);
            prop_assert!(z >= x.min(y) && z <= x.max(y), "channel {s}: {z} outside {x}–{y}");
        }
    }

    /// Fade through: the two contents never show together, and each is fully there at its end.
    #[test]
    fn fade_through_never_shows_both(t in 0.0f32..=1.0) {
        let (out, inn) = fade_through(t);
        prop_assert!(out == 0.0 || inn == 0.0, "at {t}: {out} and {inn}");
        prop_assert_eq!(fade_through(0.0), (1.0, 0.0));
        prop_assert_eq!(fade_through(1.0), (0.0, 1.0));
    }
}
