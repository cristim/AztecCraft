//! Detection proofs: each rule fails on a synthetic control and passes on the fixed one.

use super::*;
use eframe::egui::{Atom, Button, IdSalt, Sense, WidgetInfo, accesskit, vec2};

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(400.0, 300.0),
        )),
        ..Default::default()
    }
}

/// The tree after two frames of `add`, with AccessKit enabled.
fn tree_of<R>(mut add: impl FnMut(&mut egui::Ui) -> R) -> TreeUpdate {
    let context = egui::Context::default();
    context.enable_accesskit();
    context
        .run_ui(input(), |ui| {
            add(ui);
        })
        .textures_delta
        .clear();
    let mut output = context.run_ui(input(), |ui| {
        add(ui);
    });
    output.textures_delta.clear();
    output
        .platform_output
        .accesskit_update
        .expect("accesskit enabled")
}

fn codes(update: &TreeUpdate) -> Vec<&'static str> {
    let mut codes: Vec<_> = violations("fixture", update, &[])
        .iter()
        .map(|v| v.rule.code())
        .collect();
    codes.sort_unstable();
    codes
}

fn custom_region(ui: &mut egui::Ui, named: bool) {
    let response = ui.allocate_response(vec2(20.0, 20.0), Sense::click());
    if named {
        response.widget_info(|| WidgetInfo::labeled(accesskit::Role::Button, true, "Region"));
    }
}

#[test]
fn unnamed_custom_widget_fails_name_and_role_until_it_has_widget_info() {
    assert_eq!(codes(&tree_of(|ui| custom_region(ui, false))), ["R1", "R2"]);
    assert!(codes(&tree_of(|ui| custom_region(ui, true))).is_empty());
}

#[test]
fn icon_only_button_fails_name_until_named() {
    let icon = || Atom::custom(IdSalt::new("icon"), vec2(12.0, 12.0));
    assert_eq!(codes(&tree_of(|ui| ui.add(Button::new(icon())))), ["R1"]);
    let named = tree_of(|ui| {
        ui.add(Button::new(icon()))
            .widget_info(|| WidgetInfo::labeled(accesskit::Role::Button, true, "Cube"))
    });
    assert!(codes(&named).is_empty());
}

#[test]
fn key_names_scenario_rule_role_and_anchor() {
    let update = tree_of(|ui| custom_region(ui, false));
    let key = violations("menus", &update, &[])[0].key();
    assert!(
        key.starts_with("menus|R1|") && key.matches('|').count() == 3,
        "{key}"
    );
    assert!(
        violations("menus", &update, &[])[0]
            .message()
            .contains("R1")
    );
}
