//! Compose's animation specs, so every value moves on the curve and clock
//! BitChord gave it: `tween` with a cubic-bezier easing, and `spring`.

use std::time::{Duration, Instant};

/// A CSS/Compose `CubicBezierEasing`.
#[derive(Clone, Copy, Debug)]
pub struct Bezier(pub f32, pub f32, pub f32, pub f32);

/// Compose's `FastOutSlowInEasing`, the default for `tween`.
pub const FAST_OUT_SLOW_IN: Bezier = Bezier(0.4, 0.0, 0.2, 1.0);
/// `LinearEasing`.
pub const LINEAR: Bezier = Bezier(0.0, 0.0, 1.0, 1.0);

impl Bezier {
    pub fn ease(self, x: f32) -> f32 {
        let Bezier(x1, y1, x2, y2) = self;
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let curve = |a: f32, b: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        // Bisection on x(t): monotonic for every easing used here.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let mut t = x;
        for _ in 0..32 {
            let at = curve(x1, x2, t);
            if (at - x).abs() < 1e-5 {
                break;
            }
            if at < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        curve(y1, y2, t)
    }
}

/// `animateFloatAsState(target, tween(duration, easing))`: retargeting starts
/// from wherever the value is now.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    delay: Duration,
    easing: Bezier,
}

impl Tween {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            duration: Duration::ZERO,
            delay: Duration::ZERO,
            easing: LINEAR,
        }
    }

    pub fn animate_to(&mut self, to: f32, duration_ms: u64, easing: Bezier) {
        self.animate_to_after(to, duration_ms, 0, easing);
    }

    pub fn animate_to_after(&mut self, to: f32, duration_ms: u64, delay_ms: u64, easing: Bezier) {
        if to == self.to {
            return;
        }
        let now = Instant::now();
        self.from = self.value_at(now);
        self.to = to;
        self.start = now;
        self.duration = Duration::from_millis(duration_ms);
        self.delay = Duration::from_millis(delay_ms);
        self.easing = easing;
    }

    pub fn snap_to(&mut self, value: f32) {
        self.from = value;
        self.to = value;
        self.duration = Duration::ZERO;
        self.delay = Duration::ZERO;
    }

    pub fn value(&self) -> f32 {
        self.value_at(Instant::now())
    }

    fn value_at(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.start);
        if elapsed < self.delay {
            return self.from;
        }
        let elapsed = elapsed - self.delay;
        if self.duration.is_zero() || elapsed >= self.duration {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * self.easing.ease(t)
    }

    pub fn running(&self) -> bool {
        Instant::now().saturating_duration_since(self.start) < self.delay + self.duration
    }
}

/// `spring(dampingRatio, stiffness)`, integrated rather than solved: the same
/// motion, sampled once per frame.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    value: f32,
    velocity: f32,
    target: f32,
    stiffness: f32,
    damping_ratio: f32,
    threshold: f32,
    last: Instant,
}

/// Compose's `Spring` constants.
pub const STIFFNESS_MEDIUM: f32 = 1500.0;
pub const STIFFNESS_MEDIUM_LOW: f32 = 400.0;
pub const DAMPING_NO_BOUNCY: f32 = 1.0;
pub const DAMPING_MEDIUM_BOUNCY: f32 = 0.5;

impl Spring {
    pub fn new(value: f32, damping_ratio: f32, stiffness: f32, threshold: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness,
            damping_ratio,
            threshold,
            last: Instant::now(),
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if target != self.target {
            self.step();
            self.target = target;
            self.last = Instant::now();
        }
    }

    pub fn running(&self) -> bool {
        (self.value - self.target).abs() > self.threshold || self.velocity.abs() > self.threshold
    }

    /// Advances the spring to now and returns where it is.
    pub fn step(&mut self) -> f32 {
        let now = Instant::now();
        let mut dt = now.saturating_duration_since(self.last).as_secs_f32().min(0.1);
        self.last = now;
        if !self.running() {
            self.value = self.target;
            self.velocity = 0.0;
            return self.value;
        }
        let omega = self.stiffness.sqrt();
        const STEP: f32 = 1.0 / 1000.0;
        while dt > 0.0 {
            let h = dt.min(STEP);
            let accel = -self.stiffness * (self.value - self.target)
                - 2.0 * self.damping_ratio * omega * self.velocity;
            self.velocity += accel * h;
            self.value += self.velocity * h;
            dt -= h;
        }
        if !self.running() {
            self.value = self.target;
            self.velocity = 0.0;
        }
        self.value
    }
}
