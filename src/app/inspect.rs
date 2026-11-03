use super::*;

impl Editor {
    #[cfg(any(test, feature = "mcp"))]
    fn inspect_session_materials(
        &self,
    ) -> Vec<crate::editor_operations::SessionMaterialInspection> {
        let scene = self.history.scene();
        if scene.appearance_incomplete() {
            return Vec::new();
        }
        scene
            .session_materials()
            .iter()
            .zip(scene.session_shading_groups())
            .enumerate()
            .map(
                |(index, (material, group))| crate::editor_operations::SessionMaterialInspection {
                    id: index.to_string(),
                    name: material.name().into(),
                    shading_group: group.name().into(),
                    color: material.color(),
                    editable: index != 0,
                },
            )
            .collect()
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn current_viewport_error(&self) -> Option<crate::viewport_renderer::ViewportError> {
        use crate::viewport_renderer::ViewportError;
        match &self.renderer {
            None => Some(ViewportError::Uninitialized),
            Some(renderer) if renderer.terminal_error().is_some() => {
                Some(ViewportError::BackendFailed)
            }
            Some(_) => self.viewport_error,
        }
    }
    /// Guaranteed minimum of the serialized path fields: 4 bytes per path element of every node
    /// instance path, selection member and remembered target, plus the fixed keys of each member
    /// and each present target (net of the `null` it replaces).
    #[cfg(any(test, feature = "mcp"))]
    fn path_lower_bound(&self, node_paths: &[azteccraft::native_scene::TransformTarget]) -> usize {
        use crate::editor_operations::{
            MIN_EDGE_TARGET, MIN_FACE_TARGET, MIN_NULL, MIN_PATH_ELEMENT, MIN_SELECTION_MEMBER,
            MIN_VERTEX_TARGET,
        };
        let elements = |len: usize| len.saturating_mul(MIN_PATH_ELEMENT);
        // Members and targets are counted with an empty `[]`; L elements add 4L - 1 (no last comma).
        let added = |len: usize| elements(len).saturating_sub(1);
        let target = |fixed: &str, len: Option<usize>| {
            len.map_or(0, |len| (fixed.len() - MIN_NULL).saturating_add(added(len)))
        };
        let mut bytes = 0usize;
        for path_len in node_paths.iter().map(|target| target.path.len()) {
            bytes = bytes.saturating_add(elements(path_len));
        }
        for path_len in self
            .selection
            .members()
            .iter()
            .map(|member| member.path.len())
        {
            bytes = bytes
                .saturating_add(MIN_SELECTION_MEMBER.len())
                .saturating_add(added(path_len));
        }
        [
            target(
                MIN_FACE_TARGET,
                self.remembered_face.as_ref().map(|t| t.path.len()),
            ),
            target(
                MIN_VERTEX_TARGET,
                self.remembered_vertex.as_ref().map(|t| t.path.len()),
            ),
            target(
                MIN_EDGE_TARGET,
                self.remembered_edge.as_ref().map(|t| t.path.len()),
            ),
        ]
        .into_iter()
        .fold(bytes, usize::saturating_add)
    }
    /// The node range a page covers. The cursor is untrusted: token and revision are compared
    /// first, and only then is its offset checked against the node count.
    #[cfg(any(test, feature = "mcp"))]
    fn page_range(
        &self,
        page: &crate::editor_operations::InspectPage,
    ) -> Result<std::ops::Range<usize>, OperationError> {
        let total = self.history.scene().nodes().len();
        if !(1..=azteccraft::native_scene::MAX_TRANSFORM_PATHS).contains(&page.limit) {
            return Err(OperationError::InvalidInput(format!(
                "node_limit must be between 1 and {}",
                azteccraft::native_scene::MAX_TRANSFORM_PATHS
            )));
        }
        let start = match &page.cursor {
            None => 0,
            Some(cursor) => {
                if cursor.scene_token != self.scene_token
                    || cursor.revision != self.history.revision()
                {
                    return Err(OperationError::StaleScene);
                }
                if cursor.offset > total {
                    return Err(OperationError::InvalidInput(
                        "cursor offset is past the last node".into(),
                    ));
                }
                cursor.offset
            }
        };
        Ok(start..start.saturating_add(page.limit).min(total))
    }
    #[cfg(test)]
    pub(super) fn inspect(&self, text_focused: bool) -> Result<SceneInspection, OperationError> {
        self.inspect_page(text_focused, None)
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn inspect_page(
        &self,
        text_focused: bool,
        page: Option<&crate::editor_operations::InspectPage>,
    ) -> Result<SceneInspection, OperationError> {
        self.inspect_within(
            text_focused,
            crate::editor_operations::MAX_REPLY_BYTES,
            page,
        )
    }
    /// `path_limit` exists so tests can reach the budget sum through the real call site.
    #[cfg(any(test, feature = "mcp"))]
    fn inspect_within(
        &self,
        text_focused: bool,
        path_limit: usize,
        page: Option<&crate::editor_operations::InspectPage>,
    ) -> Result<SceneInspection, OperationError> {
        let scene = self.history.scene();
        let range = match page {
            None => 0..scene.nodes().len(),
            Some(page) => self.page_range(page)?,
        };
        crate::editor_operations::check_inspection_page(scene, range.clone())?;
        let all_paths = scene.transform_paths().map_err(OperationError::Scene)?;
        let page_nodes = &scene.nodes()[range.clone()];
        // Every instance path in an unpaged reply; a page keeps only its own nodes' paths.
        let paths: Vec<_> = if page.is_none() {
            all_paths
        } else {
            let ids: std::collections::HashSet<_> =
                page_nodes.iter().map(|node| node.id()).collect();
            all_paths
                .into_iter()
                .filter(|target| ids.contains(&target.node))
                .collect()
        };
        // Shapes and curves a page lists: those its nodes hold. Unpaged lists them all.
        let held: std::collections::HashSet<_> = {
            let transforms: std::collections::HashSet<_> =
                page_nodes.iter().map(|node| node.transform_id()).collect();
            scene
                .dag_edges()
                .iter()
                .filter(|edge| {
                    edge.parent()
                        .is_some_and(|parent| transforms.contains(&parent))
                })
                .map(|edge| edge.child())
                .collect()
        };
        let listed = |id| page.is_none() || held.contains(&id);
        // The schema bounds a target's path to MAX_TRANSFORM_DEPTH + 1 elements.
        let too_long = [
            self.remembered_face.as_ref().map(|t| t.path.len()),
            self.remembered_vertex.as_ref().map(|t| t.path.len()),
            self.remembered_edge.as_ref().map(|t| t.path.len()),
        ]
        .into_iter()
        .flatten()
        .any(|len| len > azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1);
        if too_long {
            return Err(OperationError::TooLarge);
        }
        let total = crate::editor_operations::inspection_page_lower_bound(scene, range.clone())
            .saturating_add(self.path_lower_bound(&paths));
        if total > path_limit {
            return Err(OperationError::TooLarge);
        }
        let units = self.history.scene().units();
        let linear = linear_unit_name(&units.linear);
        let angular = angular_unit_name(&units.angular);
        if linear
            .len()
            .saturating_add(angular.len())
            .saturating_add(units.time.len())
            > 4096
        {
            return Err(OperationError::TooLarge);
        }
        let nodes = page_nodes
            .iter()
            .map(|node| {
                Ok(NodeInspection {
                    id: node.id().to_decimal(),
                    transform_id: node.transform_id().to_decimal(),
                    name: node.name().into(),
                    visible: node.local_visibility(),
                    transform: self.transform_input(node.transform())?,
                    authoring_transform: node.transform().into(),
                    transform_attributes: Some(
                        self.history
                            .scene()
                            .transform_attributes(node.id())
                            .map_err(OperationError::Scene)?,
                    )
                    .filter(|attributes| !attributes.non_default.is_empty()),
                    shape_ids: self
                        .history
                        .scene()
                        .dag_edges()
                        .iter()
                        .filter(|edge| edge.parent() == Some(node.transform_id()))
                        .filter_map(|edge| {
                            let id = edge.child();
                            (self.history.scene().shape(id).is_ok()
                                || self
                                    .history
                                    .scene()
                                    .curves()
                                    .iter()
                                    .any(|curve| curve.id() == id))
                            .then(|| id.to_decimal())
                        })
                        .collect(),
                    instance_paths: paths
                        .iter()
                        .filter(|target| target.node == node.id())
                        .map(|target| target.path.iter().map(|id| id.to_decimal()).collect())
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, OperationError>>()?;
        let viewport_error = self.current_viewport_error();
        let has_curves = scene.curves().iter().any(|curve| listed(curve.id()));
        let prepared_view =
            has_curves.then(|| ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024));
        let gpu_healthy = self.renderer.is_some()
            && viewport_error != Some(crate::viewport_renderer::ViewportError::BackendFailed);
        let (selected, selected_path) = self.selection_presentation();
        Ok(SceneInspection {
            scene_token: self.scene_token.to_string(),
            manipulation: self.inspect_manipulation()?,
            nodes,
            retained_source_bytes: self.history.scene().retained_source_bytes(),
            retained_source_records: self.history.scene().retained_source_records(),
            read_only_reason: self
                .history
                .scene()
                .authoring_read_only_reason()
                .map(|reason| reason.to_string()),
            curves: scene
                .curves()
                .iter()
                .filter(|curve| listed(curve.id()))
                .map(|curve| {
                    Ok(crate::editor_operations::CurveInspection {
                        id: curve.id().to_decimal(),
                        name: curve.name().into(),
                        degree: curve.degree(),
                        form: curve.form(),
                        control_vertices: curve.control_vertices().len(),
                        knots: curve.knots().len(),
                        domain: curve.domain().map_err(OperationError::Scene)?,
                        read_only: self.history.scene().authoring_read_only_reason().is_some(),
                        draw_available: prepared_view.as_ref().is_some_and(|prepared| {
                            prepared.as_ref().is_ok_and(|view| {
                                view.curve_instances()
                                    .iter()
                                    .any(|instance| instance.shape() == curve.id())
                            })
                        }),
                    })
                })
                .collect::<Result<Vec<_>, OperationError>>()?,
            shapes: scene
                .shapes()
                .iter()
                .filter(|shape| listed(shape.id()))
                .map(|shape| {
                    Ok(ShapeInspection {
                        id: shape.id().to_decimal(),
                        name: shape.name().into(),
                        points: shape.mesh().point_count(),
                        edges: shape.mesh().edges().len(),
                        faces: shape.mesh().faces().len(),
                        smoothing_known: shape
                            .mesh()
                            .edges()
                            .iter()
                            .all(|edge| edge.smooth().is_some()),
                        source_appearance: self
                            .history
                            .scene()
                            .shape_appearance_readiness(shape.id())
                            .map_err(OperationError::Scene)?,
                        session_material: self
                            .history
                            .scene()
                            .session_material_id(shape)
                            .map(|id| id.index().to_string()),
                        material: match self.history.scene().material_for_shape(shape.id()) {
                            Ok(value) => Some(crate::editor_operations::MaterialInspection {
                                shader: match value.shader {
                                    azteccraft::native_scene::MaterialShader::Phong => "phong",
                                },
                                diffuse: value.diffuse,
                                shared_users: value.shared_users,
                                editing_available: value.editing_available,
                                rendering_available: value.rendering_available,
                            }),
                            Err(azteccraft::native_scene::NativeError::UnsupportedEdit) => None,
                            Err(error) => return Err(OperationError::Scene(error)),
                        },
                    })
                })
                .collect::<Result<Vec<_>, OperationError>>()?,
            session_materials: self.inspect_session_materials(),
            units: UnitsInspection {
                linear: linear.into(),
                angular: angular.into(),
                time: units.time.clone(),
            },
            save_available: self.files.save_available(),
            save_as_available: self.files.save_as_available(),
            save_reconciliation_required: self.files.reconciliation_required(),
            document_origin: self.files.document_origin(),
            has_file: self.files.has_file(),
            closing: self.files.is_closing(),
            viewport_backend: if self.renderer.is_some() {
                "wgpu_geometry_preview"
            } else {
                "uninitialized"
            },
            viewport_error,
            grid_visible: self.grid_visible,
            shading: self.shading,
            face_centres_hidden: self.face_centres_hidden,
            vertex_markers_hidden: self.vertex_markers_hidden,
            sidebar_panels: SidebarPanelsInspection {
                channel_box: self.sidebar_panel_visible(SidebarPanel::ChannelBox),
                attribute_editor: self.sidebar_panel_visible(SidebarPanel::AttributeEditor),
            },
            attribute_editor_tab: self
                .sidebar_panel_visible(SidebarPanel::AttributeEditor)
                .then(|| self.effective_attribute_editor_tab())
                .flatten()
                .map(|tab| match tab {
                    AttributeEditorTab::Transform(id) => AttributeEditorTabInspection {
                        kind: "transform",
                        id: id.to_decimal(),
                    },
                    AttributeEditorTab::Shape(id) => AttributeEditorTabInspection {
                        kind: "shape",
                        id: id.to_decimal(),
                    },
                    AttributeEditorTab::Material(id) => AttributeEditorTabInspection {
                        kind: "material",
                        id: id.to_decimal(),
                    },
                }),
            depth_complete: gpu_healthy,
            clipping_complete: gpu_healthy,
            appearance_incomplete: self.history.scene().appearance_incomplete(),
            source_appearance: self.history.scene().appearance_readiness(),
            pending_action: self.files.pending_inspection(),
            selected_node: selected.map(NodeId::to_decimal),
            selected_instance_path: selected_path.iter().map(|id| id.to_decimal()).collect(),
            selection: self
                .selection
                .members()
                .iter()
                .map(
                    |member| crate::editor_operations::SelectionMemberInspection {
                        node: member.node.to_decimal(),
                        instance_path: member.path.iter().map(|id| id.to_decimal()).collect(),
                    },
                )
                .collect(),
            selection_count: self.selection.len(),
            selection_mode: self.selection_mode,
            remembered_face: self
                .remembered_face
                .as_ref()
                .map(|target| FaceSelectionTarget {
                    node_id: target.transform.to_decimal(),
                    shape_id: target.shape.to_decimal(),
                    instance_path: target.path.iter().map(|id| id.to_decimal()).collect(),
                    face_index: target.face_index,
                }),
            remembered_vertex: self.remembered_vertex.as_ref().map(|target| {
                VertexSelectionTarget {
                    node_id: target.transform.to_decimal(),
                    shape_id: target.shape.to_decimal(),
                    instance_path: target.path.iter().map(|id| id.to_decimal()).collect(),
                    vertex_index: target.vertex_index,
                }
            }),
            remembered_edge: self
                .remembered_edge
                .as_ref()
                .map(|target| EdgeSelectionTarget {
                    node_id: target.transform.to_decimal(),
                    shape_id: target.shape.to_decimal(),
                    instance_path: target.path.iter().map(|id| id.to_decimal()).collect(),
                    edge_index: target.edge_index,
                }),
            face_selection_policy: FaceSelectionPolicy::VisibleCentersOnly,
            can_undo: self.history.can_undo(),
            undo_label: self.history.undo_label(),
            can_redo: self.history.can_redo(),
            redo_label: self.history.redo_label(),
            dirty: self.is_dirty(),
            has_draft: self.has_draft(),
            native_input_pending: self.native_input_pending,
            text_focused,
            modal: self.files.is_modal(),
            status_message: self.status_message(),
            page: page.map(|_| {
                let total = scene.nodes().len();
                let next_cursor = (range.end < total).then(|| {
                    crate::editor_operations::PageCursor {
                        scene_token: self.scene_token,
                        revision: self.history.revision(),
                        offset: range.end,
                    }
                    .to_string()
                });
                crate::editor_operations::PageInspection {
                    nodes_total: total,
                    node_offset: range.start,
                    node_count: range.len(),
                    truncated: next_cursor.is_some(),
                    next_cursor,
                }
            }),
            camera: CameraInspection {
                target: self.camera.target().to_array(),
                eye: self.camera.eye().to_array(),
                distance: self.camera.distance(),
                yaw_degrees: f64::from(self.camera.yaw()).to_degrees(),
                pitch_degrees: f64::from(self.camera.pitch()).to_degrees(),
                fov_y_degrees: f64::from(self.camera.fov_y_for(self.aspect)).to_degrees(),
                near_clip: self.camera.near(),
                far_clip: self.camera.far(),
            },
        })
    }
}

#[cfg(any(test, feature = "mcp"))]
pub(in crate::app) fn linear_unit_name(unit: &LinearUnit) -> &str {
    match unit {
        LinearUnit::Centimeter => "cm",
        LinearUnit::Millimeter => "mm",
        LinearUnit::Meter => "m",
        LinearUnit::Inch => "in",
        LinearUnit::Foot => "ft",
        LinearUnit::Unsupported(value) => value,
    }
}

pub(in crate::app) fn angular_unit_name(unit: &AngularUnit) -> &str {
    match unit {
        AngularUnit::Degree => "deg",
        AngularUnit::Radian => "rad",
        AngularUnit::Unsupported(value) => value,
    }
}

#[cfg(test)]
mod tests {
    use crate::editor_operations::{
        MIN_EDGE_TARGET, MIN_FACE_TARGET, MIN_NULL, MIN_PATH_ELEMENT, MIN_SELECTION_MEMBER,
        MIN_VERTEX_TARGET, OperationError, inspection_lower_bound,
    };

    #[test]
    fn inspect_sums_node_selection_and_remembered_paths_against_its_limit() {
        let mut editor = crate::app::Editor::default();
        editor.create(azteccraft::scene::Primitive::Cube);
        let id = editor.selected().unwrap();
        let shape = editor.selected_path()[0];
        editor.remembered_face = Some(azteccraft::native_scene::FaceTarget {
            transform: id,
            shape,
            path: vec![shape; 2],
            face_index: 0,
        });
        editor.remembered_vertex = Some(azteccraft::native_scene::VertexTarget {
            transform: id,
            shape,
            path: vec![shape; 3],
            vertex_index: 0,
        });
        editor.remembered_edge = Some(azteccraft::native_scene::EdgeTarget {
            transform: id,
            shape,
            path: vec![shape; 4],
            edge_index: 0,
        });
        let node_elements: usize = editor
            .history
            .scene()
            .transform_paths()
            .unwrap()
            .iter()
            .map(|target| target.path.len())
            .sum();
        let selection_elements: usize = editor
            .selection
            .members()
            .iter()
            .map(|member| member.path.len())
            .sum();
        let remembered =
            |fixed: &str, len: usize| fixed.len() - MIN_NULL + len * MIN_PATH_ELEMENT - 1;
        let total = inspection_lower_bound(editor.history.scene())
            + (node_elements + selection_elements) * MIN_PATH_ELEMENT
            - editor.selection.len()
            + editor.selection.len() * MIN_SELECTION_MEMBER.len()
            + remembered(MIN_FACE_TARGET, 2)
            + remembered(MIN_VERTEX_TARGET, 3)
            + remembered(MIN_EDGE_TARGET, 4);
        assert!(editor.inspect_within(false, total, None).is_ok());
        assert!(matches!(
            editor.inspect_within(false, total - 1, None),
            Err(OperationError::TooLarge)
        ));
    }

    /// A chain of 150 nested transforms: node k has a k-element instance path, 11 325 elements in
    /// all. The old 24 bytes per element plus 64 per path charged about 270 KB against 256 KiB;
    /// the paths really serialize to about 100 KB.
    #[test]
    fn a_deep_transform_chain_inspects_within_the_limit() {
        let mut source = String::from(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\";\ncurrentUnit -l centimeter -a degree -t film;\n",
        );
        for depth in 0..150 {
            let parent = if depth == 0 {
                String::new()
            } else {
                format!(" -p \"t{}\"", depth - 1)
            };
            source.push_str(&format!("createNode transform -n \"t{depth}\"{parent};\n"));
        }
        let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
        let editor = crate::app::Editor {
            history: crate::app::History::new(scene),
            ..Default::default()
        };
        let paths = editor.history.scene().transform_paths().unwrap();
        assert_eq!(paths.iter().map(|p| p.path.len()).sum::<usize>(), 11_325);
        let inspection = editor.inspect(false).unwrap();
        let real = serde_json::to_vec(&inspection).unwrap().len();
        let bound =
            inspection_lower_bound(editor.history.scene()) + editor.path_lower_bound(&paths);
        assert!(bound <= real && real <= crate::editor_operations::MAX_REPLY_BYTES);
    }
}

#[cfg(test)]
mod selection_tests {
    use crate::editor_operations::{
        MAX_RESULT_BYTES, MIN_PATH_ELEMENT, MIN_SELECTION_MEMBER, OperationError,
    };
    use azteccraft::scene::Primitive;

    fn two_selected() -> crate::app::Editor {
        let mut editor = crate::app::Editor::default();
        editor.create(Primitive::Cube);
        editor.create(Primitive::Cube);
        let paths = editor.history.scene().transform_paths().unwrap();
        editor.set_selection_members_for_test(paths);
        editor
    }

    #[test]
    fn inspection_lists_the_selection_in_order_with_the_lead_last() {
        let editor = two_selected();
        let members = editor.selection.members().to_vec();
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(inspection.selection_count, 2);
        let listed: Vec<_> = inspection
            .selection
            .iter()
            .map(|member| (member.node.clone(), member.instance_path.clone()))
            .collect();
        let expected: Vec<_> = members
            .iter()
            .map(|t| {
                (
                    t.node.to_decimal(),
                    t.path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(listed, expected);
        assert_eq!(inspection.selected_node, Some(expected[1].0.clone()));
        assert_eq!(inspection.selected_instance_path, expected[1].1);
        let json = serde_json::to_value(&inspection).unwrap();
        assert_eq!(json["selection_count"], 2);
        assert_eq!(json["selection"][1]["node"], expected[1].0);
        assert!(serde_json::to_vec(&json).unwrap().len() < MAX_RESULT_BYTES);
    }

    #[test]
    fn an_empty_selection_reports_an_empty_list() {
        let mut editor = two_selected();
        editor.deselect_all();
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(inspection.selection_count, 0);
        assert!(inspection.selection.is_empty() && inspection.selected_node.is_none());
    }

    #[test]
    fn inspection_charges_the_selection_set_against_its_limit() {
        let editor = two_selected();
        let paths = editor.history.scene().transform_paths().unwrap();
        let elements: usize = paths.iter().map(|target| target.path.len()).sum();
        // Node paths and the selection list the same two paths; each is charged by its size.
        let total = crate::editor_operations::inspection_lower_bound(editor.history.scene())
            + 2 * elements * MIN_PATH_ELEMENT
            - 2
            + 2 * MIN_SELECTION_MEMBER.len();
        assert!(editor.inspect_within(false, total, None).is_ok());
        assert!(matches!(
            editor.inspect_within(false, total - 1, None),
            Err(OperationError::TooLarge)
        ));
    }
}

#[cfg(test)]
mod lower_bound_tests {
    // No multi-instance or `visible: null` fixture: the bin crate cannot build instancing
    // (src/app/tests.rs:8252), MIN_NODE already uses null (the smallest value), and node
    // instance paths are charged 4L against a real cost of at least 4L + 1.
    use crate::app::{Editor, History};
    use crate::editor_operations::{MIN_FIXED, inspection_lower_bound};
    use azteccraft::native_scene::{NativeScene, ascii};
    use azteccraft::scene::{Primitive, Scene};

    mod ascii_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_ascii.rs"
        ));
    }

    fn editor_for(scene: NativeScene) -> Editor {
        Editor {
            history: History::new(scene),
            ..Editor::default()
        }
    }

    /// The serialized length of the real inspection, after checking the bound does not exceed it.
    fn assert_lower_bound(editor: &Editor) -> usize {
        let inspection = editor.inspect(false).unwrap();
        let real = serde_json::to_vec(&inspection).unwrap().len();
        let paths = editor.history.scene().transform_paths().unwrap();
        let bound =
            inspection_lower_bound(editor.history.scene()) + editor.path_lower_bound(&paths);
        assert!(bound <= real, "bound {bound} exceeds the real {real} bytes");
        real
    }

    fn key_structure(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), key_structure(value)))
                    .collect(),
            ),
            _ => serde_json::Value::Null,
        }
    }

    #[test]
    fn the_fixed_minimum_has_every_key_of_the_tightest_inspection() {
        // An empty default editor: every Option null, nothing selected, no nodes.
        let editor = Editor::default();
        let real = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        let minimum: serde_json::Value = serde_json::from_str(MIN_FIXED).unwrap();
        assert_eq!(key_structure(&minimum), key_structure(&real));
        let real_len = assert_lower_bound(&editor);
        assert!(MIN_FIXED.len() <= real_len);
        assert!(
            real_len - MIN_FIXED.len() < 1024,
            "the minimum drifted loose"
        );
    }

    #[test]
    fn the_bound_holds_for_a_sourced_scene_with_curves_and_null_session_material() {
        let editor = editor_for(ascii::decode(ascii_fixture::specimen().into()).unwrap());
        assert!(editor.history.scene().appearance_incomplete());
        assert_lower_bound(&editor);
    }

    #[test]
    fn the_bound_holds_for_flat_cubes_up_to_the_reply_cap() {
        for count in [1, 50, 100, 140, 146] {
            let mut editor = Editor::default();
            for _ in 0..count {
                editor.create(Primitive::Cube);
            }
            let real = assert_lower_bound(&editor);
            assert!(count < 146 || real <= crate::editor_operations::MAX_REPLY_BYTES);
        }
    }

    #[test]
    fn the_bound_holds_for_escaped_and_multibyte_names() {
        let mut accepted = 0;
        for name in [
            "quo\"te\\back".to_string(),
            "caf\u{e9} \u{4e2d}\u{6587} \u{1f600}".to_string(),
            "\"".repeat(1000),
            "plain".to_string(),
        ] {
            let mut legacy = Scene::default();
            legacy.create(Primitive::Cube, &name).unwrap();
            let editor = editor_for(NativeScene::from_legacy(legacy).unwrap());
            assert_lower_bound(&editor);
            accepted += 1;
        }
        assert_eq!(accepted, 4);
    }

    #[test]
    fn the_bound_holds_for_a_full_session_material_table() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let mut scene = editor.history.scene().clone();
        while scene.session_materials().len() < azteccraft::native_scene::MAX_SESSION_MATERIALS {
            scene.create_session_material([0.5; 3]).unwrap();
        }
        assert_lower_bound(&editor_for(scene));
    }

    #[test]
    fn the_bound_holds_for_transform_attributes_and_unproved_or_false_visibility() {
        let source = "//Maya ASCII 2016 scene\nrequires maya \"2016\";\ncurrentUnit -l centimeter -a degree -t film;\n\
            createNode transform -n \"a\";\nsetAttr \".ro\" 3;\nsetAttr \".sh\" -type \"double3\" 1 2 3;\n\
            createNode transform -n \"b\";\nsetAttr \".v\" no;\ncreateNode transform -n \"c\";\n";
        let editor = editor_for(ascii::decode(source.as_bytes().to_vec().into()).unwrap());
        let inspection = editor.inspect(false).unwrap();
        assert!(
            inspection
                .nodes
                .iter()
                .any(|n| n.transform_attributes.is_some())
        );
        assert!(inspection.nodes.iter().any(|n| n.visible == Some(false)));
        assert_lower_bound(&editor);
    }

    #[test]
    fn a_scene_the_worst_case_estimate_refused_inspects_at_the_editor() {
        let mut editor = Editor::default();
        for _ in 0..120 {
            editor.create(Primitive::Cube);
        }
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(inspection.nodes.len(), 120);
        assert!(serde_json::to_vec(&inspection).unwrap().len() < 128 * 1024);
    }

    #[test]
    fn the_bound_holds_for_a_full_selection_and_remembered_component_targets() {
        let mut editor = Editor::default();
        for _ in 0..20 {
            editor.create(Primitive::Cube);
        }
        let paths = editor.history.scene().transform_paths().unwrap();
        let transform = editor.selected().unwrap();
        let shape = editor.selected_path()[0];
        editor.set_selection_members_for_test(paths);
        let path = vec![shape; 2];
        editor.remembered_face = Some(azteccraft::native_scene::FaceTarget {
            transform,
            shape,
            path: path.clone(),
            face_index: 123_456,
        });
        editor.remembered_vertex = Some(azteccraft::native_scene::VertexTarget {
            transform,
            shape,
            path: path.clone(),
            vertex_index: 7,
        });
        editor.remembered_edge = Some(azteccraft::native_scene::EdgeTarget {
            transform,
            shape,
            path,
            edge_index: 0,
        });
        assert_lower_bound(&editor);
    }
}

#[cfg(test)]
mod paging_tests;
