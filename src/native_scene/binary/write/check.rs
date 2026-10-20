//! The self-check: decode the bytes just built with the repo's own decoder and compare
//! them with the scene they came from, so a writer bug fails here instead of on disk.
//!
//! It proves only that AztecCraft reads back what AztecCraft wrote. It does not show that
//! Maya accepts the file. Not compared, because they differ by design: `NativeId`s, the
//! UUIDs (the decoder skips the 16 `CREA` identity bytes, so a raw-byte test covers
//! them), edge smoothness and the retained source.

use super::AuthoredBinaryRefusal;
use crate::native_scene::{NativeMeshShape, NativeScene, Points, authored, binary};
use std::sync::Arc;

/// Why the decoded scene differs from the one that was written.
fn mismatch<T>() -> Result<T, AuthoredBinaryRefusal> {
    Err(AuthoredBinaryRefusal::SelfCheckMismatch)
}

/// Name, point bit patterns, edge endpoint pairs and face loops, all in order. Edge
/// smoothness is not compared: the decoder leaves it unknown.
fn same_shape(before: &NativeMeshShape, after: &NativeMeshShape) -> bool {
    let (a, b) = (before.mesh(), after.mesh());
    let single = |points: &Points| matches!(points, Points::Float3(_));
    before.name() == after.name()
        && single(a.points())
        && a.points() == b.points()
        && !b.has_point_offsets()
        && a.edges().len() == b.edges().len()
        && a.edges()
            .iter()
            .zip(b.edges())
            .all(|(x, y)| x.endpoints() == y.endpoints())
        && a.faces().len() == b.faces().len()
        && a.faces()
            .iter()
            .zip(b.faces())
            .all(|(x, y)| x.outer() == y.outer() && x.holes().is_empty() && y.holes().is_empty())
}

pub(super) fn verify(scene: &NativeScene, bytes: &[u8]) -> Result<(), AuthoredBinaryRefusal> {
    let Ok(decoded) = binary::decode(Arc::from(bytes)) else {
        return mismatch();
    };
    if decoded.units() != scene.units()
        || decoded.nodes().len() != scene.nodes().len()
        || decoded.shapes().len() != scene.shapes().len()
        || decoded.dag_edges().len() != scene.dag_edges().len()
    {
        return mismatch();
    }
    for (before, after) in scene.nodes().iter().zip(decoded.nodes()) {
        if before.name() != after.name() {
            return mismatch();
        }
        let roots = decoded
            .dag_edges()
            .iter()
            .filter(|e| e.parent().is_none() && e.child() == after.transform_id())
            .count();
        let children = decoded
            .dag_edges()
            .iter()
            .filter(|e| e.parent() == Some(after.transform_id()))
            .count();
        if roots != 1 || children != usize::from(!scene.shapes().is_empty()) {
            return mismatch();
        }
        if !scene.shapes().is_empty() {
            let (Ok(a), Ok(b)) = (
                authored::shape_of(scene, before),
                authored::shape_of(&decoded, after),
            ) else {
                return mismatch();
            };
            if !same_shape(a, b) {
                return mismatch();
            }
        }
        let (Ok(a), Ok(b)) = (
            scene.internal_transform(before.id()),
            decoded.internal_transform(after.id()),
        ) else {
            return mismatch();
        };
        let words = |t: crate::native_scene::InternalTransform| {
            [
                t.translation_cm,
                t.rotation_radians,
                t.scale,
                t.rotate_pivot_cm,
                t.scale_pivot_cm,
            ]
            .map(|v| v.map(f64::to_bits))
        };
        if words(a) != words(b) {
            return mismatch();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
