use crate::primitive_options::Axis;

/// Rotates a point built around +Y onto `axis`. The maps are exact component permutations with
/// one sign flip (determinant +1), so winding is kept and no rounding is added. `+ 0.0` turns a
/// negated zero back into +0.0.
pub(super) fn orient(axis: Axis, [x, y, z]: [f32; 3]) -> [f32; 3] {
    match axis {
        Axis::Y => [x + 0.0, y + 0.0, z + 0.0],
        Axis::X => [y + 0.0, -x + 0.0, z + 0.0],
        Axis::Z => [x + 0.0, -z + 0.0, y + 0.0],
    }
}
