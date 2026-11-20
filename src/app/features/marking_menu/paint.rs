//! Paint and AccessKit nodes of the drawn dial. The geometry is the hit test's: wedges are the
//! 45 degree sectors of `zone_at` out to `DIAL_RADIUS`. Original look, no image assets.

use super::layout::{DIAL_RADIUS, MeshAction, hit};
use super::point;
use azteccraft::marking_menu::{DEAD_ZONE, Interaction, MarkingMenu, Target, Zone};
use eframe::egui::{
    self, Align2, Color32, FontId, Id, LayerId, Order, Pos2, Rect, Role, Sense, Shape, Stroke,
    Vec2, WidgetInfo,
};

const LABEL_RADIUS: f32 = 74.0;
const LABEL_SIZE: Vec2 = Vec2::new(86.0, 22.0);

/// Screen-space angle of the centre of a zone, clockwise from north (up).
fn centre_angle(zone: Zone) -> f32 {
    -std::f32::consts::FRAC_PI_2 + zone.index() as f32 * std::f32::consts::FRAC_PI_4
}

fn wedge(origin: Pos2, zone: Zone) -> Vec<Pos2> {
    let half = std::f32::consts::FRAC_PI_8;
    let centre = centre_angle(zone);
    let mut points = vec![origin];
    for step in 0..=6 {
        let angle = centre - half + 2.0 * half * step as f32 / 6.0;
        points.push(origin + DIAL_RADIUS * Vec2::new(angle.cos(), angle.sin()));
    }
    points
}

fn label_rect(origin: Pos2, zone: Zone) -> Rect {
    let angle = centre_angle(zone);
    Rect::from_center_size(
        origin + LABEL_RADIUS * Vec2::new(angle.cos(), angle.sin()),
        LABEL_SIZE,
    )
}

pub(super) fn entry_id(target: Target) -> Id {
    match target {
        Target::Zone(zone) => Id::unique(("marking_menu_entry", zone.index())),
        Target::List(row) => Id::unique(("marking_menu_row", row)),
    }
}

fn accessible_label(zone: Zone, label: &str, checked: bool) -> String {
    let mut name = format!("{}: {label}", zone.label());
    if checked {
        name.push_str(", checked");
    }
    name
}

pub(super) fn paint(ui: &egui::Ui, interaction: &Interaction, menu: &MarkingMenu<MeshAction>) {
    let Some(origin) = interaction.origin() else {
        return;
    };
    let origin = Pos2::new(origin[0], origin[1]);
    let ctx = ui.ctx();
    let layer = LayerId::new(Order::Foreground, Id::unique("marking_menu_layer"));
    let painter = ctx.layer_painter(layer);
    let visuals = ctx.global_style().visuals.clone();
    let hovered = ui
        .input(|input| input.pointer.latest_pos())
        .and_then(|pos| hit(menu, point(origin), point(pos)));
    let focused = interaction.focused();
    painter.circle_filled(origin, DIAL_RADIUS, Color32::from_black_alpha(215));
    painter.circle_stroke(
        origin,
        DIAL_RADIUS,
        Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color),
    );
    painter.circle_filled(origin, DEAD_ZONE, Color32::from_white_alpha(18));
    let layer_ui = egui::Ui::new(
        ctx.clone(),
        Id::unique("marking_menu_ui"),
        egui::UiBuilder::new()
            .layer_id(layer)
            .max_rect(ctx.content_rect()),
    );
    for zone in Zone::ALL {
        let target = Target::Zone(zone);
        let Some(entry) = menu.entry(target) else {
            continue;
        };
        let enabled = menu.is_enabled(target);
        let active = hovered == Some(target) || focused == Some(target);
        if active {
            let fill = if enabled {
                visuals.selection.bg_fill
            } else {
                Color32::from_white_alpha(22)
            };
            painter.add(Shape::convex_polygon(
                wedge(origin, zone),
                fill,
                Stroke::NONE,
            ));
        }
        let rect = label_rect(origin, zone);
        let colour = if enabled {
            Color32::WHITE
        } else {
            Color32::from_gray(130)
        };
        if entry.checked {
            painter.circle_filled(rect.left_center() - Vec2::new(8.0, 0.0), 3.0, colour);
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &entry.label,
            FontId::proportional(13.0),
            colour,
        );
        // The entries are focusable, non-interactive widgets: the pointer and keys are handled by
        // `Interaction`, these only give screen readers a name, role, state and reason.
        let id = entry_id(target);
        let response = layer_ui.interact(rect, id, Sense::focusable_noninteractive());
        let name = accessible_label(zone, &entry.label, entry.checked);
        response
            .widget_info(|| WidgetInfo::selected(Role::MenuItem, enabled, entry.checked, &name));
        if let Some(reason) = entry.disabled_reason() {
            ctx.accesskit_node_builder(id, |node| {
                node.set_description(format!("Unavailable: {reason}"))
            });
        }
        if focused == Some(target) && ctx.memory(|memory| memory.focused()) != Some(id) {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
    }
}
