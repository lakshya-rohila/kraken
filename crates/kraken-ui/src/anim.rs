//! Animation helpers: easing curves and time → progress.

use std::time::Instant;

/// Fast start, gentle stop.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Overshoots slightly past 1 before settling: a "pop".
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Linear progress 0..=1 of an animation lasting `ms` at `speed`
/// (speed 2 = twice as fast; 0 = animations off, always finished).
pub fn progress(start: Instant, ms: f32, speed: f32) -> f32 {
    if speed <= 0.0 {
        return 1.0;
    }
    (start.elapsed().as_secs_f32() * 1000.0 * speed / ms).clamp(0.0, 1.0)
}

/// Seconds since `start`, scaled by speed.
pub fn scaled_secs(start: Instant, speed: f32) -> f32 {
    start.elapsed().as_secs_f32() * speed.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hit_endpoints() {
        for f in [ease_out_cubic, ease_out_back] {
            assert!(f(0.0).abs() < 1e-5);
            assert!((f(1.0) - 1.0).abs() < 1e-5);
        }
        // The "back" curve overshoots on the way.
        assert!((0..100).map(|i| ease_out_back(i as f32 / 100.0)).any(|v| v > 1.0));
    }
}
