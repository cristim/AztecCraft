#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use glam::Vec2;

use super::{
    FILL_VERTICES, OUTLINE_VERTICES, PlateShading, PlateStyle, PlateUploadError, QUAD_BYTES,
    UNIFORM_BYTES, VERTEX_FLOATS, check_frame, quad_vertex_bytes, uniform_bytes,
};
use crate::image_plane::fit::PlateQuad;
use crate::image_plane::model::PlateDisplayMode;

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect()
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_ne_bytes(*c))
        .collect()
}

#[test]
fn every_display_mode_maps_to_its_style() {
    for (mode, style) in [
        (PlateDisplayMode::None, PlateStyle::Skip),
        (PlateDisplayMode::Outline, PlateStyle::Outline),
        (
            PlateDisplayMode::Rgb,
            PlateStyle::Textured(PlateShading::Rgb),
        ),
        (
            PlateDisplayMode::Rgba,
            PlateStyle::Textured(PlateShading::Rgba),
        ),
        (
            PlateDisplayMode::Luminance,
            PlateStyle::Textured(PlateShading::Luminance),
        ),
        (
            PlateDisplayMode::Alpha,
            PlateStyle::Textured(PlateShading::Alpha),
        ),
    ] {
        assert_eq!(PlateStyle::of(mode), style, "{mode:?}");
    }
}

#[test]
fn the_shader_mode_numbers_are_fixed() {
    // plate_shader.wgsl branches on these literals.
    assert_eq!(
        [
            PlateShading::Rgba,
            PlateShading::Rgb,
            PlateShading::Luminance,
            PlateShading::Alpha
        ]
        .map(|s| s as u32),
        [0, 1, 2, 3]
    );
}

#[test]
fn the_uniform_is_mode_output_srgb_gain_and_padding_in_16_bytes() {
    let bytes = uniform_bytes(PlateShading::Luminance, 0.5, true).unwrap();
    assert_eq!(bytes.len(), UNIFORM_BYTES);
    // 0.5f32 is 0x3f000000.
    assert_eq!(words(&bytes), [2, 1, 0x3f00_0000, 0]);
    let linear = uniform_bytes(PlateShading::Alpha, 1.0, false).unwrap();
    // 1.0f32 is 0x3f800000.
    assert_eq!(words(&linear), [3, 0, 0x3f80_0000, 0]);
}

#[test]
fn the_gain_is_clamped_to_zero_through_one_thousand_and_non_finite_is_refused() {
    // 1000.0f32 is 0x447a0000.
    assert_eq!(
        words(&uniform_bytes(PlateShading::Rgba, 2000.0, false).unwrap())[2],
        0x447a_0000
    );
    assert_eq!(
        words(&uniform_bytes(PlateShading::Rgba, 1000.0, false).unwrap())[2],
        0x447a_0000
    );
    assert_eq!(
        words(&uniform_bytes(PlateShading::Rgba, -3.0, false).unwrap())[2],
        0
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            uniform_bytes(PlateShading::Rgba, bad, false),
            Err(PlateUploadError::NonFinite)
        );
    }
}

#[test]
fn a_frame_must_have_pixels_fit_the_device_and_match_its_byte_count() {
    // 4 x 2 RGBA8 is 32 bytes.
    assert_eq!(check_frame(4, 2, 32, 8192), Ok(32));
    assert_eq!(check_frame(0, 2, 0, 8192), Err(PlateUploadError::ZeroSize));
    assert_eq!(check_frame(4, 0, 0, 8192), Err(PlateUploadError::ZeroSize));
    assert_eq!(
        check_frame(4, 2, 31, 8192),
        Err(PlateUploadError::ByteCountMismatch)
    );
    assert_eq!(
        check_frame(4, 2, 33, 8192),
        Err(PlateUploadError::ByteCountMismatch)
    );
    assert_eq!(
        check_frame(4, 2, 0, 8192),
        Err(PlateUploadError::ByteCountMismatch)
    );
    // The device limit: exactly at it is accepted, one past it is refused on either side.
    assert_eq!(check_frame(8192, 1, 8192 * 4, 8192), Ok(8192 * 4));
    assert_eq!(check_frame(1, 8192, 8192 * 4, 8192), Ok(8192 * 4));
    assert_eq!(
        check_frame(8193, 1, 8193 * 4, 8192),
        Err(PlateUploadError::OverDeviceLimit { limit: 8192 })
    );
    assert_eq!(
        check_frame(1, 8193, 8193 * 4, 8192),
        Err(PlateUploadError::OverDeviceLimit { limit: 8192 })
    );
    // A smaller device limit is respected.
    assert_eq!(
        check_frame(4096, 1, 4096 * 4, 2048),
        Err(PlateUploadError::OverDeviceLimit { limit: 2048 })
    );
}

#[test]
fn byte_count_arithmetic_cannot_overflow() {
    // width * height * 4 overflows usize on every target; it is a mismatch, never a panic.
    assert_eq!(
        check_frame(u32::MAX, u32::MAX, 4, u32::MAX),
        Err(PlateUploadError::ByteCountMismatch)
    );
    // A wrapping multiply would make this length look right.
    let wrapped = (u32::MAX as usize)
        .wrapping_mul(u32::MAX as usize)
        .wrapping_mul(4);
    assert_eq!(
        check_frame(u32::MAX, u32::MAX, wrapped, u32::MAX),
        Err(PlateUploadError::ByteCountMismatch)
    );
}

fn quad() -> PlateQuad {
    PlateQuad {
        ndc: [
            Vec2::new(-0.5, -0.25),
            Vec2::new(0.5, -0.25),
            Vec2::new(0.5, 0.25),
            Vec2::new(-0.5, 0.25),
        ],
        z_ndc: 0.75,
        uv: [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ],
    }
}

#[test]
fn the_quad_vertices_are_two_fill_triangles_then_four_edges() {
    let bytes = quad_vertex_bytes(&quad()).unwrap();
    assert_eq!(bytes.len(), QUAD_BYTES);
    assert_eq!(QUAD_BYTES, 14 * 6 * 4);
    let v = floats(&bytes);
    let vertex = |i: usize| &v[i * VERTEX_FLOATS..(i + 1) * VERTEX_FLOATS];
    // Corners: 0 lower-left, 1 lower-right, 2 upper-right, 3 upper-left, as (x, y, z, w, u, v).
    let corner = |i: usize| -> [f32; 6] {
        let q = quad();
        [q.ndc[i].x, q.ndc[i].y, 0.75, 1.0, q.uv[i].x, q.uv[i].y]
    };
    // Fill: (0, 1, 2) and (0, 2, 3).
    for (slot, index) in [0, 1, 2, 0, 2, 3].into_iter().enumerate() {
        assert_eq!(vertex(slot), corner(index), "fill {slot}");
    }
    // Outline: 0-1, 1-2, 2-3, 3-0.
    assert_eq!(FILL_VERTICES, 6);
    assert_eq!(OUTLINE_VERTICES, 8);
    for (slot, index) in [0, 1, 1, 2, 2, 3, 3, 0].into_iter().enumerate() {
        assert_eq!(vertex(6 + slot), corner(index), "edge {slot}");
    }
}

#[test]
fn a_non_finite_corner_depth_or_uv_is_refused() {
    let mut q = quad();
    q.ndc[2].x = f32::NAN;
    assert_eq!(quad_vertex_bytes(&q), Err(PlateUploadError::NonFinite));
    let mut q = quad();
    q.z_ndc = f32::INFINITY;
    assert_eq!(quad_vertex_bytes(&q), Err(PlateUploadError::NonFinite));
    let mut q = quad();
    q.uv[0].y = f32::NAN;
    assert_eq!(quad_vertex_bytes(&q), Err(PlateUploadError::NonFinite));
}

#[test]
fn error_text_carries_no_path_and_names_the_cause() {
    for (error, needle) in [
        (PlateUploadError::ZeroSize, "no pixels"),
        (PlateUploadError::OverDeviceLimit { limit: 2048 }, "2048"),
        (PlateUploadError::ByteCountMismatch, "width and height"),
        (PlateUploadError::NonFinite, "non-finite"),
        (PlateUploadError::GpuMemory, "memory"),
    ] {
        assert!(error.to_string().contains(needle), "{error}");
    }
}
