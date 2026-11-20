//! The RMB mesh marking menu in the viewport (issue #32, slice T2b): the entry layout, the
//! per-frame input pass that drives `azteccraft::marking_menu::Interaction`, and the dispatch.
//!
//! The menu holds no scene state. A chosen mode becomes the same `SelectionModeIntent` the Status
//! Line and the F8 to F11 keys produce, so the draft, read-only and release-frame drag rules are
//! shared; Extrude opens the Extrude Options window the shelf button opens. Marking menus are a
//! UI widget with no MCP tool of their own: every enabled entry maps to an existing tool (see
//! `MeshAction::mcp_tool`), and a held or open menu makes remote mutations return `BusyEditing`.
//!
//! The layout is original and PROVISIONAL: the Maya 2027 Marking Menus page describes the
//! gestures and the Ctrl, Shift and hotkey variants but does not give the polygon menu's
//! positions or entries.

mod layout;

#[cfg(test)]
mod layout_tests;
