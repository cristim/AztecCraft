//! One published table of the resource limits that admit or refuse scene work.
//!
//! Each row reads the constant that enforces it; nothing is copied. Maya documents no scene
//! limits, so every value is PROVISIONAL application policy (#441). Selection rule: a limit
//! appears when exceeding it refuses an operation at a top-level cap (input size, parser and
//! admission meters, instance paths, primitive size, view and recovery budgets). Narrower caps
//! inside one decoder or the curve view, and the undo queue (which drops old steps and refuses
//! nothing), stay with their owners. Refusals are the
//! owner's typed error (`NativeError::ResourceLimit`, the ASCII or binary resource-limit kind,
//! `RecoveryError::TooLarge`, `PrimitiveOptionsError::TooManyFaces`, `ViewError::ResourceLimit`),
//! never an allocation failure. Boundary tests live with each owner.

use serde::Serialize;

use crate::{
    document::MAX_DOCUMENT_BYTES,
    maya_ascii::{self, lex},
    maya_binary::container,
    native_scene::{
        MAX_PATH_IDENTITIES, MAX_TRANSFORM_DEPTH, MAX_TRANSFORM_PATHS, admission,
        binary::MAX_ADMISSION_WORK,
    },
    primitive_options::{MAX_DIVISIONS, MAX_FACES},
    recovery::MAX_RECOVERY_BYTES,
    view_scene::VIEW_BUDGET,
};

/// What a limit counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitUnit {
    Bytes,
    Count,
    /// Metered parse or association steps, not time.
    WorkSteps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SceneLimit {
    pub name: &'static str,
    pub value: u64,
    pub unit: LimitUnit,
    /// The constant that enforces the value.
    pub enforced_by: &'static str,
}

const fn limit(
    name: &'static str,
    value: usize,
    unit: LimitUnit,
    enforced_by: &'static str,
) -> SceneLimit {
    SceneLimit {
        name,
        value: value as u64,
        unit,
        enforced_by,
    }
}

use LimitUnit::{Bytes, Count, WorkSteps};

static LIMITS: [SceneLimit; 17] = [
    limit(
        "json_document_bytes",
        MAX_DOCUMENT_BYTES,
        Bytes,
        "document::MAX_DOCUMENT_BYTES",
    ),
    limit(
        "maya_ascii_bytes",
        maya_ascii::MAX_BYTES,
        Bytes,
        "maya_ascii::MAX_BYTES",
    ),
    limit(
        "maya_ascii_statements",
        lex::MAX_STATEMENTS,
        Count,
        "maya_ascii::lex::MAX_STATEMENTS",
    ),
    limit(
        "maya_ascii_tokens",
        lex::MAX_TOKENS,
        Count,
        "maya_ascii::lex::MAX_TOKENS",
    ),
    limit(
        "maya_ascii_work",
        maya_ascii::MAX_WORK,
        WorkSteps,
        "maya_ascii::MAX_WORK",
    ),
    limit(
        "maya_binary_bytes",
        container::MAX_BYTES,
        Bytes,
        "maya_binary::container::MAX_BYTES",
    ),
    limit(
        "maya_binary_chunks",
        container::MAX_CHUNKS,
        Count,
        "maya_binary::container::MAX_CHUNKS",
    ),
    limit(
        "maya_binary_depth",
        container::MAX_DEPTH,
        Count,
        "maya_binary::container::MAX_DEPTH",
    ),
    limit(
        "maya_binary_admission_work",
        MAX_ADMISSION_WORK,
        WorkSteps,
        "native_scene::binary::admission::MAX_WORK",
    ),
    limit(
        "admission_accounted_bytes",
        admission::MAX_BYTES,
        Bytes,
        "native_scene::admission::MAX_BYTES",
    ),
    limit(
        "instance_paths",
        MAX_TRANSFORM_PATHS,
        Count,
        "native_scene::MAX_TRANSFORM_PATHS",
    ),
    limit(
        "instance_depth",
        MAX_TRANSFORM_DEPTH,
        Count,
        "native_scene::MAX_TRANSFORM_DEPTH",
    ),
    limit(
        "instance_path_identities",
        MAX_PATH_IDENTITIES,
        Count,
        "native_scene::MAX_PATH_IDENTITIES",
    ),
    limit(
        "primitive_faces",
        MAX_FACES as usize,
        Count,
        "primitive_options::MAX_FACES",
    ),
    limit(
        "primitive_divisions",
        MAX_DIVISIONS as usize,
        Count,
        "primitive_options::MAX_DIVISIONS",
    ),
    limit("view_bytes", VIEW_BUDGET, Bytes, "view_scene::VIEW_BUDGET"),
    limit(
        "recovery_payload_bytes",
        MAX_RECOVERY_BYTES,
        Bytes,
        "recovery::MAX_RECOVERY_BYTES",
    ),
];

/// Every scene resource limit, in a stable order.
pub fn scene_limits() -> &'static [SceneLimit] {
    &LIMITS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn value(name: &str) -> u64 {
        scene_limits()
            .iter()
            .find(|limit| limit.name == name)
            .unwrap_or_else(|| panic!("missing limit {name}"))
            .value
    }

    #[test]
    fn rows_are_unique_nonzero_and_name_their_owner() {
        let mut names = HashSet::new();
        for limit in scene_limits() {
            assert!(names.insert(limit.name), "duplicate {}", limit.name);
            assert!(limit.value > 0, "{} is zero", limit.name);
            assert!(limit.enforced_by.contains("::"), "{}", limit.name);
        }
    }

    #[test]
    fn published_values_match_the_documented_table() {
        const MIB: u64 = 1024 * 1024;
        let expected: [(&str, u64, LimitUnit, &str); 17] = [
            (
                "json_document_bytes",
                16 * MIB,
                Bytes,
                "document::MAX_DOCUMENT_BYTES",
            ),
            ("maya_ascii_bytes", 16 * MIB, Bytes, "maya_ascii::MAX_BYTES"),
            (
                "maya_ascii_statements",
                1_000_000,
                Count,
                "maya_ascii::lex::MAX_STATEMENTS",
            ),
            (
                "maya_ascii_tokens",
                1_000_000,
                Count,
                "maya_ascii::lex::MAX_TOKENS",
            ),
            (
                "maya_ascii_work",
                50_000_000,
                WorkSteps,
                "maya_ascii::MAX_WORK",
            ),
            (
                "maya_binary_bytes",
                16 * MIB,
                Bytes,
                "maya_binary::container::MAX_BYTES",
            ),
            (
                "maya_binary_chunks",
                100_000,
                Count,
                "maya_binary::container::MAX_CHUNKS",
            ),
            (
                "maya_binary_depth",
                64,
                Count,
                "maya_binary::container::MAX_DEPTH",
            ),
            (
                "maya_binary_admission_work",
                50_000_000,
                WorkSteps,
                "native_scene::binary::admission::MAX_WORK",
            ),
            (
                "admission_accounted_bytes",
                128 * MIB,
                Bytes,
                "native_scene::admission::MAX_BYTES",
            ),
            (
                "instance_paths",
                4096,
                Count,
                "native_scene::MAX_TRANSFORM_PATHS",
            ),
            (
                "instance_depth",
                256,
                Count,
                "native_scene::MAX_TRANSFORM_DEPTH",
            ),
            (
                "instance_path_identities",
                65_536,
                Count,
                "native_scene::MAX_PATH_IDENTITIES",
            ),
            (
                "primitive_faces",
                20_000,
                Count,
                "primitive_options::MAX_FACES",
            ),
            (
                "primitive_divisions",
                128,
                Count,
                "primitive_options::MAX_DIVISIONS",
            ),
            ("view_bytes", 256 * MIB, Bytes, "view_scene::VIEW_BUDGET"),
            (
                "recovery_payload_bytes",
                512 * MIB,
                Bytes,
                "recovery::MAX_RECOVERY_BYTES",
            ),
        ];
        assert_eq!(scene_limits().len(), expected.len());
        for (name, want, unit, owner) in expected {
            let row = scene_limits().iter().find(|row| row.name == name).unwrap();
            assert_eq!(
                (row.value, row.unit, row.enforced_by),
                (want, unit, owner),
                "{name}"
            );
        }
    }

    fn flat_cubes(count: usize) -> crate::native_scene::NativeScene {
        use crate::scene::{Primitive, Scene};
        let mut legacy = Scene::default();
        for index in 0..count {
            legacy
                .create(Primitive::Cube, &format!("cube_{index:05}"))
                .unwrap();
        }
        crate::native_scene::NativeScene::from_legacy(legacy).unwrap()
    }

    #[test]
    fn the_published_instance_cap_is_the_real_boundary_of_a_generated_scene() {
        // Counts, not time: a flat scene of exactly the published cap lists every path and one
        // more refuses with the typed error.
        use crate::native_scene::NativeError;
        let cap = value("instance_paths") as usize;
        assert_eq!(flat_cubes(cap).transform_paths().unwrap().len(), cap);
        assert_eq!(
            flat_cubes(cap + 1).transform_paths().unwrap_err(),
            NativeError::ResourceLimit
        );
    }
}
