//! Where the plate goes on screen (#459 P7a): the quad's corners, its depth and its texture
//! coordinates. A pure function of the plane, the film back, the viewport aspect and the camera;
//! nothing here touches a renderer.
//!
//! Everything below that the Maya pages do not state is PROVISIONAL (#5): the fit rules read
//! from the Image Plane Attributes names, fitting the covered region (not the whole image), the
//! coverage origin counted from the image's lower left, that the plate follows the film back
//! (offset, overscan, scales and roll) exactly as the film gate does, and the depth rule.

use glam::Vec2;

use super::model::{Coverage, ImagePlane, PlateDisplayMode, PlateFit};
use crate::viewport::{Camera, FilmBack, Projection, film_gate_ndc};

/// The plate as a quad. Corner order is lower-left, lower-right, upper-right, upper-left, the
/// order of [`film_gate_ndc`], for `ndc` and `uv` alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateQuad {
    pub ndc: [Vec2; 4],
    /// Clip depth of the whole quad, wgpu 0 to 1 (0 at the near plane, 1 at the far plane).
    pub z_ndc: f32,
    /// Texture coordinates with the origin at the top-left texel and v down, the row order of
    /// a decoded frame (PROVISIONAL choice; the shader samples the frame as uploaded).
    pub uv: [Vec2; 4],
}

/// The quad for `plane` drawn at its depth. The plate is sized from the pixel counts of the
/// covered region only: an image's own pixel aspect (anamorphic footage) is not applied, and
/// Maya's squeezeCorrection is out of scope (PROVISIONAL; a test pins this, so changing it must
/// be deliberate).
///
/// The quad, or `None` when nothing should be drawn: display
/// mode None, an orthographic camera (Maya's top, front and side cameras carry no plane,
/// PROVISIONAL), a depth outside the camera's near..far range (the hardware would clip it), or
/// any input that is not a finite, positive size. Never panics.
pub fn plate_quad(
    plane: &ImagePlane,
    image_size: (u32, u32),
    film: &FilmBack,
    aspect: f32,
    camera: &Camera,
) -> Option<PlateQuad> {
    if plane.display_mode == PlateDisplayMode::None
        || camera.projection() == Projection::Orthographic
    {
        return None;
    }
    let z_ndc = depth_to_z(plane.depth, camera.near(), camera.far())?;
    let (image_w, image_h) = image_size;
    if image_w == 0 || image_h == 0 || !aspect.is_finite() || aspect <= 0.0 {
        return None;
    }
    let region = plane.coverage.unwrap_or(Coverage {
        origin_x: 0,
        origin_y: 0,
        width: image_w,
        height: image_h,
    });
    let right = u64::from(region.origin_x) + u64::from(region.width);
    let top = u64::from(region.origin_y) + u64::from(region.height);
    if region.width == 0
        || region.height == 0
        || right > u64::from(image_w)
        || top > u64::from(image_h)
    {
        return None;
    }

    // The film gate in film space: width hfa * lsr, height vfa (Addendum for #459).
    let gate_w = film.aperture_in[0] * film.lens_squeeze;
    let gate_h = film.aperture_in[1];
    if !(gate_w.is_finite() && gate_h.is_finite() && gate_w > 0.0 && gate_h > 0.0) {
        return None;
    }
    let (raw_x, raw_y) = fit_ratios(
        plane.fit,
        f64::from(region.width),
        f64::from(region.height),
        gate_w,
        gate_h,
    );

    // Fill, Horizontal and Vertical can scale the plate past the gate (a ratio above 1). The Maya
    // node page crops that excess (PROVISIONAL reading: Fill crops the overflowing axis,
    // Horizontal the top and bottom, Vertical the left and right), so the quad stays inside the
    // gate and the texture shows only the part that fits, centred on the covered region.
    let (sx, sy) = (raw_x.min(1.0), raw_y.min(1.0));
    let (keep_x, keep_y) = (sx / raw_x, sy / raw_y);

    // The film-to-NDC map is affine, so the plate is C + a*sx*U + b*sy*V from the gate corners.
    let gate = film_gate_ndc(film, aspect);
    let centre = (gate[0] + gate[1] + gate[2] + gate[3]) * 0.25;
    let u = (gate[1] - gate[0]) * 0.5;
    let v = (gate[3] - gate[0]) * 0.5;
    let (sx, sy) = (sx as f32, sy as f32);
    let ndc = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(a, b): (f32, f32)| centre + u * (a * sx) + v * (b * sy));
    if ndc.iter().any(|c| !c.is_finite()) {
        return None;
    }

    // The region's lower-left is (origin_x, origin_y) counted from the image's lower left.
    let (w, h) = (f64::from(image_w), f64::from(image_h));
    let u0 = f64::from(region.origin_x) / w;
    let u1 = f64::from(region.origin_x + region.width) / w;
    let v_bottom = (h - f64::from(region.origin_y)) / h;
    let v_top = (h - f64::from(region.origin_y) - f64::from(region.height)) / h;
    let (cu, half_u) = ((u0 + u1) * 0.5, (u1 - u0) * 0.5 * keep_x);
    let (cv, half_v) = ((v_bottom + v_top) * 0.5, (v_bottom - v_top) * 0.5 * keep_y);
    let (u_lo, u_hi, v_lo, v_hi) = (cu - half_u, cu + half_u, cv + half_v, cv - half_v);
    let uv = [(u_lo, v_lo), (u_hi, v_lo), (u_hi, v_hi), (u_lo, v_hi)]
        .map(|(u, v)| Vec2::new(u as f32, v as f32));
    Some(PlateQuad { ndc, z_ndc, uv })
}

/// The plate's size over the gate's, per axis, in film space. Best keeps the region's aspect
/// inside the gate, Fill covers the gate, Horizontal matches the width, Vertical the height and
/// To Size stretches (the Maya Image Plane Attributes page names the five modes).
fn fit_ratios(fit: PlateFit, region_w: f64, region_h: f64, gate_w: f64, gate_h: f64) -> (f64, f64) {
    let by_width = gate_w / region_w;
    let by_height = gate_h / region_h;
    let scale = match fit {
        PlateFit::Best => by_width.min(by_height),
        PlateFit::Fill => by_width.max(by_height),
        PlateFit::Horizontal => by_width,
        PlateFit::Vertical => by_height,
        PlateFit::ToSize => return (1.0, 1.0),
    };
    (region_w * scale / gate_w, region_h * scale / gate_h)
}

/// wgpu clip depth of a point `depth` in front of the camera: z = f (d - n) / (d (f - n)), 0 at
/// the near plane and 1 at the far plane. `None` outside near..far (PROVISIONAL: the Maya pages
/// say nothing about occlusion). Depth and the clip planes are both in centimetres.
fn depth_to_z(depth: f32, near: f32, far: f32) -> Option<f32> {
    let (d, n, f) = (f64::from(depth), f64::from(near), f64::from(far));
    let valid = d.is_finite() && n.is_finite() && f.is_finite() && 0.0 < n && n < f;
    if !valid || d < n || d > f {
        return None;
    }
    Some((f * (d - n) / (d * (f - n))) as f32)
}
