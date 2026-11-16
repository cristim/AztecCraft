//! Focal curves share frame, time-unit, History and allocation limits with transform keys.
use super::*;
use crate::native_scene::SceneCamera;

impl NativeScene {
    pub(in crate::native_scene) fn insert_decoded_camera_key(
        &mut self,
        camera: SceneCamera,
        frame: i32,
        value: f64,
    ) -> Result<(), NativeError> {
        if self.session_keys.len() >= MAX_SESSION_KEYS {
            return Err(NativeError::KeyLimit);
        }
        self.session_keys.camera_curves[camera.index()].insert(frame, value);
        Ok(())
    }

    pub fn camera_focal_keys(&self, camera: SceneCamera) -> impl Iterator<Item = (i32, f64)> + '_ {
        self.session_keys.camera_curves[camera.index()]
            .iter()
            .map(|(frame, value)| (*frame, *value))
    }
    pub fn camera_focal_at(&self, camera: SceneCamera, frame: i32) -> Result<f64, NativeError> {
        self.check_time_unit()?;
        check_frame(frame)?;
        Ok(
            evaluate(&self.session_keys.camera_curves[camera.index()], frame)
                .unwrap_or(self.camera_optics(camera).focal_length_mm),
        )
    }
    pub fn set_camera_focal_key(
        &mut self,
        camera: SceneCamera,
        frame: i32,
        value: f64,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        self.check_time_unit()?;
        check_frame(frame)?;
        check_frame(current_frame)?;
        self.structural_edit()?;
        if !value.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if !(2.5..=100000.0).contains(&value) {
            return Err(NativeError::FocalLengthRange);
        }
        if !self.session_keys.camera_curves[camera.index()].contains_key(&frame)
            && self.session_keys.len() >= MAX_SESSION_KEYS
        {
            return Err(NativeError::KeyLimit);
        }
        let mut keys = self.session_keys.clone();
        keys.camera_curves[camera.index()].insert(frame, value);
        let writes = self.evaluated_writes_for(&keys, current_frame, None)?;
        self.session_keys = keys;
        self.apply_evaluated(writes);
        Ok(())
    }
    pub fn delete_camera_focal_keys(
        &mut self,
        camera: SceneCamera,
        frame: Option<i32>,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        self.check_time_unit()?;
        if let Some(frame) = frame {
            check_frame(frame)?;
        }
        check_frame(current_frame)?;
        self.structural_edit()?;
        let mut keys = self.session_keys.clone();
        match frame {
            Some(frame) => {
                keys.camera_curves[camera.index()].remove(&frame);
            }
            None => keys.camera_curves[camera.index()].clear(),
        }
        let writes = self.evaluated_writes_for(&keys, current_frame, None)?;
        self.session_keys = keys;
        self.apply_evaluated(writes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focal_keys_evaluate_exactly_interpolate_hold_and_refuse_invalid_without_mutation() {
        let mut scene = NativeScene::default();
        let camera = SceneCamera::Persp;
        scene
            .set_camera_focal_key(camera, 1, 35.123456789012345, 1)
            .unwrap();
        scene
            .set_camera_focal_key(camera, 11, 75.123_456_789_012_35, 1)
            .unwrap();
        assert_eq!(scene.session_key_count(), 2);
        for (frame, value) in [
            (-1, 35.123456789012345_f64),
            (1, 35.123456789012345),
            (6, 55.123456789012345),
            (11, 75.123_456_789_012_35),
            (100, 75.123_456_789_012_35),
        ] {
            let writes = scene.evaluated_writes(frame).unwrap();
            scene.apply_evaluated(writes);
            if frame == 6 {
                assert!((scene.camera_optics(camera).focal_length_mm - value).abs() < 1e-12);
            } else {
                assert_eq!(
                    scene.camera_optics(camera).focal_length_mm.to_bits(),
                    value.to_bits()
                );
            }
        }
        let baseline = scene.clone();
        for (frame, value) in [
            (MAX_FRAME + 1, 35.0),
            (1, f64::NAN),
            (1, 0.0),
            (1, 100001.0),
        ] {
            assert!(scene.set_camera_focal_key(camera, frame, value, 1).is_err());
            assert_eq!(scene, baseline);
        }
        let mut edited = scene.camera_optics(camera);
        edited.focal_length_mm = 50.0;
        assert!(scene.set_camera_optics(camera, edited).is_err());
        assert_eq!(scene, baseline);
        scene.delete_camera_focal_keys(camera, None, 1).unwrap();
        assert_eq!(scene.session_key_count(), 0);
        scene.set_camera_optics(camera, edited).unwrap();
    }
    #[test]
    fn camera_and_transform_keys_share_one_allocation_limit() {
        let mut scene = NativeScene::default();
        let node = scene.create(crate::scene::Primitive::Cube, "cube").unwrap();
        scene.session_keys.camera_curves[0] =
            (0..MAX_SESSION_KEYS as i32).map(|i| (i, 35.0)).collect();
        let baseline = scene.clone();
        assert_eq!(
            scene.set_keys(&[node], 1, [true; 9], 1),
            Err(NativeError::KeyLimit)
        );
        assert_eq!(scene, baseline);
        assert_eq!(
            scene.set_camera_focal_key(SceneCamera::Top, 1, 35.0, 1),
            Err(NativeError::KeyLimit)
        );
        assert_eq!(scene, baseline);
    }
}
