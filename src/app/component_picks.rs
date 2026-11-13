//! Accessors for the remembered face, vertex and edge picks.

use super::Editor;
use azteccraft::native_scene::{EdgeTarget, FaceTarget, VertexTarget};

impl Editor {
    pub(super) fn remembered_face(&self) -> Option<FaceTarget> {
        self.remembered_face.clone()
    }

    pub(super) fn remembered_vertex(&self) -> Option<VertexTarget> {
        self.remembered_vertex.clone()
    }

    pub(super) fn remembered_edge(&self) -> Option<EdgeTarget> {
        self.remembered_edge.clone()
    }

    pub(super) fn set_remembered_face(&mut self, target: Option<FaceTarget>) {
        self.remembered_face = target;
    }

    pub(super) fn set_remembered_vertex(&mut self, target: Option<VertexTarget>) {
        self.remembered_vertex = target;
    }

    pub(super) fn set_remembered_edge(&mut self, target: Option<EdgeTarget>) {
        self.remembered_edge = target;
    }
}
