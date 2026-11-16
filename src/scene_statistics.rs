//! Typed statistics of the live scene and its headroom against the limits registry (#441).
//!
//! Every count is read from the authoritative [`NativeScene`]; the instance-path figures reuse
//! [`NativeScene::transform_paths`] and the view bytes reuse the view builder's own preflight, so
//! nothing here is a second implementation of those rules. Arithmetic is checked: a sum that would
//! overflow `u64` sets `saturated` and pins the value at `u64::MAX`, and headroom never goes below
//! zero. Only caps that bound the live scene appear in `caps`; input-size and parser caps do not.
//! The byte figure is the view builder's preflight estimate, not a process memory measurement
//! (slice 3). Definitions beyond that are PROVISIONAL.

use serde::Serialize;

use crate::{native_scene::NativeScene, scene_limits::scene_limits, view_scene::ViewScene};

/// Counts of what the scene holds. `triangles` is the fan triangulation of each face with its
/// holes (`ring vertices - 2 + 2 * holes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SceneStatistics {
    pub nodes: u64,
    pub shapes: u64,
    pub curves: u64,
    pub joints: u64,
    pub keys: u64,
    pub vertices: u64,
    pub edges: u64,
    pub faces: u64,
    pub triangles: u64,
    pub control_vertices: u64,
    /// True when any count above hit `u64::MAX` instead of overflowing.
    pub saturated: bool,
}

/// How a cap's `used` figure was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageEstimate {
    Measured,
    /// The figure overflowed; `used` is `u64::MAX` and headroom is zero.
    Saturated,
    /// The scene cannot be measured (for example the view refuses a face); `used` is null.
    Unavailable,
}

/// One registry cap that applies to the live scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CapUsage {
    pub name: &'static str,
    pub limit: u64,
    pub used: Option<u64>,
    /// `limit - used`, never negative; null when `used` is.
    pub headroom: Option<u64>,
    pub estimate: UsageEstimate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneUsage {
    pub statistics: SceneStatistics,
    pub caps: Vec<CapUsage>,
}

/// Checked running total that pins at `u64::MAX` and remembers it did.
#[derive(Default)]
struct Total {
    value: u64,
    saturated: bool,
}

impl Total {
    fn add(&mut self, amount: usize) {
        let amount = u64::try_from(amount).unwrap_or(u64::MAX);
        match self.value.checked_add(amount) {
            Some(sum) => self.value = sum,
            None => {
                self.value = u64::MAX;
                self.saturated = true;
            }
        }
    }
}

impl NativeScene {
    pub fn statistics(&self) -> SceneStatistics {
        let mut saturated = false;
        let mut sum = |build: &dyn Fn(&mut Total)| {
            let mut total = Total::default();
            build(&mut total);
            saturated |= total.saturated;
            total.value
        };
        let nodes = sum(&|t| t.add(self.nodes().len()));
        let shapes = sum(&|t| t.add(self.shapes().len()));
        let curves = sum(&|t| t.add(self.curves().len()));
        let joints = sum(&|t| {
            let joints = self
                .nodes()
                .iter()
                .filter(|node| self.joint(node.transform_id()).is_some());
            t.add(joints.count());
        });
        let keys = sum(&|t| t.add(self.session_key_count()));
        let vertices = sum(&|t| {
            self.shapes()
                .iter()
                .for_each(|shape| t.add(shape.mesh().point_count()));
        });
        let edges = sum(&|t| {
            self.shapes()
                .iter()
                .for_each(|shape| t.add(shape.mesh().edges().len()));
        });
        let faces = sum(&|t| {
            self.shapes()
                .iter()
                .for_each(|shape| t.add(shape.mesh().faces().len()));
        });
        let triangles = sum(&|t| {
            for face in self.shapes().iter().flat_map(|s| s.mesh().faces()) {
                let ring = face.outer().len()
                    + face.holes().iter().map(Vec::len).sum::<usize>()
                    + 2 * face.holes().len();
                t.add(ring.saturating_sub(2));
            }
        });
        let control_vertices = sum(&|t| {
            self.curves()
                .iter()
                .for_each(|curve| t.add(curve.control_vertices().len()));
        });
        SceneStatistics {
            nodes,
            shapes,
            curves,
            joints,
            keys,
            vertices,
            edges,
            faces,
            triangles,
            control_vertices,
            saturated,
        }
    }

    /// Statistics plus the headroom of every registry cap that bounds this scene.
    pub fn usage(&self) -> SceneUsage {
        let paths = self.transform_paths().ok().map(|paths| {
            let mut identities = Total::default();
            paths
                .iter()
                .for_each(|target| identities.add(target.path.len()));
            let depth = paths.iter().map(|target| target.path.len()).max();
            (
                paths.len() as u64,
                depth.unwrap_or(0) as u64,
                identities.value,
            )
        });
        let view = ViewScene::required_bytes(self);
        let caps = scene_limits()
            .iter()
            .filter_map(|limit| {
                let (used, estimate) = match limit.name {
                    "instance_paths" => measured(paths.map(|p| p.0)),
                    "instance_depth" => measured(paths.map(|p| p.1)),
                    "instance_path_identities" => measured(paths.map(|p| p.2)),
                    "view_bytes" => match &view {
                        Ok(bytes) => measured(Some(*bytes as u64)),
                        Err(crate::view_scene::ViewError::ResourceLimit) => {
                            (Some(u64::MAX), UsageEstimate::Saturated)
                        }
                        Err(_) => (None, UsageEstimate::Unavailable),
                    },
                    _ => return None,
                };
                Some(CapUsage {
                    name: limit.name,
                    limit: limit.value,
                    used,
                    headroom: used.map(|used| limit.value.saturating_sub(used)),
                    estimate,
                })
            })
            .collect();
        SceneUsage {
            statistics: self.statistics(),
            caps,
        }
    }
}

fn measured(used: Option<u64>) -> (Option<u64>, UsageEstimate) {
    match used {
        Some(_) => (used, UsageEstimate::Measured),
        None => (None, UsageEstimate::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        native_scene::{ALL_CHANNELS, JointRequest, MAX_TRANSFORM_PATHS},
        scene::Primitive,
    };

    fn cap<'a>(usage: &'a SceneUsage, name: &str) -> &'a CapUsage {
        usage.caps.iter().find(|c| c.name == name).unwrap()
    }

    fn cubes(count: usize) -> NativeScene {
        let mut scene = NativeScene::default();
        for index in 0..count {
            scene
                .create(Primitive::Cube, &format!("cube_{index:04}"))
                .unwrap();
        }
        scene
    }

    #[test]
    fn an_empty_scene_counts_zero_and_has_full_headroom() {
        let usage = NativeScene::default().usage();
        assert_eq!(usage.statistics, SceneStatistics::default());
        let names: Vec<_> = usage.caps.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            [
                "instance_paths",
                "instance_depth",
                "instance_path_identities",
                "view_bytes"
            ]
        );
        for cap in &usage.caps {
            assert_eq!(cap.estimate, UsageEstimate::Measured, "{}", cap.name);
            assert_eq!(cap.headroom.unwrap() + cap.used.unwrap(), cap.limit);
        }
        assert_eq!(cap(&usage, "instance_paths").used, Some(0));
    }

    #[test]
    fn primitives_report_geometry_and_instance_figures() {
        let usage = cubes(3).usage();
        let s = usage.statistics;
        assert_eq!(
            (s.nodes, s.shapes, s.curves, s.joints, s.keys),
            (3, 3, 0, 0, 0)
        );
        assert_eq!(
            (s.vertices, s.edges, s.faces, s.triangles),
            (24, 36, 18, 36)
        );
        assert!(!s.saturated);
        assert_eq!(cap(&usage, "instance_paths").used, Some(3));
        assert_eq!(cap(&usage, "instance_depth").used, Some(1));
        assert_eq!(cap(&usage, "instance_path_identities").used, Some(3));
        assert!(cap(&usage, "view_bytes").used.unwrap() > 0);
    }

    #[test]
    fn joints_and_keys_are_counted() {
        let mut scene = cubes(1);
        let root = scene
            .create_joint(&JointRequest {
                parent: None,
                position: [0.0; 3],
                relative: false,
                radius: 1.0,
                name: None,
            })
            .unwrap();
        scene
            .create_joint(&JointRequest {
                parent: Some(root.node),
                position: [1.0, 0.0, 0.0],
                relative: true,
                radius: 1.0,
                name: None,
            })
            .unwrap();
        let cube = scene.nodes()[0].id();
        scene.set_keys(&[cube], 1, ALL_CHANNELS, 1).unwrap();
        let usage = scene.usage();
        assert_eq!(usage.statistics.joints, 2);
        assert_eq!(usage.statistics.keys, scene.session_key_count() as u64);
        assert!(usage.statistics.keys > 0);
        assert_eq!(cap(&usage, "instance_depth").used, Some(2));
    }

    #[test]
    fn a_scene_at_the_path_cap_has_zero_headroom_and_never_negative() {
        let mut legacy = crate::scene::Scene::default();
        for index in 0..MAX_TRANSFORM_PATHS {
            legacy
                .create(Primitive::Cube, &format!("cube_{index:04}"))
                .unwrap();
        }
        let usage = NativeScene::from_legacy(legacy).unwrap().usage();
        let paths = cap(&usage, "instance_paths");
        assert_eq!(paths.used, Some(MAX_TRANSFORM_PATHS as u64));
        assert_eq!(paths.headroom, Some(0));
        assert_eq!(paths.estimate, UsageEstimate::Measured);
    }

    #[test]
    fn totals_saturate_instead_of_overflowing() {
        let mut total = Total::default();
        total.add(usize::MAX);
        total.add(usize::MAX);
        total.add(1);
        assert_eq!(total.value, u64::MAX);
        assert!(total.saturated);
    }
}
