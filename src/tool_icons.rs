use azteccraft::ui_icons::{self, UiIcon};
use eframe::egui::{self, Response, Role, Ui, WidgetInfo};

const SHELF_ICON: f32 = 20.0;
const TOOL_ICON: f32 = 24.0;

pub fn button(ui: &mut Ui, label: &str, icon: UiIcon, enabled: bool) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let id = egui::IdSalt::new(("tool_icon", label));
        let atom = egui::Atom::custom(id, egui::vec2(SHELF_ICON, SHELF_ICON));
        let output = egui::Button::new((atom, label))
            .gap(6.0)
            .wrap_mode(egui::TextWrapMode::Extend)
            .atom_ui(ui);
        if let Some(rect) = output.rect(id) {
            ui_icons::paint(&ui.painter_at(rect), rect, icon);
        }
        output.response
    })
    .inner
}

/// Icon-only button. `hover` is the tooltip: shown on hover when enabled and as the disabled
/// tooltip otherwise. `label` is the AccessKit name.
pub fn icon_button(
    ui: &mut Ui,
    label: &str,
    icon: UiIcon,
    enabled: bool,
    selected: bool,
    hover: &str,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let id = egui::IdSalt::new(("tool_icon_only", label));
        let atom = egui::Atom::custom(id, egui::vec2(TOOL_ICON, TOOL_ICON));
        let output = egui::Button::new(atom).selected(selected).atom_ui(ui);
        if let Some(rect) = output.rect(id) {
            ui_icons::paint(&ui.painter_at(rect), rect, icon);
        }
        let response = output.response;
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, label));
        if enabled {
            response.on_hover_text(hover)
        } else {
            response.on_disabled_hover_text(hover)
        }
    })
    .inner
}

/// Width of an icon-only column (icon plus button and panel margins), measured from the live style.
pub fn toolbox_width(ui: &Ui) -> f32 {
    let style = ui.style();
    let button_margin = style.spacing.button_padding.x * 2.0
        + style.visuals.widgets.noninteractive.bg_stroke.width * 2.0
        + style.visuals.widgets.noninteractive.expansion * 2.0;
    let panel = egui::Frame::side_top_panel(ui.style());
    (TOOL_ICON
        + button_margin
        + panel.total_margin().sum().x
        + ui.visuals().widgets.noninteractive.bg_stroke.width)
        .ceil()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, Rect};

    #[test]
    fn toolbox_buttons_fit_at_minimum_window_and_enlarged_text() {
        for font_size in [14.0, 18.0] {
            let ctx = egui::Context::default();
            let mut style = egui::Style {
                override_font_id: Some(egui::FontId::proportional(font_size)),
                ..Default::default()
            };
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            ctx.set_style_of(egui::Theme::Dark, style);
            ctx.set_theme(egui::Theme::Dark);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0))),
                    ..Default::default()
                },
                |ui| {
                    egui::Panel::left("tools")
                        .exact_size(toolbox_width(ui))
                        .resizable(false)
                        .show(ui, |ui| {
                            let available = ui.max_rect();
                            let movement =
                                icon_button(ui, "Move", UiIcon::Move, false, false, "Move");
                            let rotation =
                                icon_button(ui, "Rotate", UiIcon::Rotate, false, false, "Rotate");
                            assert_eq!(movement.rect.height(), rotation.rect.height());
                            for r in [&movement, &rotation] {
                                assert!(
                                    r.rect.right() <= available.right(),
                                    "{font_size}: {:?} outside {available:?}",
                                    r.rect
                                );
                            }
                            assert!(!rotation.enabled());
                        });
                },
            );
            output.textures_delta.clear();
        }
    }
}
