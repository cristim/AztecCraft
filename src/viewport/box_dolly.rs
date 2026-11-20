//! Target-plane rectangle fitting; provisional until compared with Maya.
use super::{Camera, InvalidCameraInput, NdcRect, Projection};

impl Camera {
    /// Dolly into a positive-area viewport rectangle. Keep orientation and lens; center the
    /// rectangle on the target plane, scaling distance (orthographic width) by its larger extent.
    /// ponytail: target-plane fit, not depth picking; replace after pinned Maya comparison.
    pub fn box_dolly(&mut self, rect: NdcRect, aspect: f32) -> Result<(), InvalidCameraInput> {
        let size = rect.max() - rect.min();
        if !aspect.is_finite() || aspect <= 0.0 || size.min_element() <= 0.0 {
            return Err(InvalidCameraInput);
        }
        self.projection_matrix(aspect)?;
        let center = (rect.min() + rect.max()) * 0.5;
        let lens = if self.projection == Projection::Orthographic {
            self.lens.for_orthographic()
        } else {
            self.lens
        };
        let half_height = self.distance * lens.tan_half_y(aspect);
        let mut next = self.clone();
        next.pan(center * glam::Vec2::new(half_height * aspect, half_height))?;
        next.dolly(size.max_element() * 0.5)?;
        next.validate()?;
        next.projection_matrix(aspect)?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::OrthographicView;
    use glam::{Vec2, Vec3};

    #[test]
    fn rectangle_center_is_centered_and_largest_axis_fills_in_both_projections() {
        for ortho in [false, true] {
            for aspect in [0.5, 1.0, 2.0] {
                let mut camera = Camera::default();
                if ortho {
                    camera.show_orthographic(OrthographicView::Front).unwrap();
                }
                let old = camera.clone();
                let rect = NdcRect::new(Vec2::new(0.0, -0.25), Vec2::new(1.0, 0.25)).unwrap();
                let half_height = old.distance * old.lens.tan_half_y(aspect);
                let point = old.target
                    + old.view_matrix().inverse().transform_vector3(Vec3::new(
                        half_height * aspect * 0.5,
                        0.0,
                        0.0,
                    ));
                camera.box_dolly(rect, aspect).unwrap();
                assert!(
                    (camera.project(point, aspect).unwrap().unwrap().truncate()).length() < 1e-4
                );
                assert_eq!(camera.distance, old.distance * 0.5);
                assert_eq!(
                    (
                        camera.yaw,
                        camera.pitch,
                        camera.lens,
                        camera.near,
                        camera.far,
                        camera.projection
                    ),
                    (
                        old.yaw,
                        old.pitch,
                        old.lens,
                        old.near,
                        old.far,
                        old.projection
                    )
                );
                let full = NdcRect::new(Vec2::NEG_ONE, Vec2::ONE).unwrap();
                let before = camera.clone();
                camera.box_dolly(full, aspect).unwrap();
                assert_eq!(camera, before);
            }
        }
    }

    #[test]
    fn empty_invalid_aspect_and_unrepresentable_fit_preserve_the_camera() {
        let rect = NdcRect::new(Vec2::ZERO, Vec2::ONE).unwrap();
        for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            let mut camera = Camera::default();
            let before = camera.clone();
            assert!(camera.box_dolly(rect, aspect).is_err());
            assert_eq!(camera, before);
        }
        for end in [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::splat(f32::MIN_POSITIVE)] {
            let mut camera = Camera::default();
            let before = camera.clone();
            assert!(
                camera
                    .box_dolly(NdcRect::new(Vec2::ZERO, end).unwrap(), 1.0)
                    .is_err()
            );
            assert_eq!(camera, before);
        }
    }

    #[test]
    fn orthographic_box_dolly_uses_the_orthographic_lens_under_a_film_back() {
        use crate::viewport::{FilmBack, Lens};
        let aspect = 2.0;
        let mut camera = Camera::default();
        camera.show_orthographic(OrthographicView::Front).unwrap();
        camera.lens = Lens::Film(FilmBack {
            focal_mm: 70.0,
            ..FilmBack::DEFAULT
        });
        let old = camera.clone();
        let rect = NdcRect::new(Vec2::new(0.0, -0.25), Vec2::new(1.0, 0.25)).unwrap();
        let half_height = old.distance * old.lens.for_orthographic().tan_half_y(aspect);
        let point = old.target
            + old.view_matrix().inverse().transform_vector3(Vec3::new(
                half_height * aspect * 0.5,
                0.0,
                0.0,
            ));
        camera.box_dolly(rect, aspect).unwrap();
        assert!((camera.project(point, aspect).unwrap().unwrap().truncate()).length() < 1e-4);
    }
}
