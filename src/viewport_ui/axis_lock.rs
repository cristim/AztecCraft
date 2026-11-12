use glam::Vec2;

/// PROVISIONAL (the Maya docs say only "Hold Shift to constrain"): points of accumulated drag
/// before Shift picks horizontal or vertical.
const AXIS_LOCK_POINTS: f32 = 4.0;

/// Shift-constrained tumble or track: locks a drag to its dominant screen axis.
#[derive(Clone, Copy, Default)]
pub(super) struct AxisLock {
    accumulated: Vec2,
    horizontal: Option<bool>,
}

impl AxisLock {
    pub(super) fn constrain(&mut self, delta: Vec2) -> Vec2 {
        if !delta.is_finite() {
            return Vec2::ZERO;
        }
        let moved = match self.horizontal {
            Some(_) => delta,
            None => {
                self.accumulated += delta;
                if self.accumulated.abs().max_element() < AXIS_LOCK_POINTS {
                    return Vec2::ZERO;
                }
                self.horizontal = Some(self.accumulated.x.abs() >= self.accumulated.y.abs());
                self.accumulated
            }
        };
        if self.horizontal == Some(true) {
            Vec2::new(moved.x, 0.0)
        } else {
            Vec2::new(0.0, moved.y)
        }
    }
}
