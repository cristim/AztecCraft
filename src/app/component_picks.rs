//! The remembered face, vertex and edge picks, readable and writable only through `Editor`
//! accessors so their storage can change without touching readers.

use super::Editor;
use azteccraft::native_scene::{EdgeTarget, FaceTarget, VertexTarget};

#[derive(Default)]
pub(super) struct ComponentPicks {
    face: Option<FaceTarget>,
    vertex: Option<VertexTarget>,
    edge: Option<EdgeTarget>,
}

impl Editor {
    pub(super) fn remembered_face(&self) -> Option<FaceTarget> {
        self.picks.face.clone()
    }

    pub(super) fn remembered_vertex(&self) -> Option<VertexTarget> {
        self.picks.vertex.clone()
    }

    pub(super) fn remembered_edge(&self) -> Option<EdgeTarget> {
        self.picks.edge.clone()
    }

    pub(super) fn set_remembered_face(&mut self, target: Option<FaceTarget>) {
        self.picks.face = target;
    }

    pub(super) fn set_remembered_vertex(&mut self, target: Option<VertexTarget>) {
        self.picks.vertex = target;
    }

    pub(super) fn set_remembered_edge(&mut self, target: Option<EdgeTarget>) {
        self.picks.edge = target;
    }

    pub(super) fn clear_remembered_components(&mut self) {
        self.picks = ComponentPicks::default();
    }
}
