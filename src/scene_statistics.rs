//! Typed statistics of the live scene and its headroom against the limits registry (#441).
//!
//! Every count is read from the authoritative [`NativeScene`]; the instance-path figures reuse
//! [`NativeScene::transform_paths`] and the view bytes reuse the view builder's own preflight, so
//! nothing here is a second implementation of those rules. Headroom saturates at zero. Only caps
//! that bound the live scene appear in `caps`; input-size and parser caps do not. Statistics count
//! records, not instance paths. `view_bytes.used` is a LOWER BOUND of what `ViewScene::from_native`
//! charges: it is the builder's preflight figure and excludes curve tessellation, so a scene with
//! many curves can show headroom and still be refused. It is not a process memory measurement
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
}

/// How a cap's `used` figure was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageEstimate {
    Measured,
    /// The view builder refuses the scene (its byte figure overflowed); `used` is `u64::MAX` and
    /// headroom is zero.
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

impl CapUsage {
    fn new(name: &'static str, limit: u64, used: Option<u64>, estimate: UsageEstimate) -> Self {
        Self {
            name,
            limit,
            used,
            headroom: used.map(|used| limit.saturating_sub(used)),
            estimate,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneUsage {
    pub statistics: SceneStatistics,
    pub caps: Vec<CapUsage>,
}

impl NativeScene {
    /// Counts per record, not per instance path: a node or shape instanced under several parents
    /// counts once here, while `instance_paths` counts every path.
    pub fn statistics(&self) -> SceneStatistics {
        let shapes = || self.shapes().iter().map(|shape| shape.mesh());
        let count = |n: usize| n as u64;
        SceneStatistics {
            nodes: count(self.nodes().len()),
            shapes: count(self.shapes().len()),
            curves: count(self.curves().len()),
            joints: count(
                self.nodes()
                    .iter()
                    .filter(|node| self.joint(node.transform_id()).is_some())
                    .count(),
            ),
            keys: count(self.session_key_count()),
            vertices: shapes().map(|mesh| count(mesh.point_count())).sum(),
            edges: shapes().map(|mesh| count(mesh.edges().len())).sum(),
            faces: shapes().map(|mesh| count(mesh.faces().len())).sum(),
            triangles: shapes()
                .flat_map(|mesh| mesh.faces())
                .map(|face| {
                    let rings = face.outer().len()
                        + face.holes().iter().map(Vec::len).sum::<usize>()
                        + 2 * face.holes().len();
                    count(rings.saturating_sub(2))
                })
                .sum(),
            control_vertices: self
                .curves()
                .iter()
                .map(|curve| count(curve.control_vertices().len()))
                .sum(),
        }
    }

    /// Statistics plus the headroom of every registry cap that bounds this scene.
    pub fn usage(&self) -> SceneUsage {
        let paths = self.transform_paths().ok().map(|paths| {
            let identities: usize = paths.iter().map(|target| target.path.len()).sum();
            let depth = paths.iter().map(|target| target.path.len()).max();
            (
                paths.len() as u64,
                depth.unwrap_or(0) as u64,
                identities as u64,
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
                Some(CapUsage::new(limit.name, limit.value, used, estimate))
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

    fn legacy_cubes(count: usize) -> NativeScene {
        let mut legacy = crate::scene::Scene::default();
        for index in 0..count {
            legacy
                .create(Primitive::Cube, &format!("cube_{index:04}"))
                .unwrap();
        }
        NativeScene::from_legacy(legacy).unwrap()
    }

    #[test]
    fn exactly_at_the_path_cap_has_zero_headroom_and_one_over_is_unavailable() {
        let usage = legacy_cubes(MAX_TRANSFORM_PATHS).usage();
        let paths = cap(&usage, "instance_paths");
        assert_eq!(paths.used, Some(MAX_TRANSFORM_PATHS as u64));
        assert_eq!(paths.headroom, Some(0));
        assert_eq!(paths.estimate, UsageEstimate::Measured);

        // transform_paths refuses past the cap, so the path caps cannot be measured; the view
        // figure still is.
        let over = legacy_cubes(MAX_TRANSFORM_PATHS + 1).usage();
        for name in [
            "instance_paths",
            "instance_depth",
            "instance_path_identities",
        ] {
            let row = cap(&over, name);
            assert_eq!((row.used, row.headroom), (None, None), "{name}");
            assert_eq!(row.estimate, UsageEstimate::Unavailable, "{name}");
        }
        assert_eq!(cap(&over, "view_bytes").estimate, UsageEstimate::Measured);
    }

    #[test]
    fn headroom_saturates_at_zero_when_used_exceeds_the_limit() {
        let row = CapUsage::new("view_bytes", 10, Some(11), UsageEstimate::Measured);
        assert_eq!(row.headroom, Some(0));
        let row = CapUsage::new("view_bytes", 10, Some(u64::MAX), UsageEstimate::Saturated);
        assert_eq!(row.headroom, Some(0));
        let row = CapUsage::new("view_bytes", 10, None, UsageEstimate::Unavailable);
        assert_eq!(row.headroom, None);
    }

    #[test]
    fn curves_and_control_vertices_are_counted() {
        let source = "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film;\n\
            createNode transform -n \"c1\"; createNode nurbsCurve -n \"c1Shape\" -p \"c1\";\n\
            setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;";
        let scene = crate::native_scene::ascii::decode(source.as_bytes().to_vec().into()).unwrap();
        let stats = scene.statistics();
        assert_eq!((stats.curves, stats.control_vertices), (1, 4));
        assert_eq!((stats.shapes, stats.faces), (0, 0));
    }
}
