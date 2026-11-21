//! egui drawing for the Rotate and Scale handles. Colors follow the Move axis palette and are
//! provisional until compared with the pinned Maya reference.

use super::{DisabledHandle, RotateGrip, RotateHandles, ScaleHandles};
use crate::{history::FreeRotateHandle, move_handles::ScreenSegment, native_scene::ScaleHandle};
use eframe::egui::{Color32, Painter, Pos2, Stroke, vec2};

const HOVER: Color32 = Color32::WHITE;
const ACTIVE: Color32 = Color32::from_rgb(255, 225, 80);
const DISABLED: Color32 = Color32::from_rgb(110, 110, 110);
const SPHERE: Color32 = Color32::from_rgb(150, 150, 150);
const VIEW_RING: Color32 = Color32::from_rgb(120, 200, 230);

pub fn axis_color(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(235, 75, 75),
        1 => Color32::from_rgb(90, 220, 105),
        _ => Color32::from_rgb(85, 150, 245),
    }
}

// `rect_min` is the viewport's top-left in points; handle pixels are physical.
struct Surface<'a> {
    painter: &'a Painter,
    rect_min: Pos2,
    pixels_per_point: f32,
}

impl Surface<'_> {
    fn segment(&self, segment: &ScreenSegment, width: f32, color: Color32) {
        let point = |pixel: glam::DVec2| {
            self.rect_min
                + vec2(
                    pixel.x as f32 / self.pixels_per_point,
                    pixel.y as f32 / self.pixels_per_point,
                )
        };
        self.painter.line_segment(
            [point(segment.start), point(segment.end)],
            Stroke::new(width / self.pixels_per_point, color),
        );
    }

    fn disabled(&self, handles: &[DisabledHandle]) {
        for handle in handles {
            for segment in handle.segments() {
                self.segment(segment, 1.0, DISABLED);
            }
        }
    }
}

fn state_color<T: PartialEq + Copy>(
    handle: T,
    base: Color32,
    hovered: Option<T>,
    active: Option<T>,
) -> Color32 {
    if active == Some(handle) {
        ACTIVE
    } else if hovered == Some(handle) && active.is_none() {
        HOVER
    } else {
        base
    }
}

pub fn paint_rotate(
    painter: &Painter,
    rect_min: Pos2,
    pixels_per_point: f32,
    handles: &RotateHandles,
    hovered: Option<RotateGrip>,
    active: Option<RotateGrip>,
) {
    let surface = Surface {
        painter,
        rect_min,
        pixels_per_point,
    };
    // The sphere outline grey and the view ring light blue (PROVISIONAL colours, #5).
    for circle in handles.free() {
        let base = match circle.handle {
            FreeRotateHandle::Trackball => SPHERE,
            FreeRotateHandle::View => VIEW_RING,
        };
        let color = state_color(RotateGrip::Free(circle.handle), base, hovered, active);
        for segment in circle.segments() {
            surface.segment(segment, 1.5, color);
        }
    }
    for (index, ring) in handles.rings().iter().enumerate() {
        let color = state_color(
            RotateGrip::Axis(ring.axis),
            axis_color(index),
            hovered,
            active,
        );
        for segment in ring.segments() {
            surface.segment(segment, 2.0, color);
        }
    }
}

pub fn paint_scale(
    painter: &Painter,
    rect_min: Pos2,
    pixels_per_point: f32,
    handles: &ScaleHandles,
    hovered: Option<ScaleHandle>,
    active: Option<ScaleHandle>,
) {
    let surface = Surface {
        painter,
        rect_min,
        pixels_per_point,
    };
    surface.disabled(handles.disabled());
    for (index, handle) in handles.axes().iter().enumerate() {
        let Some(geometry) = handle.geometry() else {
            continue;
        };
        let color = state_color(handle.handle, axis_color(index), hovered, active);
        for segment in geometry.segments() {
            surface.segment(segment, 2.0, color);
        }
    }
    let center = state_color(
        ScaleHandle::Uniform,
        Color32::from_rgb(220, 220, 220),
        hovered,
        active,
    );
    for segment in handles.center() {
        surface.segment(segment, 2.0, center);
    }
}
