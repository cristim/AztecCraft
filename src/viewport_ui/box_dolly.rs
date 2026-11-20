//! Captured Ctrl+Alt primary gesture; cancellation consumes the release.
use super::*;

#[derive(Clone)]
struct Drag {
    origin: egui::Pos2,
    camera: Camera,
    rect: Rect,
    scale: f32,
    token: uuid::Uuid,
    cancelled: bool,
}

pub(super) struct BoxInput {
    pub active: bool,
    pub preview: Option<Rect>,
    pub operation: Option<Operation>,
}

pub(super) fn capture(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: Rect,
    camera: &Camera,
    scale: f32,
    token: uuid::Uuid,
) -> BoxInput {
    let key = response.id.with("box_dolly_capture");
    let mut drag = ui.data(|d| d.get_temp::<Drag>(key));
    let events = ui.input(|i| i.events.clone());
    let mut released = None;
    let mut presses = 0;
    for event in &events {
        if let egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        } = event
        {
            if *pressed {
                presses += 1;
                if let Some(drag) = &mut drag {
                    drag.cancelled = true;
                }
                if drag.is_none() && modifiers.ctrl && modifiers.alt && rect.contains(*pos) {
                    drag = Some(Drag {
                        origin: *pos,
                        camera: camera.clone(),
                        rect,
                        scale,
                        token,
                        cancelled: false,
                    });
                }
            } else {
                released = Some((*pos, *modifiers));
            }
        }
    }
    let Some(mut drag) = drag else {
        return BoxInput {
            active: false,
            preview: None,
            operation: None,
        };
    };
    let (focused, modifiers, position, down, scroll) = ui.input(|i| {
        (
            i.focused,
            i.modifiers,
            i.pointer.latest_pos(),
            i.pointer.button_down(PointerButton::Primary),
            i.smooth_scroll_delta,
        )
    });
    // Release-event modifiers own completion; later modifier-up events cannot cancel it.
    let modifiers = released.map_or(modifiers, |(_, modifiers)| modifiers);
    let event_count = events
        .iter()
        .position(|e| {
            matches!(
                e,
                egui::Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }
            )
        })
        .map_or(events.len(), |index| index + 1);
    let owning_events = &events[..event_count];
    drag.cancelled |= ui.input(|i| {
        i.pointer.button_down(PointerButton::Middle)
            || i.pointer.button_down(PointerButton::Secondary)
    }) || !focused
        || !modifiers.ctrl
        || !modifiers.alt
        || drag.camera != *camera
        || drag.rect != rect
        || drag.scale != scale
        || drag.token != token
        || presses > 1
        || events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    egui::Event::PointerButton {
                        button: PointerButton::Primary,
                        pressed: false,
                        ..
                    }
                )
            })
            .count()
            > 1
        || scroll != egui::Vec2::ZERO
        || owning_events.iter().any(|e| {
            matches!(
                e,
                egui::Event::PointerGone
                    | egui::Event::WindowFocused(false)
                    | egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    }
                    | egui::Event::PointerButton {
                        button: PointerButton::Middle | PointerButton::Secondary,
                        pressed: true,
                        ..
                    }
            )
        });
    drag.cancelled |= owning_events.iter().any(|e| match e {
        egui::Event::ModifiersChanged(m) | egui::Event::PointerButton { modifiers: m, .. } => {
            !m.ctrl || !m.alt
        }
        _ => false,
    });
    let to_ndc = |p: egui::Pos2| {
        let p = (p - rect.min) / rect.size();
        Vec2::new(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0)
    };
    let operation = if !drag.cancelled && response.drag_stopped_by(PointerButton::Primary) {
        released
            .and_then(|(end, _)| NdcRect::clamped(to_ndc(drag.origin), to_ndc(end)).ok())
            .map(|rect| Operation::BoxDolly { rect })
    } else {
        None
    };
    let preview = (!drag.cancelled && down && response.dragged_by(PointerButton::Primary))
        .then_some(position)
        .flatten()
        .map(|p| Rect::from_two_pos(rect.clamp(drag.origin), rect.clamp(p)));
    // A lost pointer remains consumed until a release or the first all-up frame.
    ui.data_mut(|d| {
        if !down {
            d.remove::<Drag>(key);
        } else {
            d.insert_temp(key, drag);
        }
    });
    BoxInput {
        active: true,
        preview,
        operation,
    }
}
