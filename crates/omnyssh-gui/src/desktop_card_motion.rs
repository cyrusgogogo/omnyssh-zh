//! Horizontal motion in physical pixels, including negative monitor coordinates.

pub const DURATION_MS: u64 = 280;

pub fn offscreen_right(monitors: impl IntoIterator<Item = (i32, u32)>, window_left: i32) -> i32 {
    monitors
        .into_iter()
        .map(|(left, width)| i64::from(left) + i64::from(width))
        .max()
        .unwrap_or(i64::from(window_left))
        .max(i64::from(window_left))
        .saturating_add(32)
        .min(i64::from(i32::MAX)) as i32
}

pub fn position_at(from: i32, to: i32, progress: f64) -> i32 {
    let t = progress.clamp(0.0, 1.0);
    // Smooth acceleration and deceleration in both directions.
    let eased = t * t * (3.0 - 2.0 * t);
    (f64::from(from) + (f64::from(to) - f64::from(from)) * eased).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flies_past_all_monitors_including_negative_coordinates() {
        assert_eq!(
            offscreen_right([(-1920, 1920), (0, 2560), (2560, 1920)], -1500),
            4512
        );
        assert_eq!(offscreen_right([(-1920, 1920)], -1200), 32);
        assert_eq!(offscreen_right([(0, 3840)], 3200), 3872);
    }

    #[test]
    fn motion_is_monotonic_and_reverses_back_to_the_exact_origin() {
        let (origin, outside) = (-1200, 4512);
        let mut previous = origin;
        for frame in 0..=100 {
            let t = f64::from(frame) / 100.0;
            let x = position_at(origin, outside, t);
            assert!(x >= previous && x <= outside);
            assert!((x - position_at(outside, origin, 1.0 - t)).abs() <= 1);
            previous = x;
        }
        assert_eq!(position_at(origin, outside, 1.0), outside);
        assert_eq!(position_at(outside, origin, 1.0), origin);
    }

    #[test]
    fn bounds_unusual_monitor_geometry_and_elapsed_time() {
        assert_eq!(offscreen_right([], 300), 332);
        assert_eq!(offscreen_right([(i32::MAX - 10, u32::MAX)], 0), i32::MAX);
        assert_eq!(position_at(100, 200, -1.0), 100);
        assert_eq!(position_at(200, 100, 2.0), 100);
    }
}
