use crate::app::*;
use azteccraft::{
    camera_optics::{CameraOptics, FilmFit, FilmRollOrder},
    native_scene::SceneCamera,
    viewport::Lens,
};

pub(in crate::app) fn field_id(i: usize) -> egui::Id {
    egui::Id::unique(("camera_optics", i))
}

const LABELS: [&str; 16] = [
    "Focal Length (mm)",
    "Horizontal Aperture",
    "Vertical Aperture",
    "Horizontal Film Offset",
    "Vertical Film Offset",
    "Film Fit Offset",
    "Lens Squeeze Ratio",
    "Overscan",
    "Camera Scale",
    "Pre Scale",
    "Post Scale",
    "Film Translate H",
    "Film Translate V",
    "Film Roll (deg)",
    "Horizontal Roll Pivot",
    "Vertical Roll Pivot",
];

pub(in crate::app) struct CameraDraft {
    token: Uuid,
    camera: SceneCamera,
    original: CameraOptics,
    text: [String; 16],
    fit: FilmFit,
    order: FilmRollOrder,
    mm: bool,
}
impl CameraDraft {
    fn new(token: Uuid, camera: SceneCamera, original: CameraOptics) -> Self {
        let mut draft = Self {
            token,
            camera,
            original,
            text: original.values().map(|v| v.to_string()),
            fit: original.film_fit,
            order: original.film_roll_order,
            mm: true,
        };
        draft.text = std::array::from_fn(|i| draft.shown(i));
        draft
    }

    fn shown(&self, i: usize) -> String {
        let v = self.original.values()[i];
        (if i == 13 {
            v.to_degrees()
        } else if self.mm && (1..=5).contains(&i) {
            v * 25.4
        } else {
            v
        })
        .to_string()
    }
    pub(in crate::app) fn changed(&self) -> bool {
        self.fit != self.original.film_fit
            || self.order != self.original.film_roll_order
            || self
                .text
                .iter()
                .enumerate()
                .any(|(i, text)| *text != self.shown(i))
    }
    fn parse(&self) -> Result<CameraOptics, String> {
        let mut v = self.original.values();
        for (i, value) in v.iter_mut().enumerate() {
            if self.text[i] != self.shown(i) {
                *value = self.text[i]
                    .parse::<f64>()
                    .map_err(|_| format!("Enter a finite number for {}", LABELS[i]))?;
                if i == 13 {
                    *value = value.to_radians();
                } else if self.mm && (1..=5).contains(&i) {
                    *value /= 25.4;
                }
            }
        }
        let optics = CameraOptics {
            focal_length_mm: v[0],
            aperture_in: [v[1], v[2]],
            film_fit: self.fit,
            film_offset_in: [v[3], v[4]],
            film_fit_offset_in: v[5],
            lens_squeeze_ratio: v[6],
            overscan: v[7],
            camera_scale: v[8],
            pre_scale: v[9],
            post_scale: v[10],
            film_translate: [v[11], v[12]],
            film_roll_radians: v[13],
            film_roll_pivot: [v[14], v[15]],
            film_roll_order: self.order,
        };
        optics.validate().map_err(|e| e.to_string())?;
        if self.text[0] != self.shown(0) && !(2.5..=100000.0).contains(&optics.focal_length_mm) {
            return Err("Focal Length must be between 2.5 and 100000 mm".into());
        }
        Ok(optics)
    }
}
impl Editor {
    pub(in crate::app) fn active_scene_camera(&self) -> SceneCamera {
        self.active_camera
    }

    pub(in crate::app) fn sync_camera_optics(&mut self) {
        // Legacy explicit-FOV library cameras are not bound to a scene camera.
        if matches!(self.camera.lens(), Lens::VerticalFov(_)) {
            return;
        }
        let camera = self.active_scene_camera();
        self.camera.set_lens(
            if self.history.scene().camera_projection_available(camera) {
                Lens::Optics(self.history.scene().camera_optics(camera))
            } else {
                Lens::Unsupported
            },
        );
    }
    pub(in crate::app) fn check_camera_projection(
        &self,
        optics: CameraOptics,
    ) -> Result<(), OperationError> {
        let mut candidate = self.camera.clone();
        candidate.set_lens(Lens::Optics(optics));
        candidate
            .projection_matrix(self.aspect)
            .map(|_| ())
            .map_err(OperationError::Camera)
    }
    pub(in crate::app) fn camera_key_command(
        &self,
        camera: SceneCamera,
        time: Option<f64>,
        value: Option<f64>,
        delete: bool,
    ) -> Result<Command, OperationError> {
        let frame = time
            .map(azteccraft::native_scene::whole_frame)
            .transpose()
            .map_err(OperationError::Scene)?;
        let current_frame = self.timeline.current;
        Ok(if delete {
            Command::DeleteCameraFocalKeys {
                camera,
                frame,
                current_frame,
            }
        } else {
            Command::SetCameraFocalKey {
                camera,
                frame: frame.unwrap_or(current_frame),
                value: value.unwrap_or(self.history.scene().camera_optics(camera).focal_length_mm),
                current_frame,
            }
        })
    }
    fn key_camera_from_draft(&mut self) {
        let result = (|| {
            let Some(draft) = self.camera_draft.as_ref() else {
                return Err("Camera draft unavailable".to_string());
            };
            if draft.token != self.scene_token
                || self.history.scene().camera_optics(draft.camera) != draft.original
            {
                return Err("Camera changed while entering its focal key".into());
            }
            let optics = draft.parse()?;
            let mut other = optics;
            other.focal_length_mm = draft.original.focal_length_mm;
            if other != draft.original {
                return Err("Apply other camera attributes before setting a focal key".into());
            }
            Ok((draft.camera, optics.focal_length_mm))
        })();
        let (camera, value) = match result {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        };
        let draft = self.camera_draft.take();
        if self
            .native_operation_result(Operation::SetCameraFocalKey {
                camera,
                time: None,
                value: Some(value),
            })
            .is_err()
        {
            self.camera_draft = draft;
        }
    }
    pub(in crate::app) fn camera_draft_command(&self) -> Result<Command, String> {
        let draft = self
            .camera_draft
            .as_ref()
            .ok_or_else(|| "Camera draft is unavailable".to_string())?;
        if self.has_channel_draft()
            || self
                .material_draft
                .as_ref()
                .is_some_and(MaterialDraft::changed)
        {
            return Err("Finish other attribute edits before editing camera attributes".into());
        }
        if draft.token != self.scene_token
            || self.history.scene().camera_optics(draft.camera) != draft.original
        {
            return Err("Camera draft belongs to a replaced scene or changed camera".into());
        }
        let optics = draft.parse()?;
        let mut candidate = self.camera.clone();
        if draft.camera == self.active_scene_camera() {
            candidate.set_lens(Lens::Optics(optics));
            candidate
                .projection_matrix(self.aspect)
                .map_err(|e| e.to_string())?;
        }
        Ok(Command::SetCameraOptics {
            camera: draft.camera,
            optics,
        })
    }
    pub(in crate::app) fn camera_attributes(&mut self, ctx: &egui::Context) {
        if !self.camera_attributes_open {
            return;
        }
        let camera = self.active_scene_camera();
        if self.camera_draft.is_none() {
            self.camera_draft = Some(CameraDraft::new(
                self.scene_token,
                camera,
                self.history.scene().camera_optics(camera),
            ));
        }
        let available = self.history.scene().camera_projection_available(camera);
        let writable = self.history.scene().camera_writable_fields(camera);
        let admitted = !self.files.is_modal()
            && !self.files.is_closing()
            && self.history.active_manipulation().is_none()
            && !egui::Popup::is_any_open(ctx);
        let owns_input = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| (0..16).any(|i| field_id(i) == id));
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut key = false;
        let mut delete_key = false;
        egui::Window::new("Camera Attribute Editor")
            .default_pos(egui::pos2(300.0, 180.0))
            .default_width(520.0)
            .default_height(700.0)
            .min_width(460.0)
            .vscroll(true)
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                if !admitted {
                    ui.disable();
                }
                ui.label(camera.shape_name());
                ui.heading("Film Back");
                if !available {
                    ui.colored_label(
                        Color32::YELLOW,
                        "Unsupported source projection; viewport and picking unavailable",
                    );
                }
                ui.weak("Projection policy provisional; Maya comparison pending");
                let Some(draft) = self.camera_draft.as_mut() else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label("Film units");
                    let mut mm = draft.mm;
                    ui.add_enabled_ui(!draft.changed(), |ui| {
                        ui.radio_value(&mut mm, false, "in");
                        ui.radio_value(&mut mm, true, "mm");
                    });
                    if mm != draft.mm {
                        draft.mm = mm;
                        draft.text = std::array::from_fn(|i| draft.shown(i));
                    }
                });
                egui::Grid::new("camera_optical_fields").show(ui, |ui| {
                    for (i, label) in LABELS.iter().enumerate() {
                        ui.label(*label);
                        ui.add_enabled(
                            writable[i],
                            egui::TextEdit::singleline(&mut draft.text[i])
                                .id(field_id(i))
                                .desired_width(180.0)
                                .min_size(egui::vec2(180.0, 20.0)),
                        );
                        ui.end_row();
                    }
                });
                ui.add_enabled_ui(writable[16], |ui| {
                    egui::ComboBox::from_label("Film Fit")
                        .selected_text(format!("{:?}", draft.fit))
                        .show_ui(ui, |ui| {
                            for fit in [
                                FilmFit::Fill,
                                FilmFit::Horizontal,
                                FilmFit::Vertical,
                                FilmFit::Overscan,
                            ] {
                                ui.selectable_value(&mut draft.fit, fit, format!("{fit:?}"));
                            }
                        });
                });
                ui.add_enabled_ui(writable[17], |ui| {
                    egui::ComboBox::from_label("Film Roll Order")
                        .selected_text(match draft.order {
                            FilmRollOrder::RotateTranslate => "Rotate then Translate",
                            FilmRollOrder::TranslateRotate => "Translate then Rotate",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut draft.order,
                                FilmRollOrder::RotateTranslate,
                                "Rotate then Translate",
                            );
                            ui.selectable_value(
                                &mut draft.order,
                                FilmRollOrder::TranslateRotate,
                                "Translate then Rotate",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    apply = ui.button("Apply").clicked();
                    cancel = ui.button("Revert").clicked();
                });
                ui.horizontal(|ui| {
                    key = ui.button("Set Focal Key").clicked();
                    delete_key = ui.button("Delete Focal Key").clicked();
                });
                ui.weak("Focal keys save as native linear curves in authored .ma/.mb");
                if admitted && owns_input {
                    apply |= ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
                    cancel |= ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
                }
            });
        if !admitted {
            return;
        }
        if key {
            self.key_camera_from_draft();
        } else if delete_key {
            self.native_operation(Operation::DeleteCameraFocalKeys {
                camera,
                time: Some(f64::from(self.timeline.current)),
            });
        } else if cancel {
            self.camera_draft = None;
        } else if apply {
            self.commit_pending();
        }
        if !open
            && (!self.camera_draft.as_ref().is_some_and(CameraDraft::changed)
                || self.commit_pending())
        {
            self.native_operation(Operation::SetCameraAttributesVisible { visible: false });
        }
    }
}

#[cfg(any(test, feature = "mcp"))]
impl Editor {
    pub(in crate::app) fn inspect_camera_focal_keys(
        &self,
        camera: SceneCamera,
        cursor: Option<&crate::editor_operations::PageCursor>,
    ) -> Result<crate::editor_operations::KeysInspection, OperationError> {
        use crate::editor_operations::{
            CameraKeyInspection, CursorScope, KeysInspection, MAX_KEYS_PER_PAGE, PageCursor,
        };
        let scene = self.history.scene();
        let keys_total = scene.camera_focal_keys(camera).count();
        let offset = if let Some(cursor) = cursor {
            if cursor.scene_token != self.scene_token || cursor.revision != self.history.revision()
            {
                return Err(OperationError::StaleScene);
            }
            if cursor.scope != CursorScope::CameraKeys(camera) || cursor.offset > keys_total {
                return Err(OperationError::InvalidInput(
                    "Cursor does not belong to this camera key page".into(),
                ));
            }
            cursor.offset
        } else {
            0
        };
        let camera_keys: Vec<_> = scene
            .camera_focal_keys(camera)
            .skip(offset)
            .take(MAX_KEYS_PER_PAGE)
            .map(|(frame, value)| CameraKeyInspection {
                camera,
                attribute: "focal_length",
                frame,
                value,
            })
            .collect();
        let end = offset + camera_keys.len();
        Ok(KeysInspection {
            camera_keys,
            scene_token: self.scene_token.to_string(),
            revision: self.history.revision(),
            unavailable_reason: scene
                .has_source()
                .then_some("Source camera curves are not interpreted"),
            keys_total,
            key_offset: offset,
            keys: Vec::new(),
            next_cursor: (end < keys_total).then(|| {
                PageCursor {
                    scene_token: self.scene_token,
                    revision: self.history.revision(),
                    offset: end,
                    scope: CursorScope::CameraKeys(camera),
                }
                .to_string()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        editor: &mut Editor,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 1000.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| editor.render(ui));
        output.textures_delta.clear();
        output
    }
    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    #[test]
    fn native_text_enter_escape_units_and_modal_keep_camera_ownership() {
        let mut editor = Editor::default();
        let ctx = egui::Context::default();
        editor.native_operation(Operation::SetCameraAttributesVisible { visible: true });
        frame(&mut editor, &ctx, vec![]);
        assert!(ctx.read_response(field_id(0)).is_some());
        ctx.memory_mut(|m| m.request_focus(field_id(0)));
        frame(
            &mut editor,
            &ctx,
            vec![
                key(
                    Key::A,
                    egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                ),
                egui::Event::Text("48.123456789012345".into()),
            ],
        );
        assert!(editor.has_draft());
        frame(
            &mut editor,
            &ctx,
            vec![key(Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(
            editor
                .history
                .scene()
                .camera_optics(SceneCamera::Persp)
                .focal_length_mm,
            48.123456789012345
        );
        assert!(!editor.has_draft());
        frame(&mut editor, &ctx, vec![]);
        let original = editor.history.scene().camera_optics(SceneCamera::Persp);
        let draft = editor.camera_draft.as_mut().unwrap();
        draft.mm = true;
        draft.text = std::array::from_fn(|i| draft.shown(i));
        assert!(!draft.changed());
        assert_eq!(draft.parse().unwrap(), original);
        ctx.memory_mut(|m| m.request_focus(field_id(0)));
        frame(
            &mut editor,
            &ctx,
            vec![
                key(
                    Key::A,
                    egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                ),
                egui::Event::Text("85".into()),
            ],
        );
        frame(
            &mut editor,
            &ctx,
            vec![key(Key::Escape, egui::Modifiers::NONE)],
        );
        assert_eq!(
            editor.history.scene().camera_optics(SceneCamera::Persp),
            original
        );
        assert!(!editor.has_draft());
        frame(&mut editor, &ctx, vec![]);
        editor.camera_draft.as_mut().unwrap().text[0] = "95".into();
        let baseline = editor.history.scene().clone();
        // Dirty-scene New creates the real file modal without discarding camera text.
        let draft = editor.camera_draft.as_mut().unwrap();
        draft.token = Uuid::new_v4();
        assert!(!editor.commit_pending());
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.camera_draft.as_ref().unwrap().text[0], "95");
        editor.camera_draft = None;
        editor.native_operation(Operation::NewScene);
        assert!(editor.files.is_modal());
        frame(&mut editor, &ctx, vec![]);
        editor.camera_draft.as_mut().unwrap().text[0] = "110".into();
        ctx.memory_mut(|m| m.request_focus(field_id(0)));
        frame(
            &mut editor,
            &ctx,
            vec![key(Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.camera_draft.as_ref().unwrap().text[0], "110");
    }
    #[test]
    fn unavailable_source_view_can_switch_and_missing_ascii_projection_is_truthful() {
        let mut editor = Editor::default();
        editor.camera.set_lens(Lens::Unsupported);
        editor
            .apply_operation(Operation::SetView(ViewPreset::Top))
            .unwrap();
        assert!(editor.camera.projection_matrix(1.0).is_ok());
        let source: std::sync::Arc<[u8]> =
            std::sync::Arc::from(&b"//Maya ASCII 2027 scene\nrequires maya \"2027\"; currentUnit -l centimeter -a degree -t film; select -ne :perspShape; setAttr \".o\" yes;"[..]);
        let scene = azteccraft::native_scene::ascii::decode(source).unwrap();
        assert!(!scene.camera_projection_available(SceneCamera::Persp));
        assert!(scene.camera_projection_available(SceneCamera::Front));
        let ctx = egui::Context::default();
        editor.native_operation(Operation::SetCameraAttributesVisible { visible: true });
        frame(&mut editor, &ctx, vec![]);
        editor
            .apply_operation(Operation::SetView(ViewPreset::Front))
            .unwrap();
        frame(&mut editor, &ctx, vec![]);
        assert_eq!(
            editor.camera_draft.as_ref().unwrap().camera,
            SceneCamera::Front
        );
        editor
            .apply_operation(Operation::SetCameraPose {
                target: [0.0; 3],
                distance: 10.0,
                yaw: 0.7,
                pitch: 0.3,
            })
            .unwrap();
        assert_eq!(editor.active_scene_camera(), SceneCamera::Front);
        editor.sync_fields();
        frame(&mut editor, &ctx, vec![]);
        editor.camera_draft.as_mut().unwrap().text[0] = "70".into();
        assert!(editor.commit_pending());
        assert_eq!(
            editor
                .history
                .scene()
                .camera_optics(SceneCamera::Front)
                .focal_length_mm,
            70.0
        );
        assert_eq!(
            editor
                .history
                .scene()
                .camera_optics(SceneCamera::Persp)
                .focal_length_mm,
            35.0
        );
    }
    #[test]
    fn native_focal_button_keys_precise_draft_and_preserves_refused_draft() {
        fn text_center(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            for shape in shapes {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && text.galley.job.text == label
                {
                    return text.pos + text.galley.size() * 0.5;
                }
            }
            panic!("button text absent: {label}");
        }
        let mut editor = Editor::default();
        let ctx = egui::Context::default();
        editor.native_operation(Operation::SetCameraAttributesVisible { visible: true });
        frame(&mut editor, &ctx, vec![]);
        editor.camera_draft.as_mut().unwrap().text[0] = "48.123456789012345".into();
        let out = frame(&mut editor, &ctx, vec![]);
        let pos = text_center(&out.shapes, "Set Focal Key");
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            editor
                .history
                .scene()
                .camera_focal_keys(SceneCamera::Persp)
                .collect::<Vec<_>>(),
            vec![(1, 48.123456789012345)]
        );
        assert_eq!(
            editor.camera.lens(),
            Lens::Optics(editor.history.scene().camera_optics(SceneCamera::Persp))
        );
        editor.native_operation(Operation::Undo);
        assert_eq!(editor.history.scene().session_key_count(), 0);
        frame(&mut editor, &ctx, vec![]);
        editor.camera_draft.as_mut().unwrap().text[0] = "invalid".into();
        let baseline = editor.history.scene().clone();
        editor.key_camera_from_draft();
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.camera_draft.as_ref().unwrap().text[0], "invalid");
    }
}
