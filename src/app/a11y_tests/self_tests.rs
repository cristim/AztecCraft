//! Detection proofs: each rule fails on a synthetic control and passes on the fixed one.

use super::*;
use eframe::egui::{Atom, Button, IdSalt, Sense, TextEdit, WidgetInfo, accesskit, vec2};

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

/// Test-local stand-in for the production helper: the reason replaces any hover description.
fn describe_disabled(response: egui::Response, reason: &str) -> egui::Response {
    if !response.enabled() {
        response.ctx.accesskit_node_builder(response.id, |node| {
            node.set_description(format!("{DISABLED_PREFIX}{reason}"));
        });
    }
    response
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
fn text_edit_fails_name_until_labelled() {
    let mut text = String::new();
    assert_eq!(
        codes(&tree_of(|ui| ui.add(TextEdit::singleline(&mut text)))),
        ["R1"]
    );
    let by_name = tree_of(|ui| {
        ui.add(TextEdit::singleline(&mut text))
            .accessible_name("Object name");
    });
    assert!(codes(&by_name).is_empty());
    let by_label = tree_of(|ui| {
        let label = ui.label("Translate X");
        ui.add(TextEdit::singleline(&mut text))
            .labelled_by(label.id);
    });
    assert!(codes(&by_label).is_empty());
}

#[test]
fn disabled_control_needs_the_reason_description() {
    let hover_only = tree_of(|ui| {
        ui.add_enabled(false, Button::new("Undo"))
            .on_hover_text("Nothing to undo");
    });
    assert_eq!(codes(&hover_only), ["R3"]);
    let described = tree_of(|ui| {
        describe_disabled(
            ui.add_enabled(false, Button::new("Undo"))
                .on_hover_text("Nothing to undo"),
            "Nothing to undo",
        );
    });
    assert!(codes(&described).is_empty());
}

#[test]
fn enabled_control_with_the_reason_prefix_fails() {
    let stale = tree_of(|ui| {
        let response = ui.button("Undo");
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_description(format!("{DISABLED_PREFIX}old reason"));
        });
    });
    assert_eq!(codes(&stale), ["R3b"]);
    let enabled = tree_of(|ui| describe_disabled(ui.button("Undo"), "never written"));
    assert!(codes(&enabled).is_empty());
}

#[test]
fn checkbox_without_state_fails() {
    let stateless = tree_of(|ui| {
        let response = ui.allocate_response(vec2(20.0, 20.0), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(accesskit::Role::CheckBox, true, "Flag"));
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.clear_toggled());
    });
    assert_eq!(codes(&stateless), ["R4"]);
    let mut flag = false;
    assert!(codes(&tree_of(|ui| ui.checkbox(&mut flag, "Flag"))).is_empty());
}

#[test]
fn glyph_name_fails_quality() {
    assert_eq!(codes(&tree_of(|ui| ui.button("\u{2610}"))), ["R5"]);
    assert_eq!(codes(&tree_of(|ui| ui.button("<k"))), ["R5"]);
    assert!(codes(&tree_of(|ui| ui.button("Options"))).is_empty());
}

#[test]
fn registered_window_chrome_is_exempt_and_unregistered_chrome_fails() {
    let area = titled_window_id("Probe window");
    let window = |ui: &mut egui::Ui| {
        egui::Window::new("Probe window")
            .resizable(true)
            .show(ui.ctx(), |ui| ui.button("Close probe"));
    };
    let update = tree_of(window);
    let chrome = chrome_ids(&[area]);
    let present: Vec<_> = update
        .nodes
        .iter()
        .filter(|(id, node)| chrome.contains(id) && node.role() == Role::Unknown)
        .collect();
    // Fails when egui renames its title id: the exemption would silently stop matching. The
    // resize corner exists only for some windows; the UV Editor scenario covers it.
    let title = area.with("__title_click").accesskit_id();
    assert!(
        present.iter().any(|(id, _)| *id == title),
        "egui title id changed"
    );
    assert!(violations("fixture", &update, &[area]).is_empty());
    let loud = violations("fixture", &update, &[]);
    assert!(!loud.is_empty() && loud.iter().all(|v| v.rule.code() != "R3"));
    assert!(
        loud.iter()
            .any(|v| v.message().contains("register the window"))
    );
}

#[test]
fn chrome_id_outside_a_window_is_not_exempt() {
    let area = titled_window_id("Probe window");
    let update = tree_of(|ui| {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(20.0, 20.0));
        ui.interact(rect, area.with("__title_click"), Sense::click());
    });
    assert!(!violations("fixture", &update, &[area]).is_empty());
}

#[test]
fn key_names_scenario_rule_role_and_anchor() {
    let update = tree_of(|ui| custom_region(ui, false));
    let key = violations("menus", &update, &[])[0].key();
    assert!(
        key.starts_with("menus|R1|") && key.matches('|').count() == 3,
        "{key}"
    );
}
