//! Session UV layouts for the revolved primitives (#453 T8b), built from the same vertex and face
//! order as their mesh builders. All values lie in 0..1.
//!
//! What Maya's 2027 options page (GUID-45D2EAD4) states: the Sphere defaults to "Sawtooth at
//! poles", which the sphere layout follows, and the Cylinder and Cone default to "Normalize
//! Collectively and Preserve Aspect Ratio". PROVISIONAL deviation: the side strip (u 0..1, v
//! 0..0.5) and the 0.25-radius cap discs here ignore the aspect ratio. The page does not state the
//! u/v directions, the seam position or the cap placement, so those are PROVISIONAL too.
//!
//! - u runs with the right-handed azimuth about the axis, so faces are not mirrored: corner column
//!   `c` of `C` has `u = (C - c) / C`, and the last column reaches exactly 0.
//! - Sphere v is the latitude (1 at the top pole). Pole and cone-apex triangles are sawtooth: the
//!   pole corner takes the u of its column's midpoint, so no UV face is degenerate.
//! - Cylinder and cone sides fill `v` in 0..0.5 (top of the side strip at 0.5); each cap is a
//!   planar disc of radius 0.25 centred at (0.25, 0.75) for the top and (0.75, 0.75) for the bottom.
//! - Torus u follows the ring and v the section step (`s / S`, reaching exactly 1 at the wrap).
//! - The cube's six sides each fill their own cell of a 3 x 2 atlas (see `cube`); the plane uses
//!   the planar projection in `session_uv`.

use super::NativeMesh;
use crate::native_scene::{NativeError, UvSet};
use crate::primitive_options::{Axis, PrimitiveOptions};

type Corners = Vec<Vec<[f64; 2]>>;

const DISC_RADIUS: f64 = 0.25;
const TOP_DISC: [f64; 2] = [0.25, 0.75];
const BOTTOM_DISC: [f64; 2] = [0.75, 0.75];

/// The UVs for a Sphere, Cylinder, Cone or Torus built from `options`; any other primitive is
/// refused with a typed error.
pub(in crate::native_scene) fn layout(
    mesh: &NativeMesh,
    options: &PrimitiveOptions,
) -> Result<UvSet, NativeError> {
    let faces = match *options {
        PrimitiveOptions::Sphere {
            sub_axis,
            sub_height,
            ..
        } => sphere(sub_axis as usize, sub_height as usize),
        PrimitiveOptions::Cylinder {
            sub_axis,
            sub_height,
            sub_caps,
            axis,
            ..
        } => {
            let (columns, bands) = (sub_axis as usize, sub_height as usize);
            let mut faces = side_bands(columns, bands, false);
            let cap = cap_face_count(columns, sub_caps as usize);
            let start = faces.len();
            discs(mesh, axis, &mut faces, start, cap, TOP_DISC, -1.0)?;
            discs(mesh, axis, &mut faces, start + cap, cap, BOTTOM_DISC, 1.0)?;
            faces
        }
        PrimitiveOptions::Cone {
            sub_axis,
            sub_height,
            sub_caps,
            axis,
            ..
        } => {
            let (columns, bands) = (sub_axis as usize, sub_height as usize);
            let mut faces = side_bands(columns, bands, true);
            let start = faces.len();
            let cap = cap_face_count(columns, sub_caps as usize);
            discs(mesh, axis, &mut faces, start, cap, BOTTOM_DISC, 1.0)?;
            faces
        }
        PrimitiveOptions::Torus {
            sub_axis,
            sub_height,
            ..
        } => torus(sub_axis as usize, sub_height as usize),
        PrimitiveOptions::Cube {
            sub_width,
            sub_height,
            sub_depth,
            ..
        } => cube([sub_width, sub_height, sub_depth].map(|n| n as usize)),
        // The plane is projected in `session_uv`; the Platonic solids have no layout yet.
        PrimitiveOptions::Plane { .. } | PrimitiveOptions::PlatonicSolid { .. } => {
            return Err(NativeError::UnsupportedUvTopology);
        }
    };
    UvSet::from_corners(mesh, faces)
}

fn u(column: usize, columns: usize) -> f64 {
    (columns - column) as f64 / columns as f64
}

/// The u of the middle of a column, where a pole or apex corner sits.
fn mid_u(column: usize, columns: usize) -> f64 {
    (columns as f64 - column as f64 - 0.5) / columns as f64
}

fn sphere(columns: usize, rows: usize) -> Corners {
    let v = |ring: usize| 1.0 - ring as f64 / rows as f64;
    let mut faces = Vec::with_capacity(columns * rows);
    for c in 0..columns {
        faces.push(vec![
            [mid_u(c, columns), 1.0],
            [u(c + 1, columns), v(1)],
            [u(c, columns), v(1)],
        ]);
    }
    for ring in 1..rows - 1 {
        quads(&mut faces, columns, v(ring), v(ring + 1));
    }
    for c in 0..columns {
        faces.push(vec![
            [mid_u(c, columns), 0.0],
            [u(c, columns), v(rows - 1)],
            [u(c + 1, columns), v(rows - 1)],
        ]);
    }
    faces
}

fn quads(faces: &mut Corners, columns: usize, upper: f64, lower: f64) {
    for c in 0..columns {
        faces.push(vec![
            [u(c, columns), upper],
            [u(c + 1, columns), upper],
            [u(c + 1, columns), lower],
            [u(c, columns), lower],
        ]);
    }
}

/// A cylinder's (or, with `apex`, a cone's) side faces in builder order.
fn side_bands(columns: usize, bands: usize, apex: bool) -> Corners {
    let v = |ring: usize| 0.5 * (1.0 - ring as f64 / bands as f64);
    let mut faces = Vec::new();
    if apex {
        for c in 0..columns {
            faces.push(vec![
                [mid_u(c, columns), 0.5],
                [u(c + 1, columns), v(1)],
                [u(c, columns), v(1)],
            ]);
        }
    }
    for ring in usize::from(apex)..bands {
        quads(&mut faces, columns, v(ring), v(ring + 1));
    }
    faces
}

/// The six sides of a cube in builder order (back z0, front z1, left x0, right x1, bottom y0,
/// top y1), each normalised across its own cell of a 3 x 2 atlas. PROVISIONAL: the polyCube
/// command page lists `createUVs` 2 as "Normalize each face separately" and the default 3 as
/// "Normalize collectively"; the options page (GUID-45D2EAD4) says Maya maps the texture to each
/// face individually and names "Normalize Collectively and Preserve Aspect Ratio" as the default of
/// the normalization settings (not as a cube statement), so the two pages disagree on the default.
/// Neither page says where the faces land; stacking every side in one 0..1 square is only an
/// inference, and it would overlap, so this atlas is our own choice.
/// Within a cell the side's grid runs across the cell by its two in-plane coordinates, mirrored
/// per side so each UV face stays counter-clockwise.
fn cube([nx, ny, nz]: [usize; 3]) -> Corners {
    let mut faces = Vec::with_capacity(2 * (nx * ny + ny * nz + nx * nz));
    let mut cell = 0usize;
    // `corners` are the builder's four loop corners as (i, j) grid steps; `to_st` turns one into
    // the side's (s, t) fractions.
    let mut side = |outer: usize,
                    inner: usize,
                    corner: &dyn Fn(usize, usize) -> [[usize; 2]; 4],
                    to_st: &dyn Fn(usize, usize) -> [f64; 2]| {
        let (col, row) = ((cell % 3) as f64, (cell / 3) as f64);
        for a in 0..outer {
            for b in 0..inner {
                faces.push(
                    corner(a, b)
                        .iter()
                        .map(|&[x, y]| {
                            let [s, t] = to_st(x, y);
                            [(col + s) / 3.0, (row + t) / 2.0]
                        })
                        .collect(),
                );
            }
        }
        cell += 1;
    };
    let f = |n: usize, i: usize| i as f64 / n as f64;
    // Back and front: loops over (i, j) with j outer; corners are (i, j) steps.
    side(
        ny,
        nx,
        &|j, i| [[i, j], [i, j + 1], [i + 1, j + 1], [i + 1, j]],
        &|i, j| [1.0 - f(nx, i), f(ny, j)],
    );
    side(
        ny,
        nx,
        &|j, i| [[i, j], [i + 1, j], [i + 1, j + 1], [i, j + 1]],
        &|i, j| [f(nx, i), f(ny, j)],
    );
    // Left and right: k outer, j inner; corners are (k, j) steps.
    side(
        nz,
        ny,
        &|k, j| [[k, j], [k + 1, j], [k + 1, j + 1], [k, j + 1]],
        &|k, j| [f(nz, k), f(ny, j)],
    );
    side(
        nz,
        ny,
        &|k, j| [[k, j], [k, j + 1], [k + 1, j + 1], [k + 1, j]],
        &|k, j| [1.0 - f(nz, k), f(ny, j)],
    );
    // Bottom and top: k outer, i inner; corners are (i, k) steps.
    side(
        nz,
        nx,
        &|k, i| [[i, k], [i + 1, k], [i + 1, k + 1], [i, k + 1]],
        &|i, k| [f(nx, i), f(nz, k)],
    );
    side(
        nz,
        nx,
        &|k, i| [[i, k], [i, k + 1], [i + 1, k + 1], [i + 1, k]],
        &|i, k| [1.0 - f(nx, i), f(nz, k)],
    );
    faces
}

fn torus(rings: usize, steps: usize) -> Corners {
    let v = |step: usize| step as f64 / steps as f64;
    let mut faces = Vec::with_capacity(rings * steps);
    for ring in 0..rings {
        for step in 0..steps {
            faces.push(vec![
                [u(ring, rings), v(step)],
                [u(ring, rings), v(step + 1)],
                [u(ring + 1, rings), v(step + 1)],
                [u(ring + 1, rings), v(step)],
            ]);
        }
    }
    faces
}

/// Faces of one cap: a single n-gon at 0 subdivisions, else `columns * sub_caps`.
fn cap_face_count(columns: usize, sub_caps: usize) -> usize {
    if sub_caps == 0 { 1 } else { columns * sub_caps }
}

/// Appends the corner UVs of `count` cap faces starting at mesh face `start`: each vertex's local
/// (x, z) mapped onto a disc about `centre`; `flip` is -1 for a cap that faces +y (z points down
/// on screen) and +1 for one that faces -y, which keeps both counter-clockwise.
fn discs(
    mesh: &NativeMesh,
    axis: Axis,
    faces: &mut Corners,
    start: usize,
    count: usize,
    centre: [f64; 2],
    flip: f64,
) -> Result<(), NativeError> {
    let mut local = Vec::with_capacity(count);
    let mut widest = 0.0f64;
    for face in mesh
        .faces()
        .get(start..start + count)
        .ok_or(NativeError::InvalidSessionUvs)?
    {
        let mut corners = Vec::with_capacity(face.outer().len());
        for directed in face.outer() {
            let p = mesh.effective_point_cm(mesh.directed_start(*directed)?)?;
            let [x, _, z] = to_local(axis, p);
            widest = widest.max(x.hypot(z));
            corners.push([x, z]);
        }
        local.push(corners);
    }
    if !(widest.is_finite() && widest > 0.0) {
        return Err(NativeError::DegenerateUvProjection);
    }
    for corners in local {
        faces.push(
            corners
                .into_iter()
                .map(|[x, z]| {
                    [
                        centre[0] + DISC_RADIUS * x / widest,
                        centre[1] + DISC_RADIUS * flip * z / widest,
                    ]
                })
                .collect(),
        );
    }
    Ok(())
}

/// The inverse of the builders' `orient`: a point on `axis` back to the +Y frame they were built in.
fn to_local(axis: Axis, p: [f64; 3]) -> [f64; 3] {
    match axis {
        Axis::Y => p,
        Axis::X => [-p[1], p[0], p[2]],
        Axis::Z => [p[0], p[2], -p[1]],
    }
}

#[cfg(test)]
#[path = "primitive_uv_tests.rs"]
mod tests;
