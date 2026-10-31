use crate::editor_operations::{
    ActiveTool, AttributeEditorTab, MenuSet, Operation, OperationError, OperationReply, SelectRule,
    SelectionChange, SelectionMode, ShadingMode, SidebarPanel,
};
#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{
    AttributeEditorTabInspection, CameraInspection, EdgeSelectionTarget, FaceSelectionPolicy,
    FaceSelectionTarget, NodeInspection, RemoteRequest, SceneInspection, ShapeInspection,
    SidebarPanelsInspection, TransformInput, UnitsInspection, VertexSelectionTarget,
};
#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::{LinearUnit, NativeTransform};
use azteccraft::{
    history::{Command, EditOutcome, History, ManipulationProvenance},
    native_scene::{AngularUnit, EdgeTarget, FaceTarget, NativeId, TransformTarget, VertexTarget},
    scene::{NodeId, Primitive},
    selection::ObjectSelection,
    view_scene::ViewScene,
    viewport::{Camera, FrameChildren},
};
use eframe::egui::{self, Color32, Key, RichText};
#[cfg(test)]
use glam::Vec3;
use uuid::Uuid;

mod dispatch;
mod features;
#[path = "../file_ui.rs"]
mod files;
mod inspect;
mod menus;
mod panels;
mod shortcuts;
mod shutdown_signal;
mod terminate_bridge;

struct MaterialDraft {
    scene_token: Uuid,
    shape: NativeId,
    original: f64,
    text: String,
    intent: bool,
    focus: egui::Id,
}

impl MaterialDraft {
    fn changed(&self) -> bool {
        self.intent || self.text != self.original.to_string()
    }
}

#[derive(Clone, Copy)]
enum SelectionModeIntent {
    Toggle,
    /// Keeps a component mode; from Object or Hierarchy enters Face.
    Component,
    Object,
    Face,
    Vertex,
    Edge,
    Hierarchy,
}

pub struct Editor {
    #[cfg(feature = "mcp")]
    mcp: Option<crate::mcp::UiBridge>,
    scene_token: Uuid,
    history: History,
    selection: ObjectSelection,
    selection_mode: SelectionMode,
    remembered_face: Option<FaceTarget>,
    remembered_vertex: Option<VertexTarget>,
    remembered_edge: Option<EdgeTarget>,
    camera: Camera,
    grid_visible: bool,
    shading: ShadingMode,
    menu_set: MenuSet,
    /// Last painted frame hid Wireframe face centres (occlusion check over budget).
    face_centres_hidden: bool,
    /// Last painted frame hid Wireframe vertex markers (occlusion check over budget).
    vertex_markers_hidden: bool,
    sidebar_dock: Option<SidebarPanel>,
    sidebar_panel_request: Option<(SidebarPanel, bool)>,
    attribute_editor_tab: Option<AttributeEditorTab>,
    attribute_editor_tab_request: Option<AttributeEditorTab>,
    active_tool: ActiveTool,
    manipulation: features::manipulation_tool::ManipulationOptions,
    native_move: Option<features::move_tool::NativeMove>,
    move_viewport: Option<features::move_tool::MoveViewportFeedback>,
    move_consumed_frame: bool,
    move_suppress_primary: bool,
    // egui reports lost_focus for two frames, so a pre-focus move suppresses blur commits for two.
    blur_commit_skip_frames: u8,
    renderer: Option<crate::viewport_renderer::ViewportRenderer>,
    viewport_error: Option<crate::viewport_renderer::ViewportError>,
    name: String,
    channels: [String; 9],
    channel_intents: [bool; 9],
    name_intent: bool,
    material_shape: Option<NativeId>,
    material_draft: Option<MaterialDraft>,
    primitive_options: features::primitive_options::PrimitiveOptionsUi,
    joint_options: features::rigging::JointOptionsUi,
    native_input_pending: bool,
    status: String,
    /// Exact `status` text produced from an error whose text can carry a local path. The native
    /// status line shows it; the inspection withholds it. Matching on the text keeps it from going
    /// stale when a later message replaces the status.
    status_withholds_path: Option<String>,
    error: bool,
    aspect: f32,
    files: files::FileState,
    terminate: Box<dyn terminate_bridge::TerminateBridge>,
    shutdown: shutdown_signal::ShutdownSignal,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            #[cfg(feature = "mcp")]
            mcp: None,
            scene_token: Uuid::new_v4(),
            history: History::default(),
            selection: ObjectSelection::default(),
            selection_mode: SelectionMode::default(),
            remembered_face: None,
            remembered_vertex: None,
            remembered_edge: None,
            camera: Camera::default(),
            grid_visible: true,
            shading: ShadingMode::default(),
            menu_set: MenuSet::default(),
            face_centres_hidden: false,
            vertex_markers_hidden: false,
            sidebar_dock: Some(SidebarPanel::ChannelBox),
            sidebar_panel_request: None,
            attribute_editor_tab: None,
            attribute_editor_tab_request: None,
            active_tool: ActiveTool::Select,
            manipulation: Default::default(),
            native_move: None,
            move_viewport: None,
            move_consumed_frame: false,
            move_suppress_primary: false,
            blur_commit_skip_frames: 0,
            renderer: None,
            viewport_error: None,
            name: String::new(),
            channels: Default::default(),
            channel_intents: [false; 9],
            name_intent: false,
            material_shape: None,
            material_draft: None,
            primitive_options: Default::default(),
            joint_options: Default::default(),
            native_input_pending: false,
            status: "Ready | Create a cube, plane, sphere, cylinder or cone to start".into(),
            status_withholds_path: None,
            error: false,
            aspect: 1.0,
            files: files::FileState::default(),
            terminate: terminate_bridge::platform_bridge(),
            shutdown: Default::default(),
        }
    }
}

impl Editor {
    /// The single place an `OperationError` becomes status text.
    fn set_error_status(&mut self, error: &OperationError) {
        self.status = error.to_string();
        self.status_withholds_path =
            matches!(error, OperationError::File(_)).then(|| self.status.clone());
    }
    /// Status text for inspection: the native line verbatim, except path-bearing errors.
    #[cfg(any(test, feature = "mcp"))]
    fn status_message(&self) -> Option<String> {
        let text = if self.status_withholds_path.as_deref() == Some(self.status.as_str()) {
            crate::editor_operations::STATUS_MESSAGE_WITHHELD
        } else {
            self.status.as_str()
        };
        (!text.is_empty()).then(|| {
            text.chars()
                .take(crate::editor_operations::STATUS_MESSAGE_MAX_CHARS)
                .collect()
        })
    }
    /// Hover text for a disabled Delete control: the read-only reason (no mode change enables
    /// Delete), otherwise the component refusal in a component mode, otherwise the missing
    /// selection.
    fn delete_disabled_hover(&self, read_only: Option<&str>, menu: bool) -> String {
        if let Some(reason) = read_only {
            return reason.to_owned();
        }
        match self.selection_mode.component_noun() {
            Some(noun) if menu => format!(
                "Object or Hierarchy selection required; {} deletion is unsupported",
                noun.to_lowercase()
            ),
            Some(noun) => {
                format!("{noun} deletion is unsupported; switch to Object or Hierarchy selection")
            }
            None => "Select an object to delete".into(),
        }
    }
    fn clear_remembered_components(&mut self) {
        self.remembered_face = None;
        self.remembered_vertex = None;
        self.remembered_edge = None;
    }

    pub(super) fn sidebar_panel_visible(&self, panel: SidebarPanel) -> bool {
        self.sidebar_dock == Some(panel)
    }
    #[cfg(feature = "mcp")]
    pub(crate) fn start_mcp(&mut self, context: &egui::Context) -> std::io::Result<()> {
        self.mcp = Some(crate::mcp::UiBridge::start(context.clone())?);
        Ok(())
    }
    /// Routes SIGTERM into the close guard (#261, PROVISIONAL). Opt-in per process: tests and
    /// headless editors never register a handler.
    pub fn install_shutdown_signal(&mut self, ctx: &egui::Context) -> std::io::Result<()> {
        self.shutdown = shutdown_signal::install(ctx)?;
        Ok(())
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, crate::viewport_renderer::RenderError> {
        let renderer = crate::viewport_renderer::ViewportRenderer::initialize(cc)?;
        let mut style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        style.visuals.panel_fill = Color32::from_gray(48);
        style.visuals.window_fill = Color32::from_gray(52);
        style.visuals.selection.bg_fill = Color32::from_rgb(58, 92, 120);
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        Ok(Self {
            renderer: Some(renderer),
            ..Self::default()
        })
    }
}

impl eframe::App for Editor {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let pointer_held = ctx.input(|input| input.pointer.any_down())
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerButton { pressed: true, .. }));
        self.native_input_pending |= input.events.iter().any(|event| match event {
            egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_) => pointer_held,
            egui::Event::Key { pressed, .. } => *pressed,
            egui::Event::Copy
            | egui::Event::Screenshot { .. }
            | egui::Event::ModifiersChanged(_)
            | egui::Event::WindowFocused(_)
            | egui::Event::PointerGone => false,
            egui::Event::Cut
            | egui::Event::Paste(_)
            | egui::Event::Text(_)
            | egui::Event::PointerButton { .. }
            | egui::Event::Zoom(_)
            | egui::Event::Rotate(_)
            | egui::Event::Ime(_)
            | egui::Event::Touch { .. }
            | egui::Event::MouseWheel { .. }
            | egui::Event::AccessKitActionRequest(_) => true,
        });
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().visible()) == Some(false) {
            self.move_viewport = None;
            if self
                .history
                .active_manipulation()
                .is_some_and(|active| active.provenance == ManipulationProvenance::Native)
                && let Err(error) = self.cancel_move()
            {
                self.set_error_status(&error);
                self.error = true;
            }
            self.handle_close(ctx);
            #[cfg(feature = "mcp")]
            self.process_mcp(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

impl Editor {
    #[cfg(feature = "mcp")]
    fn process_mcp(&mut self, ctx: &egui::Context) {
        if let Some(mut bridge) = self.mcp.take() {
            if !self.native_input_pending && bridge.take_close_request() {
                self.request_close();
                ctx.request_repaint();
            }
            bridge.dispatch(self, ctx);
            self.mcp = Some(bridge);
        }
    }
}

/// The one reason shown on every disabled control that has no behaviour yet.
const NOT_IMPLEMENTED_YET: &str = "Not implemented yet";

fn unsupported(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_enabled(false, egui::Button::new(label))
        .on_disabled_hover_text(NOT_IMPLEMENTED_YET)
}

fn text_field_ids() -> impl Iterator<Item = egui::Id> {
    [
        name_edit_id(),
        material_edit_id(),
        features::manipulation_tool::manipulation_value_id(),
    ]
    .into_iter()
    .chain((0..9).map(channel_edit_id))
    .chain(features::primitive_options::option_field_ids())
    .chain(features::rigging::joint_field_ids())
}

fn text_edit_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| {
        memory
            .focused()
            .is_some_and(|id| text_field_ids().any(|field| field == id))
    })
}

fn name_edit_id() -> egui::Id {
    egui::Id::new("node_name")
}

fn channel_edit_id(index: usize) -> egui::Id {
    egui::Id::new(("channel", index))
}

fn material_edit_id() -> egui::Id {
    egui::Id::new("material_diffuse")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../snapshots.rs"]
pub(crate) mod snapshots;

#[cfg(test)]
#[path = "../entry_point_tests.rs"]
mod entry_point_tests;

#[cfg(test)]
#[path = "../guard_matrix_tests.rs"]
mod guard_matrix_tests;

#[cfg(all(test, feature = "mcp"))]
#[path = "../mcp_tests/mod.rs"]
mod mcp_editor_tests;

#[cfg(test)]
#[path = "../gpu_viewport_tests.rs"]
mod gpu_viewport_tests;

#[cfg(test)]
mod rotated_scale_tests;
