//! Tests for this module, split from the source file; one module per contract area.

mod contracts {
    use crate::anim::*;

    fn id(n: u64) -> Id {
        Id::from_u64(n)
    }

    /// A bounce that arrives at a page-scale coordinate must still land: integrated in f32
    /// its last steps are below one ulp, so it parks there with `live` true for ever.
    #[test]
    fn a_spring_parked_below_one_ulp_still_lands() {
        // The numbers are one stalled bounce as the engine's own report gave it.
        let mut e = SpringEntry {
            x: 1286.9996,
            v: 0.0034,
            target: 1287.0,
            stiffness: 280.0,
            damping: 30.0,
            last: 0.0,
            born: 0.0,
            pending: false,
            moved: 0.0,
            drives: 0,
            steps: 0,
            adv: 0,
            dt: 0.0,
        };
        for _ in 0..275 {
            e.step(1.0 / 275.0);
        }
        assert!(!e.live(), "one second must carry it past the last f32 ulp");
        assert_eq!((e.x, e.v), (1287.0, 0.0), "rest is exact, not merely near");
    }

    /// Tests drive the engine's clock by hand: a timestamp is the only time input.
    struct Clock(f64);

    impl Clock {
        fn new() -> Self {
            Self(0.0)
        }
        fn now(&self) -> f64 {
            self.0
        }
        /// Move forward by `dt` seconds, as one frame would.
        fn step(&mut self, dt: f64) {
            self.0 += dt;
        }
        fn set(&mut self, t: f64) {
            self.0 = t;
        }
    }

    #[test]
    fn a_value_that_was_never_animated_settles_immediately() {
        let clock = Clock::new();
        let mut store = AnimStore::new();
        let v = store.animate(id(1), 1.0, AnimSpec::new(0.2, Easing::EaseOut), clock.now());
        assert_eq!(v, 1.0);
        assert!(!store.is_animating(clock.now()));
    }

    #[test]
    fn a_retarget_travels_from_the_current_value_without_jumping() {
        let mut clock = Clock::new();
        let mut store = AnimStore::new();
        store.animate(id(1), 0.0, AnimSpec::new(0.1, Easing::Linear), clock.now());
        store.animate(id(1), 1.0, AnimSpec::new(0.1, Easing::Linear), clock.now());
        clock.step(0.05);
        let mid = store.value(id(1), clock.now()).expect("tracked");
        assert!(mid > 0.0 && mid < 1.0, "mid-flight value is {mid}");

        let after = store.animate(id(1), 0.0, AnimSpec::new(0.1, Easing::Linear), clock.now());
        assert!(
            (after - mid).abs() < 1e-6,
            "retarget must continue from {mid}, got {after}"
        );
    }

    #[test]
    fn keyframes_land_on_every_stop_and_end_at_the_last() {
        // Three stops with the curves the reference uses for the thumb's squash.
        let mut s = AnimStore::new();
        let id = Id::str("v");
        let stops = [
            Stop::new(0.25, 34.0, Easing::Linear),
            Stop::new(0.83, 24.0, Easing::Linear),
            Stop::new(1.0, 24.0, Easing::Linear),
        ];
        s.animate_stops(id, &stops, AnimSpec::new(1.0, Easing::Linear), 0.0);
        assert_eq!(
            s.value(id, 0.0),
            Some(24.0),
            "a fresh instance starts settled"
        );
        // Retarget the other way so the sequence actually runs.
        let down = [
            Stop::new(0.17, 16.0, Easing::Linear),
            Stop::new(0.75, 34.0, Easing::Linear),
            Stop::new(1.0, 16.0, Easing::Linear),
        ];
        s.animate_stops(id, &down, AnimSpec::new(1.0, Easing::Linear), 1.0);
        assert_eq!(s.value(id, 1.0), Some(24.0), "starts where it was");
        assert!(
            (s.value(id, 1.17).unwrap() - 16.0).abs() < 0.01,
            "first stop"
        );
        assert!((s.value(id, 1.75).unwrap() - 34.0).abs() < 0.01, "the pill");
        assert!(
            (s.value(id, 2.0).unwrap() - 16.0).abs() < 0.01,
            "the far end"
        );
    }

    #[test]
    fn retargeting_every_frame_follows_without_jumping_and_lands_exactly() {
        // A drag sets a new target every frame on a very short follow. Each retarget must
        // hand back the current value (that is the "sync first" half), trail the finger,
        // never overshoot, and land exactly once the finger stops.
        let spec = AnimSpec::new(0.024, Easing::Linear);
        let mut clock = Clock::new();
        let mut store = AnimStore::new();
        store.animate(id(1), 0.0, spec, clock.now());
        let mut target = 0.0f32;
        for _ in 0..20 {
            target = (target + 0.05).min(1.0);
            let before = store.value(id(1), clock.now()).expect("tracked");
            let returned = store.animate(id(1), target, spec, clock.now());
            assert!(
                (returned - before).abs() < 1e-6,
                "a retarget must sync to the current value: {before} -> {returned}"
            );
            clock.step(0.016);
            let v = store.value(id(1), clock.now()).expect("tracked");
            assert!(
                v <= target + 1e-6,
                "the value must not overshoot: {v} > {target}"
            );
            assert!(
                v >= before - 1e-6,
                "and must not go backwards: {before} -> {v}"
            );
        }
        for _ in 0..40 {
            store.animate(id(1), target, spec, clock.now());
            clock.step(0.016);
        }
        let end = store.value(id(1), clock.now()).expect("tracked");
        assert!(
            (end - target).abs() < 1e-3,
            "must land on {target}, got {end}"
        );
        assert!(!store.is_animating(clock.now()), "and must stop animating");
    }

    #[test]
    fn a_broken_clock_timestamp_cannot_rewind_or_poison_a_value() {
        let mut clock = Clock::new();
        let mut store = AnimStore::new();
        store.animate(id(1), 0.0, AnimSpec::new(0.1, Easing::Linear), clock.now());
        store.animate(id(1), 1.0, AnimSpec::new(0.1, Easing::Linear), clock.now());
        clock.step(0.05);
        let mid = store.value(id(1), clock.now()).expect("tracked");

        // The engine clock only moves forward; these are the timestamps a broken host
        // could still hand in, and none of them may rewind or poison a value.
        clock.set(clock.now() - 1.0);
        assert!(
            store
                .value(id(1), clock.now())
                .is_some_and(|v| v <= mid + 1e-6),
            "stepping back must not advance"
        );
        clock.set(f64::NAN);
        let nan = store.value(id(1), clock.now()).expect("tracked");
        assert!(
            nan.is_finite(),
            "a NaN timestamp must not poison the value, got {nan}"
        );
        clock.set(f64::INFINITY);
        let inf = store.value(id(1), clock.now()).expect("tracked");
        assert!(
            inf.is_finite(),
            "an infinite timestamp must not poison the value, got {inf}"
        );
    }

    /// The same elapsed time must produce the same spring position at 60 Hz and at 120 Hz;
    /// a per-frame integrator would make a slow display also slow the motion.
    #[test]
    fn a_spring_travels_the_same_distance_at_any_frame_rate() {
        let run = |fps: u32| {
            let mut clock = Clock::new();
            let mut store = AnimStore::new();
            store.spring(id(1), 0.0, 322.29, 30.51, clock.now());
            let step = 1.0 / fps as f64;
            let mut x = 0.0;
            for _ in 0..fps {
                clock.step(step);
                x = store.spring(id(1), 1.0, 322.29, 30.51, clock.now()).x;
            }
            x
        };
        let (slow, fast) = (run(60), run(120));
        assert!(
            (slow - fast).abs() < 5e-3,
            "60 Hz reads {slow}, 120 Hz reads {fast}"
        );
        assert!(
            slow > 0.9,
            "a settling spring must have travelled, got {slow}"
        );
    }

    /// A spring in flight is motion: the frame loop must stay awake, and must stop once it
    /// rests, or a control would freeze mid-ring or spin forever.
    #[test]
    fn a_spring_keeps_the_frame_loop_awake_until_it_rests() {
        let mut clock = Clock::new();
        let mut store = AnimStore::new();
        store.spring(id(1), 0.0, 322.29, 30.51, clock.now());
        clock.step(1.0 / 60.0);
        store.spring(id(1), 1.0, 322.29, 30.51, clock.now());
        assert!(store.is_animating(clock.now()), "the spring just started");
        for _ in 0..120 {
            clock.step(1.0 / 60.0);
            store.spring(id(1), 1.0, 322.29, 30.51, clock.now());
        }
        assert!(
            !store.is_animating(clock.now()),
            "two seconds is enough to rest"
        );
        let at = store.spring(id(1), 1.0, 322.29, 30.51, clock.now());
        assert_eq!((at.x, at.v), (1.0, 0.0), "rest is exact, not merely small");
    }
}

mod sequences {
    use crate::anim::*;

    /// A stop is reached exactly at its own weight, and the first one starts from `from`.
    #[test]
    fn weighted_hits_every_stop_exactly() {
        let stops = [
            (0.11, Easing::Linear, 34.0_f32),
            (0.83, Easing::Linear, 24.0),
        ];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        assert_eq!(weighted(0.0, &stops, 16.0, lerp), 16.0);
        assert!((weighted(0.11, &stops, 16.0, lerp) - 34.0).abs() < 1e-4);
        assert!((weighted(0.83, &stops, 16.0, lerp) - 24.0).abs() < 1e-4);
        assert_eq!(weighted(1.0, &stops, 16.0, lerp), 24.0);
    }

    /// The shared leg clock: a zero duration waits only for its delay, a clock behind the
    /// leg holds its start, and a clock with no usable value lands the leg.
    #[test]
    fn the_leg_clock_waits_out_its_delay_and_holds_a_backwards_time() {
        let write = AnimSpec::new(0.0, Easing::Linear).delayed(0.2);
        assert_eq!(write.progress_at(1.0, 1.1), 0.0);
        assert_eq!(write.progress_at(1.0, 1.25), 1.0);
        let leg = AnimSpec::new(2.0, Easing::Linear);
        assert_eq!(leg.progress_at(5.0, 4.0), 0.0);
        assert_eq!(leg.progress_at(5.0, 6.0), 0.5);
        assert_eq!(leg.progress_at(5.0, f64::INFINITY), 1.0);
        assert_eq!(leg.progress_at(5.0, f64::NAN), 1.0);
    }

    /// A loop's phase wraps, and an age before the loop's start reads as its start.
    #[test]
    fn phase_wraps_a_loop_and_refuses_a_backwards_clock() {
        assert!((phase(1.5, 2.0) - 0.75).abs() < 1e-6);
        assert!((phase(2.5, 2.0) - 0.25).abs() < 1e-6);
        assert_eq!(phase(-1.0, 2.0), 0.0);
        assert_eq!(phase(f32::NAN, 2.0), 0.0);
        assert_eq!(phase(1.0, 0.0), 0.0);
    }
}
