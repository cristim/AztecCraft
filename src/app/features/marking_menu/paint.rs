//! Paint and AccessKit nodes of the drawn dial. The geometry is the hit test's: wedges are the
//! 45 degree sectors of `zone_at` out to `DIAL_RADIUS`. Original look, no image assets.

use super::layout::{DIAL_RADIUS, LIST_ROW, MeshAction, hit_origin, list_rect, pick};
use super::point;
use azteccraft::marking_menu::{DEAD_ZONE, Entry, Interaction, MarkingMenu, Target, Zone};
use eframe::egui::{
    self, Align2, Color32, FontId, Id, LayerId, Order, Pos2, Rect, Role, Sense, Shape, Stroke,
    Vec2, WidgetInfo,
};

pub(super) const LABEL_RADIUS: f32 = 74.0;
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

pub(super) fn paint(
    ui: &egui::Ui,
    interaction: &Interaction,
    menu: &MarkingMenu<MeshAction>,
    viewport: Rect,
) {
    // Drawn where it is hit from: a held dial at the press (what is flicked is what is shown), a
    // sticky one pulled inside the viewport.
    let Some(gesture) = hit_origin(interaction, viewport) else {
        return;
    };
    let origin = Pos2::new(gesture[0], gesture[1]);
    let ctx = ui.ctx();
    let layer = LayerId::new(Order::Foreground, Id::unique("marking_menu_layer"));
    let painter = ctx.layer_painter(layer);
    let visuals = ctx.global_style().visuals.clone();
    let hovered = ui
        .input(|input| input.pointer.latest_pos())
        .and_then(|pos| pick(menu, interaction, viewport, point(pos)));
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
    // The dial is a menu node, with the entries as its children.
    let dial = Rect::from_center_size(origin, Vec2::splat(2.0 * DIAL_RADIUS));
    let menu_id = Id::unique("marking_menu_container");
    let container = layer_ui.interact(dial, menu_id, Sense::hover());
    container.widget_info(|| WidgetInfo::labeled(Role::Menu, true, "Marking menu"));
    let layer_ui = egui::Ui::new(
        ctx.clone(),
        Id::unique("marking_menu_entries"),
        egui::UiBuilder::new()
            .layer_id(layer)
            .max_rect(ctx.content_rect())
            .accessibility_parent(menu_id),
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
        let name = accessible_label(zone, &entry.label, entry.checked);
        expose(&layer_ui, target, rect, &name, entry, focused);
    }
    if interaction.is_sticky() {
        paint_list(
            ctx,
            &painter,
            &layer_ui,
            interaction,
            menu,
            viewport,
            gesture,
            hovered,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_list(
    ctx: &egui::Context,
    painter: &egui::Painter,
    layer_ui: &egui::Ui,
    interaction: &Interaction,
    menu: &MarkingMenu<MeshAction>,
    viewport: Rect,
    centre: [f32; 2],
    hovered: Option<Target>,
) {
    let visuals = ctx.global_style().visuals.clone();
    let panel = list_rect(centre, menu.list.len(), viewport);
    painter.rect_filled(panel, 4.0, Color32::from_black_alpha(215));
    let focused = interaction.focused();
    for (row, entry) in menu.list.iter().enumerate() {
        let target = Target::List(row);
        let enabled = menu.is_enabled(target);
        let rect = Rect::from_min_size(
            panel.min + Vec2::new(0.0, LIST_ROW * row as f32),
            Vec2::new(panel.width(), LIST_ROW),
        );
        if hovered == Some(target) || focused == Some(target) {
            let fill = if enabled {
                visuals.selection.bg_fill
            } else {
                Color32::from_white_alpha(22)
            };
            painter.rect_filled(rect, 2.0, fill);
        }
        let colour = if enabled {
            Color32::WHITE
        } else {
            Color32::from_gray(130)
        };
        painter.text(
            rect.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            &entry.label,
            FontId::proportional(13.0),
            colour,
        );
        expose(layer_ui, target, rect, &entry.label, entry, focused);
    }
}

/// The entries are focusable, non-interactive widgets: the pointer and keys are handled by
/// `Interaction`; these give screen readers a name, role, state and reason.
fn expose(
    layer_ui: &egui::Ui,
    target: Target,
    rect: Rect,
    name: &str,
    entry: &Entry<MeshAction>,
    focused: Option<Target>,
) {
    let ctx = layer_ui.ctx();
    let id = entry_id(target);
    let enabled = entry.disabled_reason().is_none();
    let response = layer_ui.interact(rect, id, Sense::focusable_noninteractive());
    response.widget_info(|| WidgetInfo::selected(Role::MenuItem, enabled, entry.checked, name));
    // The default action reaches `Interaction` through the pass, like Enter; egui itself handles
    // a screen reader's Focus request, and the pass mirrors it into the menu.
    ctx.accesskit_node_builder(id, |node| {
        node.add_action(egui::accesskit::Action::Click);
        if let Some(reason) = entry.disabled_reason() {
            node.set_description(format!("Unavailable: {reason}"));
        }
    });
    if focused == Some(target) && ctx.memory(|memory| memory.focused()) != Some(id) {
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
}
