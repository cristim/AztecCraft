pub(in crate::app) mod attribute_editor;
mod camera;
mod camera_bookmarks;
pub(in crate::app) mod extrude_options;
#[cfg(any(test, feature = "mcp"))]
pub(super) mod gltf_export;
mod history;
pub(in crate::app) mod manipulation_tool;
mod material;
pub(in crate::app) mod move_tool;
mod node_edit;
pub(super) mod obj_export;
pub(super) mod obj_import;
#[cfg(any(test, feature = "mcp"))]
mod recovery;

pub(in crate::app) use node_edit::NameStyle;
pub(in crate::app) mod marquee;
pub(in crate::app) mod primitive_options;
pub(in crate::app) mod rigging;
mod selection;
mod status_line;
pub(in crate::app) mod timeline;
pub(in crate::app) mod uv;
