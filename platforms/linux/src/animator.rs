//! Native transitions declared with `.transition(…)` / `.spring(…)`: the core
//! marks a batch ANIMATED, and the frame and opacity of nodes that declared a
//! transition for that property move to their new value on GTK's frame clock.
//!
//! This module is the time math only (easing curves, springs, interpolation);
//! [`crate::node_view::NodeView`] drives it from a tick callback.

use native_shell_core::protocol::Frame;
use serde_json::Value;

/// How progress (0…1, springs may overshoot) follows time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// CSS `cubic-bezier(x1, y1, x2, y2)` over `duration` seconds.
    Timing { duration: f64, bezier: [f64; 4] },
    /// A unit-mass spring released from rest, SwiftUI-style: `response` is the
    /// period of the undamped spring in seconds, `damping` the damping ratio.
    Spring { response: f64, damping: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anim {
    pub curve: Curve,
    /// Seconds before the value starts moving.
    pub delay: f64,
}

/// Progress below this distance from the target counts as settled.
const SETTLED: f64 = 1e-3;
/// No declared transition runs longer than this, whatever the script says.
const MAX_SECONDS: f64 = 10.0;

impl Anim {
    /// `{kind: "timing"|"spring", duration, delay (ms), easing, response, damping}`.
    pub fn parse(v: &Value) -> Option<Anim> {
        let v = v.as_object()?;
        let num = |key: &str| v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite());
        let delay = (num("delay").unwrap_or(0.0) / 1000.0).clamp(0.0, MAX_SECONDS);
        let curve = if v.get("kind").and_then(Value::as_str) == Some("spring") {
            Curve::Spring {
                response: num("response").unwrap_or(0.35).clamp(0.05, MAX_SECONDS),
                damping: num("damping").unwrap_or(0.825).clamp(0.05, 10.0),
            }
        } else {
            let bezier = match v.get("easing").and_then(Value::as_str) {
                Some("linear") => [0.0, 0.0, 1.0, 1.0],
                Some("ease_in") => [0.42, 0.0, 1.0, 1.0],
                Some("ease_out") => [0.0, 0.0, 0.58, 1.0],
                _ => [0.42, 0.0, 0.58, 1.0],
            };
            Curve::Timing {
                duration: (num("duration").unwrap_or(250.0) / 1000.0).clamp(0.0, MAX_SECONDS),
                bezier,
            }
        };
        Some(Anim { curve, delay })
    }

    /// Seconds from the start (delay included) until the value rests at its target.
    pub fn total(&self) -> f64 {
        self.delay
            + match self.curve {
                Curve::Timing { duration, .. } => duration,
                Curve::Spring { response, damping } => spring_settle_time(response, damping),
            }
    }

    /// Progress at `elapsed` seconds since the start; exactly 1 once finished.
    pub fn progress(&self, elapsed: f64) -> f64 {
        let t = elapsed - self.delay;
        if t <= 0.0 {
            return 0.0;
        }
        if elapsed >= self.total() {
            return 1.0;
        }
        match self.curve {
            Curve::Timing { duration, bezier } => {
                if duration <= 0.0 {
                    1.0
                } else {
                    cubic_bezier(bezier, t / duration)
                }
            }
            Curve::Spring { response, damping } => spring(response, damping, t),
        }
    }
}

/// The `Animation` prop: which of a node's properties animate in ANIMATED batches.
/// Only `frame` and `opacity` exist on every renderer; other keys are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimationSpec {
    pub frame: Option<Anim>,
    pub opacity: Option<Anim>,
}

impl AnimationSpec {
    pub fn parse(json: &str) -> Option<AnimationSpec> {
        let v: Value = serde_json::from_str(json).ok()?;
        let spec = AnimationSpec {
            frame: v.get("frame").and_then(Anim::parse),
            opacity: v.get("opacity").and_then(Anim::parse),
        };
        (spec.frame.is_some() || spec.opacity.is_some()).then_some(spec)
    }
}

/// CSS `cubic-bezier` easing: solves x(s) = `x` for the curve parameter, returns y(s).
pub fn cubic_bezier([x1, y1, x2, y2]: [f64; 4], x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let coord = |a: f64, b: f64, s: f64| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let slope = |a: f64, b: f64, s: f64| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    // Newton first (fast where the curve is steep), bisection when it stalls.
    let mut s = x;
    for _ in 0..8 {
        let err = coord(x1, x2, s) - x;
        if err.abs() < 1e-7 {
            return coord(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s -= err / d;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    s = x;
    for _ in 0..40 {
        let v = coord(x1, x2, s);
        if (v - x).abs() < 1e-7 {
            break;
        }
        if v < x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    coord(y1, y2, s)
}

/// Position of a unit spring going from 0 to 1, released at rest, after `t` seconds.
pub fn spring(response: f64, damping: f64, t: f64) -> f64 {
    let w0 = std::f64::consts::TAU / response;
    let z = damping;
    if z < 1.0 {
        let wd = w0 * (1.0 - z * z).sqrt();
        1.0 - (-z * w0 * t).exp() * ((wd * t).cos() + (z * w0 / wd) * (wd * t).sin())
    } else if z == 1.0 {
        1.0 - (-w0 * t).exp() * (1.0 + w0 * t)
    } else {
        let s = (z * z - 1.0).sqrt();
        let r1 = -w0 * (z - s);
        let r2 = -w0 * (z + s);
        // x(0) = 0 and x'(0) = 0.
        let c2 = r1 / (r2 - r1);
        let c1 = -1.0 - c2;
        1.0 + c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
    }
}

/// When the spring stays within [`SETTLED`] of its target for good.
fn spring_settle_time(response: f64, damping: f64) -> f64 {
    let w0 = std::f64::consts::TAU / response;
    if damping < 1.0 {
        // The oscillation stays inside its decaying envelope.
        let wd = w0 * (1.0 - damping * damping).sqrt();
        let amplitude = (1.0 + (damping * w0 / wd).powi(2)).sqrt();
        return ((amplitude / SETTLED).ln() / (damping * w0)).min(MAX_SECONDS);
    }
    // Critically and over-damped springs approach monotonically.
    let step = 1.0 / 240.0;
    let mut t = 0.0;
    while t < MAX_SECONDS && 1.0 - spring(response, damping, t) > SETTLED {
        t += step;
    }
    t
}

pub fn lerp(a: f64, b: f64, p: f64) -> f64 {
    a + (b - a) * p
}

pub fn lerp_frame(a: Frame, b: Frame, p: f64) -> Frame {
    let l = |x: f32, y: f32| lerp(x as f64, y as f64, p) as f32;
    Frame {
        x: l(a.x, b.x),
        y: l(a.y, b.y),
        width: l(a.width, b.width).max(0.0),
        height: l(a.height, b.height).max(0.0),
    }
}

/// Reduce Motion: GNOME's "Reduce animation" switch is `gtk-enable-animations`.
/// Changes that move things apply at once; fades still animate.
pub fn reduce_motion() -> bool {
    gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_curves_match_css() {
        let ease = Anim::parse(&serde_json::json!({"kind": "timing", "duration": 200})).unwrap();
        assert_eq!(ease.total(), 0.2);
        assert_eq!(ease.progress(0.0), 0.0);
        assert_eq!(ease.progress(0.2), 1.0);
        // ease-in-out is symmetric around the middle.
        assert!((ease.progress(0.1) - 0.5).abs() < 1e-4);
        let linear =
            Anim::parse(&serde_json::json!({"duration": 100, "easing": "linear"})).unwrap();
        assert!((linear.progress(0.025) - 0.25).abs() < 1e-6);
        // CSS ease-in at x = 0.5 is about 0.3153.
        assert!((cubic_bezier([0.42, 0.0, 1.0, 1.0], 0.5) - 0.3153).abs() < 1e-3);
        assert!((cubic_bezier([0.0, 0.0, 0.58, 1.0], 0.5) - 0.6847).abs() < 1e-3);
    }

    #[test]
    fn delay_holds_the_start_value() {
        let a = Anim::parse(&serde_json::json!({"duration": 100, "delay": 50, "easing": "linear"}))
            .unwrap();
        assert_eq!(a.progress(0.04), 0.0);
        assert!((a.progress(0.1) - 0.5).abs() < 1e-6);
        assert!((a.total() - 0.15).abs() < 1e-9);
    }

    #[test]
    fn springs_overshoot_when_underdamped_and_settle() {
        let bouncy =
            Anim::parse(&serde_json::json!({"kind": "spring", "response": 0.5, "damping": 0.5}))
                .unwrap();
        let peak = (0..200)
            .map(|i| bouncy.progress(i as f64 / 200.0))
            .fold(0.0, f64::max);
        assert!(peak > 1.05, "underdamped spring overshoots, peak {peak}");
        assert_eq!(bouncy.progress(bouncy.total()), 1.0);
        for damping in [1.0, 2.0] {
            let s = Anim::parse(
                &serde_json::json!({"kind": "spring", "response": 0.35, "damping": damping}),
            )
            .unwrap();
            let mut last = 0.0;
            for i in 0..100 {
                let p = s.progress(i as f64 * s.total() / 100.0);
                assert!(
                    p >= last - 1e-9 && p <= 1.0 + 1e-9,
                    "no overshoot at damping {damping}"
                );
                last = p;
            }
            assert!(s.total() < 3.0);
        }
        // Defaults: the iOS 0.35 s / 0.825 spring settles in well under a second.
        let default = Anim::parse(&serde_json::json!({"kind": "spring"})).unwrap();
        assert!(
            default.total() > 0.2 && default.total() < 1.0,
            "{}",
            default.total()
        );
        assert!(spring(0.35, 0.825, 0.0).abs() < 1e-12);
    }

    #[test]
    fn spec_parses_frame_and_opacity_only() {
        let spec = AnimationSpec::parse(
            r#"{"opacity": {"kind": "timing", "duration": 300}, "transform": {}}"#,
        )
        .unwrap();
        assert!(spec.opacity.is_some() && spec.frame.is_none());
        assert_eq!(AnimationSpec::parse(r#"{"background": {}}"#), None);
        assert_eq!(AnimationSpec::parse("not json"), None);
        // Hostile values are clamped, never NaN or endless.
        let a = Anim::parse(&serde_json::json!({"duration": 1e12, "delay": -5})).unwrap();
        assert_eq!(a.delay, 0.0);
        assert_eq!(a.total(), MAX_SECONDS);
    }

    #[test]
    fn frames_interpolate() {
        let a = Frame {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 10.0,
        };
        let b = Frame {
            x: 50.0,
            y: 20.0,
            width: 0.0,
            height: 30.0,
        };
        let mid = lerp_frame(a, b, 0.5);
        assert_eq!(
            mid,
            Frame {
                x: 25.0,
                y: 10.0,
                width: 50.0,
                height: 20.0
            }
        );
        // A spring overshoot never makes a negative size.
        assert_eq!(lerp_frame(a, b, 1.2).width, 0.0);
    }
}
