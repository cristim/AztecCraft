# Maya 2026 UI parity inventory

Research date: 2026-10-16. Main SHA: ef86b64. Evidence was gathered at that SHA, and `src/app.rs` has since become the `src/app/` module directory. Citations use a file plus a quoted symbol or string anchor (`path "anchor"`, with `\"` for a literal quote), checked against main at 112f60d by grepping the anchor in the cited file, and each anchor is expected to match exactly once; older `path:NN "anchor"` citations keep their line number only as a hint that may have drifted. Rows citing `src/app/menus.rs` are at d70edd2 (branch feat/59-status-file-buttons; merged later via the Status Line PRs). All rows citing `src/app/features/status_line.rs` are at 57cf561 (branch feat/59-hierarchy-selection-mode).

Umbrella issue: #71. Slice labels S1 to S15 map to the GitHub issues and PR listed in the Prioritized backlog.

Precedence: [inventory-seed.md](inventory-seed.md) and [parity.md](parity.md) own the functional denominator. [mcp.md](mcp.md) owns MCP coverage. This document owns only the UI-surface backlog. Its MCP column points to tool names and does not define coverage. No percentage is reported here, and nothing here claims that AztecCraft matches Maya.

## How to read this

- Source: an Autodesk help page. FALLBACK means a 2016, 2024 or 2025 page was used. UNCONFIRMED means the research did not confirm the fact in a 2026 page.
- Status: Supported means the element exists and does something. It does not mean parity. Partial means the control exists but does only part of the documented role. Visible-unsupported means a disabled or placeholder control is drawn. Missing means nothing is drawn.
- Evidence: required for Supported and Visible-unsupported rows. Missing rows say what was checked.
- Priority: P1 is frequent and blocking in a modeler's daily loop. P2 is regular use. P3 is occasional or outside the modeling loop.
- Visible-unsupported rows confirmed in a native macOS screenshot on 2026-10-16 of a build whose application code is identical to ef86b64: Tool Box Move/Rotate/Scale (and a Snap placeholder removed since), shelf Extrude/Bevel/Materials, five Time Slider buttons. The screenshot shows the menu-bar labels only, with no menu open, so menu contents were checked from source. Rows tagged source-verified are not shown in that screenshot.
- On this SHA, Move, Rotate and Scale in the Tool Box are disabled. PR #77 (Move) is not included. The Tool Box rows in section 4 were updated for the icon-only Tool Box change and are cited at that branch.
- MCP is ahead of the native UI; the shelf Extrude is deliberately disabled (known-issues.md). Enabling it is a backlog item.
- Names of tools come from `src/mcp/tools.rs`.


## 1. Main menus and menu sets

Menu bar draws File, Edit, Create, Select, Modify, Display, Windows, then the nine Modeling menu set menus (Mesh, Edit Mesh, Mesh Tools, Mesh Display, Curves, Surfaces, Deform, UV, Generate), then Help. [menus] gives the first seven in that order and says the Modeling set is added after them; [modeling-menus] lists the nine topics but states no ordering rule, so their order is PROVISIONAL. Help is not among the seven and its position is UNCONFIRMED, so it stays last. The bar is `src/app/menus.rs "pub(super) const ALWAYS_ON_MENUS"` plus `src/app/menus.rs "pub(super) const MODELING_SET_MENUS"` plus `src/app/menus.rs "pub(super) const HELP_MENU"`. The Modeling set label at the shelf is static text; only the Modeling set is active, so its menus are always drawn.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| File menu | [menus] | Supported | `src/app/menus.rs "ui.menu_button(\"File\""` (New scene, Open, Save, Save As, Close); Export at `src/app/menus.rs "Export..."` is Visible-unsupported (source-verified); Save: current main also disables these for read-only curve scenes (after ef86b64) | `new_scene`, `open_scene`, `save_scene`, `save_scene_as`, `resolve_file_action`, `request_close` | P1 | S6 Menu content (#66) |
| Edit menu | [menus] | Supported | `src/app/menus.rs "ui.menu_button(\"Edit\""` (Undo, Redo, Delete; Delete by Type and Delete All by Type are Visible-unsupported rows with a hover reason, #187; Delete of a node with children or a parent returns the typed `unsupported_delete` refusal) | `undo`, `redo`, `delete_node` | P1 | S6 Menu content (#66) |
| Create menu | [menus] | Supported | `src/app/menus.rs "ui.menu_button(\"Create\""` (Polygon cube, plane and provisional sphere, cylinder and cone; other primitives at `src/app/menus.rs "Other primitives"` are Visible-unsupported, source-verified; current main also disables these for read-only curve scenes (after ef86b64)) | `create_primitive` | P1 | S6 Menu content (#66) |
| Select menu | [menus] | Supported | src/app/menus.rs `menu_button("Select"` (Object/Component F8, Vertex F9, Edge F10, Face F11) | `set_selection_mode`, `select_node`, `select_face`, `select_vertex`, `select_edge` | P1 | S3 Component modes (#29) |
| Select > Components: Vertex, Edge, UV | [select] | Partial (Vertex F9, Edge F10 and Face F11 entries; UV visible-unsupported) | src/app/menus.rs "Vertex   F9", "Edge   F10", "Face   F11", "UVs" | `set_selection_mode` `vertex`/`edge`, `select_vertex` and `select_edge` | P1 | S3 Component modes (#29) |
| Shift/Ctrl click add/toggle components | [select] | Missing (modified click refused; the refusal message is returned as an error) | src/viewport_ui.rs ModifiedClick refusal (`panels.rs` dispatches via `unsupported_selection()`) in Vertex, Edge and Face modes; selection unchanged | none (`select_vertex`/`select_edge` select one component) | P1 | S3 Component modes (#29) |
| Modify menu | [menus] | Visible-unsupported | `src/app/menus.rs "Modify"` (disabled placeholder entry; source-verified) | none | P1 | S6 Menu content (#66) |
| Display menu | [menus] | Visible-unsupported | `src/app/menus.rs "fn placeholder_menu"` draws it as a disabled placeholder (one disabled "Not implemented yet" row; source-verified) between Modify and Windows | none | P2 | S6 Menu content (#66) |
| Display menu items (Heads-up display, UI Elements and others) | [displaymenu]: the Display menu lists Grid, Heads Up Display (submenu), Hide (submenu), Show (submenu), Toggle Show/Hide, Per Camera Visibility (submenu), Wireframe Color, Object Display, Transform Display, Polygons, NURBS, Animation and Rendering (submenus), then Frame All in All Views, Frame Selection in All Views, Frame Selection with Children in All Views. Which menu set shows this page's list is UNCONFIRMED. A UI Elements entry is not in the 2026 list, UNCONFIRMED. 2027 differences UNCONFIRMED (2027 page not read) | Missing | absent; the Display placeholder holds only a disabled Grid Options row (#160) with the default-grid reason | none | P2 | S6 Menu content (#66) |
| Windows menu | [menus] | Visible-unsupported | `src/app/menus.rs "fn placeholder_menu"` (disabled placeholder row; source-verified) | none | P2 | S6 Menu content (#66) |
| Modeling menu set (default, UNCONFIRMED as default) | [menus] UNCONFIRMED | Partial | `src/app/features/status_line.rs:169 "ComboBox::from_id_salt(\"menu_set\")"` selector with Modeling active; see the Status Line 'Menu set drop-down' row | none | P2 | S1 Status Line strip (#59) |
| Rigging menu set | [menus] | Missing | no set switching; the Status Line selector disables the other sets (`src/app/features/status_line.rs "for set in MENU_SETS"`) | none | P3 | S6 Menu content (#66) |
| Animation menu set | [menus] | Missing | no set switching; the Status Line selector disables the other sets (`src/app/features/status_line.rs "for set in MENU_SETS"`) | none | P3 | S6 Menu content (#66) |
| FX menu set (page also says Dynamics) | [menus] | Missing | no set switching; the Status Line selector disables the other sets (`src/app/features/status_line.rs "for set in MENU_SETS"`) | none | P3 | S6 Menu content (#66) |
| Rendering menu set | [menus] | Missing | Rendering is a menu set, not a menu; the Render placeholder menu was removed and the Status Line selector disables the set (`src/app/features/status_line.rs "for set in MENU_SETS"`) | none | P3 | S6 Menu content (#66) |
| Per-set menu lists: the nine Modeling set menus Mesh, Edit Mesh, Mesh Tools, Mesh Display, Curves, Surfaces, Deform, UV, Generate | [modeling-menus]; order PROVISIONAL | Visible-unsupported | `src/app/menus.rs "pub(super) const MODELING_SET_MENUS"` drawn by `src/app/menus.rs "fn placeholder_menu"`: each opens to one disabled "Not implemented yet" row, no invented items; menu contents UNCONFIRMED; source-verified | none | P2 | S6 Menu content (#66) |
| Help menu position | [menus] lists no Help menu; position UNCONFIRMED | Supported (content is prototype text only) | `src/app/menus.rs "pub(super) const HELP_MENU"` is drawn last, after the Modeling set menus | none | P3 | S6 Menu content (#66) |
| Menu set switching by F2 to F6 | [menus]; [hotkeys-2025] FALLBACK 2025 lists F2, F3, F4, F6 only | Missing | no handler in Editor::shortcuts (`src/app/shortcuts.rs "fn shortcuts"`); deferred | none | P3 | S5 Hotkeys (#32) |
| Custom sets through Menu Set Editor | [menuset-editor] | Missing | absent | none | P3 | S6 Menu content (#66) |
| Menu bar toggle Ctrl+M | [hotkeys-2025] FALLBACK 2025 | Missing | absent | none | P3 | S5 Hotkeys (#32) |
| Application Home icon | [overview] | Missing | absent | none | P3 | S6 Menu content (#66) |

## 2. Status Line

The AztecCraft window draws a partial Status Line strip, from `Editor::status_line` (`src/app/features/status_line.rs:98 "pub(in crate::app) fn status_line"`). Groups without AztecCraft behavior are not drawn. See known-issues.md, "Status Line is a partial subset".

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Menu set drop-down | [statusline] | Partial | `src/app/features/status_line.rs:168 "fn menu_set_selector"`; only Modeling is active, the other four sets are disabled; native-only, no state | none | P2 | S1 Status Line strip (#59) |
| File buttons: new, open, save | [statusline] | Supported | `src/app/features/status_line.rs:107 "self.file_actions()"`, shared with the File menu at `src/app/menus.rs:27 "fn file_actions"`; Save As stays in the File menu | `new_scene`, `open_scene`, `save_scene` | P2 | S1 Status Line strip (#59) |
| Selection mode menu (presets) | [statusline] | Visible-unsupported | `src/app/features/status_line.rs:12 "Selection mode menu (presets): not implemented yet"`; disabled combo box with no state; real presets need masks | none | P1 | S3 Component modes (#29) |
| Hierarchy mode button | [statusline] | Partial | `src/app/features/status_line.rs:33 "Hierarchy selection (Root only; Leaf and Template unsupported)"`; Root sub-mask only, a viewport click selects the hit hierarchy's root transform; Leaf and Template unsupported | `set_selection_mode` | P1 | S1 Status Line strip (#59) |
| Object mode button | [statusline] | Supported | `src/app/features/status_line.rs:39 "Object selection (F8 toggles)"` | `set_selection_mode` | P1 | S1 Status Line strip (#59) |
| Component mode button | [statusline] | Partial | `src/app/features/status_line.rs:45 "Component selection (keeps the current component mode; from Object or Hierarchy enters Face; pick Vertex, Edge or Face"`; the button enters Face from Object and Hierarchy, keeps the mode in Vertex, Edge and Face, and stays pressed in all three | `set_selection_mode` | P1 | S1 Status Line strip (#59) |
| Component-type buttons: Vertex, Edge, Face | [statusline] | Partial | `src/app/features/status_line.rs "const COMPONENT_TYPES"`; three original icons dispatching the F9/F10/F11 typed operations; order provisional (no fetched Maya 2026 page names the icons or order); exclusive modes, not Maya's per-type mask toggles; UV and Vertex Face unsupported; no UV button drawn; like F8 to F11 and the other mode buttons they cancel an active native Move, Rotate or Scale manipulation instead of being disabled, but are disabled while a remote manipulation is active | `set_selection_mode` | P1 | S3 Component modes (#29) |
| Selection mask buttons (per object or component type) | [statusline] | Missing | absent; the Vertex, Edge and Face buttons above are exclusive modes, not per-type mask toggles | none | P1 | S3 Component modes (#29) |
| Lock/unlock current selection | [statusline] | Missing | absent | none | P3 | S1 Status Line strip (#59) |
| Highlight Selection mode | [statusline] | Missing | absent | none | P3 | S1 Status Line strip (#59) |
| Snap buttons: grids, curves, points, Projected Center, view planes | [statusline] | Missing in the Status Line | absent from the Status Line strip (`src/app/features/status_line.rs "const COMPONENT_TYPES"` draws only the component-type buttons); the former Tool Box 'Snap' placeholder was removed because the Tool Box page ([toolbox]) lists seven tools and snapping is a Status Line group per [statusline] | none | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Make Live and live-surface field | [statusline] | Missing | absent | none | P3 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Symmetry setting | [statusline] | Missing | absent | none | P2 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Construction history Inputs/Outputs menus and toggle | [statusline] | Missing | absent from the strip; a disabled INPUTS stub exists in the Channel Box | none | P2 | S4 Attribute Editor and Channel Box (#60) |
| Render buttons: Render View, render, IPR, render settings | [statusline] | Missing | absent | none | P3 | S6 Menu content (#66) |
| Hypershade button | [statusline] | Missing | absent | none | P3 | S6 Menu content (#66) |
| Input box (Absolute, Relative, Rename, Select by name) | [statusline] | Missing | absent; the Channel Box has per-field edits instead | `edit_node`, `rename_node` | P2 | S1 Status Line strip (#59) |
| Workspace selector | [overview] | Missing | absent from the window; no entry in the menu bar (`src/app/menus.rs "pub(super) const ALWAYS_ON_MENUS"`) or the shelf (`src/app/menus.rs "UiIcon::PolyCube"`) | none | P2 | S14 Docking and workspaces (#67) |
| Sidebar toggle: Modeling Toolkit | [statusline] | Visible-unsupported | `src/app/features/status_line.rs:55 "Modeling Toolkit: not implemented yet"` | none | P2 | S1 Status Line strip (#59) |
| Sidebar toggle: HumanIK | [statusline] | Visible-unsupported | `src/app/features/status_line.rs:61 "HumanIK: not implemented yet"` | none | P3 | S1 Status Line strip (#59) |
| Sidebar toggle: Attribute Editor | [statusline] | Supported | `src/app/features/status_line.rs:67 "Show or hide the Attribute Editor"`; shows the Attribute Editor and hides the Channel Box; disabled during an active manipulation | `set_sidebar_panel_visibility` | P1 | S1 Status Line strip (#59) |
| Sidebar toggle: Tool Settings | [statusline] | Visible-unsupported | `src/app/features/status_line.rs:73 "Tool Settings: not implemented yet"` | none | P2 | S1 Status Line strip (#59) |
| Sidebar toggle: Channel Box / Layer Editor | [statusline] | Partial | `src/app/features/status_line.rs "Show or hide the Channel Box (Layer Editor: not implemented yet)"`; no Layer Editor exists, so it shows or hides the Channel Box only and the hover says so; disabled during an active manipulation | `set_sidebar_panel_visibility` | P1 | S1 Status Line strip (#59) |

## 3. Shelf

One fixed shelf row exists under the menu bar. Tab order and the Poly Modeling tab contents are UNCONFIRMED in the report.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Shelf strip under the Status Line | [shelf] | Supported | `src/app/menus.rs:194 "ui.horizontal"` (one fixed row, no label; the static Modeling label was removed when the Status Line selector landed) | none | P1 | S7 Shelf (#65) |
| Shelf tabs and tab drop-down | [shelf] | Missing | absent; single hard-coded row | none | P2 | S7 Shelf (#65) |
| Create > Polygon Primitives option boxes (per-primitive options window with Create button) | [menus]; Create-menu page 2025 FALLBACK (2026 equivalent not confirmed) | Partial | each of the five Create rows has an option box that opens a non-modal "Polygon <name> Options" window (size, subdivision and axis fields validated like MCP, Create and Close; Free and Camera axis, Interactive creation, Create UVs, Exit on completion, cap divisions and Round cap are disabled with reasons; the numeric fields are text-only, with no drag-to-scrub or sliders as in Maya, PROVISIONAL gap); native visual QA pending; the shelf buttons have none, as in Maya (PROVISIONAL) | `create_primitive` `options` (the window is a presentation of it, no extra tool) | P1 | S6 Menu content (#66) |
| Poly Modeling tab: primitive creation icons (cube) | [shelf] UNCONFIRMED contents | Supported | `src/app/menus.rs "UiIcon::PolyCube"`; current main also disables these for read-only curve scenes (after ef86b64) | `create_primitive` | P1 | S7 Shelf (#65) |
| Poly Modeling tab: primitive creation icons (plane) | [shelf] UNCONFIRMED contents | Supported | `src/app/menus.rs "button(ui, \"Plane\""`; current main also disables these for read-only curve scenes (after ef86b64) | `create_primitive` | P1 | S7 Shelf (#65) |
| Other primitive icons (sphere, cylinder and others) | [shelf] UNCONFIRMED contents | Visible-unsupported | no shelf icon; Create > Polygon sphere, cylinder and cone exist (provisional, #43), other primitives remain the "Other primitives" menu placeholder | none | P2 | S7 Shelf (#65) |
| Poly Modeling tab: Extrude icon | [shelf] UNCONFIRMED contents | Visible-unsupported | MCP ahead of native UI; native control deliberately disabled (known-issues.md, `src/app/menus.rs "egui::Button::new(\"Extrude\")"`) | `extrude_face` | P1 | S7 Shelf (#65) |
| Poly Modeling tab: Bevel icon | [shelf] UNCONFIRMED contents | Visible-unsupported | `src/app/menus.rs "Bevel"` (disabled) | none | P2 | S7 Shelf (#65) |
| Materials icon | [shelf] UNCONFIRMED contents | Visible-unsupported | `src/app/menus.rs "Materials"` (disabled) | none | P3 | S7 Shelf (#65) |
| Undo and redo buttons: AztecCraft-native shelf buttons; not documented on the Maya Status Line (Maya has Edit > Undo and Redo) | [menus] | Supported | `src/app/menus.rs "UiIcon::Undo"`, `src/app/menus.rs "UiIcon::Redo"` | `undo`, `redo` | P1 | S7 Shelf (#65) |
| Content Browser icon on Sculpting and Poly Modeling shelves | [content-browser-2024] FALLBACK 2024 | Missing | absent | none | P3 | S7 Shelf (#65) |
| Other default tabs: Curves/Surfaces, Sculpting, Rigging, Animation, Rendering, FX and more | [shelf] UNCONFIRMED list | Missing | absent | none | P3 | S7 Shelf (#65) |
| Shelf Editor (Shelves, Command, Double Click Command, Popup Menu Items) | [shelf-editor] | Missing | absent | none | P3 | S7 Shelf (#65) |

## 4. Tool Box and quick layouts

The Tool Box rows below reflect the icon-only Tool Box change (feat/70-wire-ui-icons): Select, Lasso, Paint Selection, Move, Rotate, Scale and Last Used in the documented order, with labels in hover text and AccessKit. Select, Move, Rotate and Scale are live through `set_active_tool`; the other three are disabled. The Tool Box draws exactly these seven icon buttons: no "Tools" label and no Snap button, matching the Tool Box page (seven tools, no label, no Snap); the Snap row in section 2 tracks the Status Line group. Cites in those rows are at that branch, not ef86b64.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Select tool (Q; Q-click Selection Mask marking menu) | [toolbox] | Supported | src/app/panels.rs "Some(ActiveTool::Select)" (icon-only button, hover "Select tool (Q)" at src/app/panels.rs; Q-click marking menu unsupported) | `set_active_tool`, `select_node` | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Lasso tool | [toolbox] | Visible-unsupported | src/app/panels.rs "\"Lasso\", UiIcon::Lasso" (disabled, hover "not implemented yet") | none | P2 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Paint Selection tool | [toolbox] | Visible-unsupported | src/app/panels.rs "UiIcon::PaintSelect" (disabled, hover "not implemented yet") | none | P3 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Move tool (W) | [toolbox] | Supported | src/app/panels.rs "Some(ActiveTool::Move)" (icon-only button, hover "Move tool (W)" at src/app/panels.rs); Axis Orientation selector fixed at World (Maya 2026 Move Tool default) with the other seven documented orientations listed disabled (`src/app/features/manipulation_tool.rs "MOVE_ORIENTATIONS"`) | `set_active_tool`, `begin_manipulation` (`kind` `move`), `update_manipulation` (`distance`), `finish_manipulation` | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Rotate tool (E) | [toolbox] | Supported | src/app/panels.rs "Some(ActiveTool::Rotate)" (icon-only button, hover "Rotate tool (E)"); per-tool Object/World selector (default Object, documented in the Maya 2026 Rotate Tool settings page) and numeric row in src/app/features/manipulation_tool.rs | `set_active_tool`, `begin_manipulation` (`kind` `rotate`), `update_manipulation` (`angle`), `finish_manipulation` | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Scale tool (R) | [toolbox] | Supported | src/app/panels.rs "Some(ActiveTool::Scale)" (icon-only button, hover "Scale tool (R)"); per-tool Object/World selector (default World, documented in the Maya 2026 Scale Tool settings page) and numeric row in src/app/features/manipulation_tool.rs | `set_active_tool`, `begin_manipulation` (`kind` `scale`), `update_manipulation` (`factor`), `finish_manipulation` | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Last Used tool (Y) | [toolbox] | Visible-unsupported | src/app/panels.rs "UiIcon::LastUsed" (disabled, hover "not implemented yet"; dynamic last-used behaviour unsupported) | none | P2 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Show Manipulators (T) | [overview] | Missing | absent | none | P2 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| Quick Layout: single pane | [quicklayout] | Missing | absent; fixed layout of `egui::Panel` calls in `src/app/panels.rs "timeline_status"` | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Quick Layout: four panes | [quicklayout] | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Quick Layout: two panes side by side | [quicklayout] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Quick Layout: Outliner button | [quicklayout] | Missing | Outliner is always docked at `src/app/panels.rs "self.outliner(ui)"`; no button | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Restore Tool Box (Windows > UI Elements) | [toolbox] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |

## 5. Viewport panel menus and Panel Toolbar

The viewport header (`src/app/panels.rs "persp"`) shows the camera name, the selection mode text, a Grid toggle, a Shading panel menu and a navigation hint. Shading is the only panel menu; the others in the table below are absent.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| ViewCube | [viewcube-2026]: on by default in the upper-right corner of the scene view; clicking a face, edge or corner animates the camera to one of 26 standard views, hover highlights the target, and a Home button above the cube resets to the default perspective view; hidden by right-click Hide ViewCube or the ViewCube preference, whose size, position and opacity settings also apply; dragging snaps to other positions. Also listed in Display > Heads Up Display as "Viewcube" [hud-options], default state there UNCONFIRMED. [viewcube-overview]: a navigation tool in a viewport corner, no default stated. 2027 differences UNCONFIRMED | Missing | absent from the viewport header `src/app/panels.rs "persp"` | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Heads-up display | [hud-options] (2026): Display > Heads Up Display, Ctrl keeps the submenu open. Order: Animation Details, Cache, Camera Names, Caps Lock, Current Asset, Current Frame, Evaluation, EM State, Manipulation, GPU Override, Memory Limit, Focal Length, Game Vertex Count, Frame Rate, HIK Details, Material Loading Details, Object Details, Origin Axis, Particle Count, Poly Count (Cage and Smooth Mesh Preview sub-options), Scene Timecode, Select Details, Symmetry, View Axis, Viewcube, Viewport Renderer, In-view Messages, XGen Info, In-View Editors. Documented on by default: Camera Names, Caps Lock, Symmetry, View Axis, In-view Messages, In-View Editors; documented off: Animation Details, Current Frame, Focal Length, Frame Rate, HIK Details, Object Details, Origin Axis, Poly Count, Scene Timecode, Viewport Renderer; the rest UNCONFIRMED. [hud-panel]: panel Show > HUD toggles all HUD items in that panel; default of the panel toggle UNCONFIRMED. 2027 differences UNCONFIRMED | Missing | absent from the viewport header `src/app/panels.rs "persp"` | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: View | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P1 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Shading | [shadingmenu] (19 items; reached from [panelmenus]) | Partial | `src/app/panels.rs "ui.menu_button(\"Shading\""`: Wireframe and Smooth Shade All work (the [hotkeys-2026] page names hotkey 4 "Shading > Wireframe" and 5 "Shaded display"); the disabled rows with a reason follow the [shadingmenu] order: Use default material, Wireframe on Shaded, Hardware Texturing (6; [hotkeys-2026] names hotkey 6 "Shaded and Textured display", [paneltoolbar] calls the button Textured); the other 14 listed items (Smooth Shade Selected Items, Flat Shade All, Flat Shade Selected Items, Bounding Box, X-Ray, X-Ray Joints, X-Ray Active Components, Cycle rig display mode, Color Index Mode, Backface Culling, Smooth Wireframe, Hardware Fog, Depth of Field, Apply Current to All) are not drawn; Use All Lights (7; [hotkeys-2026] "Lighting > Use All Lights") has no Shading row because the 2026 menu page lists it under Lighting; hotkeys 6 and 7 only set a status message (`src/app/shortcuts.rs "Use All Lights (7)"`); visible labels follow [shadingmenu], hotkey names follow [hotkeys-2026]. Lighting is `DefaultLightingApprox`, a per-face headlight Lambert term on the CPU (`src/shading.rs "DefaultLightingApprox"`), not verified Maya default lighting; no parity claim. Edge lines are always drawn in Smooth Shade All (`src/viewport_renderer.rs "corners[(index + 1) % corners.len()]"`) whereas Maya's default is off, as `.project-docs/parity.md` records | `set_shading_mode`; `shading` in inspection (`src/editor_operations.rs "pub shading: ShadingMode"`) | P1 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Lighting | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Show | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Renderer | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Panels | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel menu: Stereo | [panelmenus] contents UNCONFIRMED | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: View (Select Camera, Lock Camera, Bookmarks, Image Plane, 2D Pan/Zoom, Blue Pencil) | [paneltoolbar] | Missing | only the camera name label at `src/app/panels.rs "persp"` | `orbit_camera`, `pan_camera`, `dolly_camera`, `frame_selected` | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Camera Settings, Grid | [paneltoolbar] | Supported | `src/app/panels.rs "egui::Button::new(\"Grid\")"` (Grid toggle) | `set_grid_visibility` | P1 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Camera Settings, others (Film Gate, Resolution Gate, Gate Mask, Field Chart, Safe Action, Safe Title) | [paneltoolbar] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Shading (Wireframe, Smooth Shade All, Default Material, Wireframe on Shaded, Textured) | [paneltoolbar] | Partial | one Shading menu in the panel header (not a toolbar group), whose rows follow the same order as the toolbar (Use default material, Wireframe on Shaded, then Hardware Texturing, the menu label for the toolbar's Textured); Wireframe and Smooth Shade All work, the rest are disabled rows; hotkeys 4 and 5 are confirmed by [hotkeys-2026]; 6 ("Shaded and Textured display") and 7 ("Lighting > Use All Lights") report unsupported | `set_shading_mode` | P1 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Lighting (Use All Lights, Shadows) | [paneltoolbar] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Isolate Select | [paneltoolbar] | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Viewport 2.0 effects (SSAO, Motion blur, MSAA, Depth of field) | [paneltoolbar] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: X-Ray (X-Ray, X-Ray Active Components, X-Ray Joints) | [paneltoolbar] | Missing | absent | none | P2 | S8 Panels and quick layouts (#48 / #69) |
| Panel Toolbar group: Color Management (Exposure, Gamma, View Transform) | [paneltoolbar] | Missing | absent | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Toggle panel menus (Shift+M) and Panel Toolbar (Ctrl+Shift+M) | [panelvis] | Missing | absent | none | P3 | S5 Hotkeys (#32) |

## 6. Channel Box and Layer Editor

The Channel Box panel docks right at `src/app/panels.rs "Some(SidebarPanel::ChannelBox) =>"` and draws from Editor::channels (`src/app/features/node_edit.rs "fn channels"`).

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Channel Box panel, right side | [channelbox] | Supported | `src/app/panels.rs "Some(SidebarPanel::ChannelBox) =>"`, `src/app/features/node_edit.rs "fn channels"` | `inspect_scene` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Object name field (the section heading, editable in place; Attribute Editor keeps a visible label row) | [channelbox] | Supported | `src/app/features/node_edit.rs "fn name_field"`; the heading now carries an explicit accessible name "Object name" (improved over the unlabelled base field; Attribute Editor: visible label, no accessible name on the field) | `rename_node`, `edit_node` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Transform channels (Translate, Rotate, Scale X/Y/Z) | [channelbox] | Supported | `src/app/features/node_edit.rs "Translate X"` | `set_transform`, `edit_node` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Visibility channel | [channelbox] | Partial | read-only row for the selected transform's own `.v` (on/off/unknown), disabled with "visibility edit not implemented"; src/app/features/node_edit.rs | `inspect_scene` (`nodes[].visible`, read-only) | P1 | S4 Attribute Editor and Channel Box (#60) |
| SHAPES section | [channelbox] | Visible-unsupported | `src/app/features/node_edit.rs "unsupported_section"` (disabled stub, no shape data; the nearby `src/app/features/material.rs "Materials"` is the Materials block with Diffuse) | `set_material_diffuse` | P2 | S4 Attribute Editor and Channel Box (#60) |
| INPUTS section | [channelbox] | Visible-unsupported | `src/app/features/node_edit.rs "unsupported_section"` (disabled stub; source-verified) | none | P2 | S4 Attribute Editor and Channel Box (#60) |
| OUTPUTS section | [channelbox] | Visible-unsupported | `src/app/features/node_edit.rs "unsupported_section"` (disabled stub) | none | P3 | S4 Attribute Editor and Channel Box (#60) |
| Channel Box feedback for selected vertices or edges (component attributes, coordinates) | [channelbox] UNCONFIRMED for Maya 2026 | Missing | in Vertex, Edge and Face modes the Channel Box (`Editor::channels`, src/app/features/node_edit.rs) shows the parent object's name, Translate/Rotate/Scale rows, material controls and Delete, with no component-specific rows; nothing about the picked component is shown | none | P2 | S3 Component modes (#29) |
| Channel Box blank with no selection; reason, curves note and appearance summary on the Help Line row | [channelbox] | Supported | `src/app/features/node_edit.rs "fn help_line"` (the appearance summary is now collapsed by default under "Scene diagnostics"), `src/app/panels.rs "timeline_status"`; read-only fields also explain themselves on hover | `inspect_scene` (`read_only`, `source_appearance`) | P1 | S4 Attribute Editor and Channel Box (#60) |
| Layer Editor panel (stub heading in the Channel Box) | [layers] | Visible-unsupported | `src/app/features/node_edit.rs "unsupported_section"`; no tabs or layers; the Layer Editor sitting in the Channel Box by default is PROVISIONAL (inferred from the "Show Layer Editor within Channel Box" option); each stub has a distinct accessible name, for example "Layers: not implemented yet" | none | P2 | S9 Layer Editor (#61) |
| Layer Editor: Display Layers tab | [layers] | Missing | absent | none | P2 | S9 Layer Editor (#61) |
| Layer Editor: Animation Layers tab | [layers] | Missing | absent | none | P3 | S9 Layer Editor (#61) |

## 7. Attribute Editor

A first-slice Attribute Editor exists (#60): a right dock panel that replaces the Channel Box. It is an approximation, not Maya dock or tab parity. The Transform Attributes list comes only from the Maya 2023 documentation page and is provisional.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Attribute Editor panel (Ctrl+A toggles with Channel Box) | [attr] | Partial | `Editor::attribute_editor` draws one dock panel; Ctrl+A is wired with an explicit Ctrl modifier (macOS Cmd-vs-Ctrl unverified) | `set_sidebar_panel_visibility` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Node tabs across the top | [attr] | Partial | tabs for the selected transform, its direct mesh shapes and their supported materials; connected-node tabs absent | `select_attribute_editor_tab` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Attribute list | [attr] | Partial | Transform tab: name, Translate/Rotate/Scale and a read-only Visibility row (shared Channel Box drafts); Material tab: Diffuse. A read-only Transform Attributes block lists Shear, Rotate Order and Rotate Axis in Maya's 2026 order, with the two pivot translations under a separate Pivots heading. The 2026 transform node page lists no keyable flag for them, so no Channel Box rows are added (an inference; Autodesk does not state what the Channel Box shows). Other attributes, sections, editing of those values, the rotate and scale pivots and limits are absent | `edit_node`, `set_transform`, `set_material_diffuse`, `inspect_scene` (`nodes[].transform_attributes`, read-only) | P1 | S4 Attribute Editor and Channel Box (#60) |
| Pin Tab icon | [attr] | Visible-unsupported | drawn disabled with "Pin Tab: not implemented yet" | none | P3 | S4 Attribute Editor and Channel Box (#60) |
| Default dock position (not stated for 2026) | [attr] UNCONFIRMED; [rearrange-2016] FALLBACK 2016 | Missing | absent | none | P3 | S4 Attribute Editor and Channel Box (#60) |
| Tool Settings panel (default position UNCONFIRMED for 2026) | [rearrange-2016] FALLBACK 2016; UNCONFIRMED | Missing | absent | none | P2 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |

## 8. Outliner

The Outliner is a fixed left panel at `src/app/panels.rs "self.outliner(ui)"` drawing Editor::outliner (`src/app/features/selection.rs "fn outliner"`).

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Outliner panel | [outliner] | Supported | `src/app/panels.rs "self.outliner(ui)"`, `src/app/features/selection.rs "fn outliner"` | `inspect_scene` | P1 | S10 Outliner (#68) |
| Hierarchy list with click selection | [outliner] | Supported (a row click is refused with BusyEditing while a file modal is open) | src/app/features/selection.rs `outliner` "selectable_label", dispatched through `native_operation`; modal refusal in `src/app/dispatch.rs` (#141) | `select_node` | P1 | S10 Outliner (#68) |
| Open from Windows > Outliner, Panels or Quick Layout | [outliner] | Missing | always docked; no open or close | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Display menu | [outliner] | Missing | absent | none | P2 | S10 Outliner (#68) |
| Show menu | [outliner] | Missing | absent | none | P2 | S10 Outliner (#68) |
| Render Setup filter | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |
| Search field | [outliner] | Missing | absent | none | P2 | S10 Outliner (#68) |
| Groups | [outliner] | Missing | absent | none | P2 | S10 Outliner (#68) |
| Session material assignment and base colour (#51) | [attr] | Missing | no native control; MCP supported, native UI missing, blocked on Attribute Editor B3 | `create_session_material`, `assign_session_material`, `set_session_material_color` | P2 | S4 Attribute Editor and Channel Box (#60) |
| Materials | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |
| Hidden objects | [outliner] | Missing | absent | none | P2 | S10 Outliner (#68) |
| Sets | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |
| Assets | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |
| References | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |
| Scene Assembly | [outliner] | Missing | absent | none | P3 | S10 Outliner (#68) |

## 9. Time Slider, Range Slider and playback

The bottom panel (`src/app/panels.rs "timeline_status"`) shows disabled labels and a status message. Animation is not implemented.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Time Slider (ruler, key ticks, waveform, cache stripes) | [timeslider] | Visible-unsupported | `src/app/panels.rs "Time slider"`, `src/app/panels.rs "Animation not implemented"` (source-verified) | none | P2 | S11 Timeline (#62) |
| Current time field | [timeslider] | Missing | absent | none | P2 | S11 Timeline (#62) |
| Time Slider bookmarks | [timeslider] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Go To Start | [playback] | Visible-unsupported | `src/app/panels.rs "Time slider"` (disabled labels in the loop below it) | none | P2 | S11 Timeline (#62) |
| Step buttons '<' '>' (frame or key unspecified) | [playback] | Visible-unsupported | `src/app/panels.rs "Time slider"` (disabled labels in the loop below it; '<' and '>') | none | P2 | S11 Timeline (#62) |
| Step Back Frame (Alt/Opt+,) | [playback] | Missing | no separate control | none | P2 | S11 Timeline (#62) |
| Step Back Key (,) | [playback] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Play Backwards | [playback] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Play Forwards (Alt/Opt+V); Stop (Esc) | [playback] | Visible-unsupported | `src/app/panels.rs "Time slider"` (disabled labels in the loop below it) | none | P2 | S11 Timeline (#62) |
| Step Forward Key (.) | [playback] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Step Forward Frame (Alt/Opt+.) | [playback] | Missing | no separate control | none | P2 | S11 Timeline (#62) |
| Go To End | [playback] | Visible-unsupported | `src/app/panels.rs "Time slider"` (disabled labels in the loop below it) | none | P2 | S11 Timeline (#62) |
| Range Slider: Animation Start and End | [range] | Missing | absent | none | P2 | S11 Timeline (#62) |
| Range Slider: Playback Start, End and range bar | [range] | Missing | absent | none | P2 | S11 Timeline (#62) |
| Range Slider: Bookmark icon | [range] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Range Slider: Character Set menu | [range] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Range Slider: Animation Layer menu | [range] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Framerate | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Loop | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Cached Playback | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Context View / Cut Slider | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Volume | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Auto Key | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |
| Playback option: Animation Preferences | [playopts] | Missing | absent | none | P3 | S11 Timeline (#62) |

## 10. Command Line and Help Line

The bottom message (`src/app/panels.rs "&self.status"`) is an operation status line. Row order between Command Line, Help Line and Range Slider is UNCONFIRMED in the report.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Command Line input with MEL/Python toggle | [cmdline] | Missing | absent; no scripting | none | P2 | S12 Command and Help Line (#63) |
| Command Line result box and draggable divider | [cmdline] | Missing | absent | none | P3 | S12 Command and Help Line (#63) |
| Script Editor button | [cmdline] | Missing | absent | none | P3 | S12 Command and Help Line (#63) |
| Help Line along the bottom left | [helpline] | Missing | no tool-hint line; the nearest neighbour is the operation status line at `src/app/panels.rs "&self.status"` | none | P2 | S12 Command and Help Line (#63) |

## 11. Modeling Toolkit

No Modeling Toolkit panel exists. Default dock is UNCONFIRMED for 2026.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Modeling Toolkit panel | [toolkit] | Missing | absent | none | P2 | S13 Modeling Toolkit (#64) |
| Section: menus | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Selection modes | [toolkit] | Missing | absent; Object and Face modes exist in the Select menu only | `set_selection_mode` | P2 | S13 Modeling Toolkit (#64) |
| Section: Selection options | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Selection constraints | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Transform constraints | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Commands and tools | [toolkit] | Missing | absent; Extrude is disabled at `src/app/menus.rs "egui::Button::new(\"Extrude\")"` | `extrude_face` | P2 | S13 Modeling Toolkit (#64) |
| Section: Transform tool settings | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Custom shelf | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |
| Section: Hotkeys | [toolkit] | Missing | absent | none | P3 | S13 Modeling Toolkit (#64) |

## 12. Hotkeys

Editor::shortcuts (`src/app/shortcuts.rs "fn shortcuts"`) handles Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z, F, Cmd/Ctrl+S and Cmd/Ctrl+Shift+S. F8, F11 and Delete are read in `src/app/panels.rs "pressed(Key::F8)"`. Escape cancels an edit in `src/app/shortcuts.rs "fn cancel_on_escape"`.

| Maya 2026 element | Source | AztecCraft status | Evidence | MCP | Priority | Proposed slice |
| --- | --- | --- | --- | --- | --- | --- |
| Q Select | [toolbox] | Missing | no key handler | none | P1 | S5 Hotkeys (#32) |
| W Move | [toolbox] | Missing | no key handler; tool disabled | none | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| E Rotate | [toolbox] | Supported | src/app/shortcuts.rs | `set_active_tool` (`rotate`), `begin_manipulation`, `update_manipulation`, `finish_manipulation` (the key itself has no MCP form) | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| R Scale | [toolbox] | Supported | src/app/shortcuts.rs | `set_active_tool` (`scale`), `begin_manipulation`, `update_manipulation`, `finish_manipulation` (the key itself has no MCP form) | P1 | S2 Transform tools (PR #77 (Move), #34, #35, #36) |
| + and - resize the manipulator | Maya 2027 [Resize the manipulator](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-09DEFD6A-E8F9-4606-9D81-1CD45E01B59A.htm) and [Manipulators preferences](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Customizing/files/GUID-8740471F-BD72-4BDC-87B4-4DE870AD51E5.htm) (Global scale 0.10 to 10.00) | Partial | src/app/shortcuts.rs (Plus and Equals grow, Minus shrinks, Move, Rotate or Scale active); step 1.25^k grid (k -10 to 10), key repeat, default 1.0, session-only storage and the 36 px floor are PROVISIONAL. The Preferences Manipulators page, Handle size and Line size are unsupported | `set_manipulator_size`, `inspect_scene` `manipulation.manipulator_size` | P2 | #165 |
| T Show Manipulators | [overview] | Missing | no key handler | none | P2 | S5 Hotkeys (#32) |
| Y Last Used tool | [toolbox] | Missing | no key handler | none | P2 | S5 Hotkeys (#32) |
| F8 toggle Object/Component | [select] | Partial | src/app/panels.rs "Key::F8" (Face, Vertex and Edge go to Object; Object and Hierarchy go to Face, PROVISIONAL) | `set_selection_mode` | P1 | S1 Status Line strip (#59) |
| B soft selection | [select] | Missing | no key handler | none | P2 | S3 Component modes (#29) |
| F9 Vertex | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Supported | src/app/panels.rs "Key::F9" (unmodified key only) | `set_selection_mode` `vertex` and `select_vertex` | P1 | S3 Component modes (#29) |
| F10 Edge | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Supported | src/app/panels.rs "Key::F10" (unmodified key only) | `set_selection_mode` `edge` and `select_edge` | P1 | S3 Component modes (#29) |
| F11 Face | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Supported | src/app/panels.rs "Key::F11" | `set_selection_mode`, `select_face` | P1 | S3 Component modes (#29) |
| Alt+F9 Vertex Face | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Missing | no key handler | none | P2 | S3 Component modes (#29) |
| F12 UV | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Missing | no key handler | none | P2 | S3 Component modes (#29) |
| F7 multi-component | [select-2025] FALLBACK 2025; UNCONFIRMED for 2026 | Missing | no key handler | none | P3 | S3 Component modes (#29) |
| F Frame Selected | [hotkeys-2025] FALLBACK 2025 | Supported | `src/app/shortcuts.rs "Key::F"` | `frame_selected` | P1 | S5 Hotkeys (#32) |
| Shift+F Frame Selected in all views | [hotkeys-2025] FALLBACK 2025 | Missing | single view only | none | P3 | S5 Hotkeys (#32) |
| A Frame All in active panel | [hotkeys-2026] | Supported | `src/app/shortcuts.rs "Key::A"`; fires only with no modifiers and no focused text field or open popup. Frames every mesh and curve instance (selection ignored). PROVISIONAL: the pages do not state whether hidden objects, lights or cameras count, the empty-scene result or undo behaviour; here an empty scene returns `nothing_to_frame` and the camera is not on the undo queue. Maya adds a 95% default fill preference that is not reproduced (fit factor 1.05 as Frame Selected) | `frame_all` | P2 | S5 Hotkeys (#32) |
| Panel menu: View > Frame All | [viewmenu] MAYA 2014 page, menu path only | Supported | `src/app/panels.rs "Frame All (A)"`; the other View menu rows are not drawn | `frame_all` | P2 | S8 Panels and quick layouts (#48 / #69) |
| Shift+A Frame All in all views | [hotkeys-2025] FALLBACK 2025 | Missing | single view only | none | P3 | S5 Hotkeys (#32) |
| Shift+M panel menus; Ctrl+Shift+M Panel Toolbar | [panelvis] | Missing | no key handler | none | P3 | S5 Hotkeys (#32) |
| Tap Space single/multi-pane; Ctrl+Space full-screen | [hotkeys-2025] FALLBACK 2025 | Missing | no key handler | none | P3 | S8 Panels and quick layouts (#48 / #69) |
| Ctrl+A Attribute Editor / Channel Box | [attr] | Partial | `shortcuts` dispatches the next dock panel for an explicit Ctrl+A; skipped while text, a popup or a dialog has the keyboard | `set_sidebar_panel_visibility` | P1 | S4 Attribute Editor and Channel Box (#60) |
| Space-hold hotbox; Alt+M hotbox style | [hotkeys-2025] FALLBACK 2025 | Missing | no hotbox | none | P3 | S5 Hotkeys (#32) |
| Undo, Redo, Save, Save As, Delete shortcuts (Maya key map UNCONFIRMED in the report) | UNCONFIRMED | Supported | `src/app/shortcuts.rs "let (undo, redo, frame, save, save_as)"`, `src/app/panels.rs "Key::Delete"` | `undo`, `redo`, `save_scene`, `save_scene_as`, `delete_node` | P1 | S5 Hotkeys (#32) |

## Prioritized backlog

Each item names the candidate slice. Rows above carry the same slice label.

Slice map: S1 #59, S2 PR #77 (Move) and #34, #35, #36, S3 #29, S4 #60, S5 #32, S6 #66, S7 #65, S8 #48 / #69, S9 #61, S10 #68, S11 #62, S12 #63, S13 #64, S14 #67, S15 #70. Umbrella: #71.

### P1

1. **S1 Status Line strip** (#59): remaining work is selection masks (real presets belong to S3, #29), snaps, symmetry, construction history, and the input box. The panels behind the Modeling Toolkit, HumanIK, Attribute Editor (#60) and Tool Settings toggles, and the Layer Editor, are separate work.
2. **S2 Transform tools** (PR #77 (Move), #34, #35, #36): enable Move, Rotate and Scale (W, E, R) with Q for Select, then the Snap buttons. Depends on PR #77 (Move).
3. **S3 Component modes** (#29): selection mask toggles and presets. Vertex and Edge modes (F9, F10), the Select > Components entries and the Status Line Vertex, Edge and Face buttons exist; the masks are per-type toggles in Maya and exclusive modes here. S3 also replaces the disabled Status Line presets menu with real presets over mask state.
4. **S4 Attribute Editor and Channel Box** (#60): Attribute Editor panel with node tabs and attribute list, Ctrl+A, and the Visibility channel in the Channel Box.
5. **S5 Hotkeys** (#32): one key map module for Q, W, E, R, A and the existing bindings, with MCP parity for each action.
6. **S6 Menu content** (#66): Modify menu content and a Display menu. Edit already has Undo, Redo and Delete.
7. **S7 Shelf** (#65): a native Extrude control wired to the existing `extrude_face` operation, so UI and MCP match.
8. **S8 Panels and quick layouts** (#48 / #69): View and Shading panel menus and the Shading group of the Panel Toolbar (Wireframe, Smooth Shade All).

### P2

1. S1 Status Line strip (#59): masks (real presets: S3, #29), snaps, symmetry, construction history, input box; the Modeling Toolkit, HumanIK, Attribute Editor, Tool Settings and Layer Editor panels.
2. S2 Transform tools (PR #77 (Move), #34, #35, #36): Last Used (Y), Show Manipulators (T), Lasso, Symmetry.
3. S3 Component modes (#29): Alt+F9, F12 UV, soft selection (B).
4. S4 Attribute Editor and Channel Box (#60): SHAPES and INPUTS sections, construction history entries.
5. S6 Menu content (#66): Display and Windows menus, per-set menu lists.
6. S7 Shelf (#65): tabs and drop-down, Bevel, other primitives.
7. S8 Panels and quick layouts (#48 / #69): Lighting, Show and Panels menus, Isolate Select, X-Ray, four-pane layout, Frame All (A).
8. S9 Layer Editor (#61): Display Layers.
9. S10 Outliner (#68): search, Display and Show menus, Groups, Hidden objects.
10. S11 Timeline (#62): Time Slider, current time, Go To Start/End, Play, Step Frame, Range Slider.
11. S12 Command and Help Line (#63): Command Line input with MEL/Python toggle, tool hints.
12. S13 Modeling Toolkit (#64): panel, Selection modes, Commands and tools.

### Cross-cutting

- S14 Docking, workspaces and layout persistence (#67): Workspace selector, dockable panels and saved layouts.
- S15 Original icon set (#70): original icons for every drawn control, with licence records.

### P3

1. S1 Status Line strip (#59): Lock selection, Highlight Selection mode.
2. S2 Transform tools (PR #77 (Move), #34, #35, #36): Paint Selection, Make Live.
3. S5 Hotkeys (#32): hotbox, Shift+F and Shift+A variants, panel toggles.
4. S6 Menu content (#66): Rigging, Animation, FX and Rendering sets, Menu Set Editor, Render and Hypershade buttons, Application Home.
5. S7 Shelf (#65): other default tabs, Shelf Editor, Content Browser icon, Materials.
6. S8 Panels and quick layouts (#48 / #69): Renderer and Stereo menus, Lighting, effects and Color Management groups, remaining Camera Settings, Outliner button and Windows > UI Elements.
7. S9 Layer Editor (#61): Animation Layers.
8. S10 Outliner (#68): Render Setup filter, Materials, Sets, Assets, References, Scene Assembly.
9. S11 Timeline (#62): step by key, play backwards, bookmarks, Character Set and Animation Layer menus, playback options.
10. S12 Command and Help Line (#63): result box, Script Editor button.
11. S13 Modeling Toolkit (#64): remaining sections.
12. S8 Panels and quick layouts (#48 / #69): Heads-up display.


## Link references

[overview]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm
[menus]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-D90A2BDB-FD05-4528-8A95-C33A02D15129.htm
[modeling-menus]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Modeling/files/GUID-A0E08BF2-0206-4518-8431-DC4E15AD4558.htm
[menuset-editor]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Customizing/files/GUID-C14313A4-D814-4C2A-987E-F54B46FC7707.htm
[statusline]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-86E5CEDA-4100-40AE-8F95-346206CF8456.htm
[shelf]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-4A21F741-C9AC-4AE5-897E-B6F8C68ADF90.htm
[shelf-editor]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Customizing/files/GUID-06C06384-6FE5-4618-84A6-22289814D1EE.htm
[content-browser-2024]: https://help.autodesk.com/cloudhelp/2024/ENU/Maya-Modeling/files/GUID-C4F6724D-1887-41C5-ADB1-A32FEF47FDD3.htm
[toolbox]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-B345E162-0149-4E09-AC98-48DCFC227F33.htm
[quicklayout]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-693FBB4C-4010-4D55-8215-67FBE19114E7.htm
[panelmenus]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-BD5D066A-4876-4B56-8A9F-96E2243E22A3.htm
[paneltoolbar]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-42224988-F140-4BFF-849B-F89CCB554E13.htm
[shadingmenu]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-C6188583-EA9E-4880-AAE7-8D895FD94DD6.htm
[viewmenu]: https://download.autodesk.com/global/docs/maya2014/en_us/files/Viewing_the_scene_Center_the_view_on_selected_or_all_objects.htm
[hotkeys-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-KeyboardShortcuts/files/GUID-30CACC9D-8FBE-4B85-8A8F-C5ADF32DDD4E.htm
[panelvis]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-455539A6-3506-458C-92DA-14F171C14553.htm
[channelbox]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-4C954FB2-8B6A-4BBD-9695-DF432616D0D2.htm
[layers]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-509DE7DD-E706-49AE-90D7-E9CD5F6582A7.htm
[attr]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-67A58D31-4722-4769-B3E6-1A35B5B53BED.htm
[rearrange-2016]: https://help.autodesk.com/cloudhelp/2016/ENU/Maya/files/GUID-67562106-21C8-4ADD-B315-297F94CC8B4C.htm
[outliner]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-4B9A9A3A-83C5-445A-95D5-64104BC47406.htm
[toolkit]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Modeling/files/GUID-D4CD168A-34F2-465B-A39C-DE20B34E5535.htm
[timeslider]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-827ED8CD-C6AA-4495-8B5E-2FC98C8D49EE.htm
[playback]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-277ADCBF-6277-4B32-81E0-AEB6D081153F.htm
[range]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-E9E054A1-A5D0-412C-976A-F57C0F6960B1.htm
[playopts]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-9853B7B5-8AD5-46F6-A921-A3771A505AD1.htm
[cmdline]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-4981A0A5-9418-4274-918C-561E33B0102D.htm
[helpline]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-AD137576-F1CF-4A62-A7DA-E0ECF6524D11.htm
[select]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-5C5F2B9B-BE16-4C49-B72B-70EA7CB4C688.htm
<!-- select-2025: UNCONFIRMED. This is a JavaScript viewer URL, not a static help page. -->
[select-2025]: https://help.autodesk.com/view/MAYAUL/2025/ENU/?guid=GUID-5C5F2B9B-BE16-4C49-B72B-70EA7CB4C688
[hotkeys-2025]: https://help.autodesk.com/cloudhelp/2025/ENU/Maya-KeyboardShortcuts/files/GUID-F5379746-7A11-48C0-AFBA-D4B333D3B63A.htm
[displaymenu]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-0657DA41-332E-4F3C-BDAE-4A9AD8CA2553.htm
[hud-options]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-0E9D9594-6664-4B69-B944-8B69E9551415.htm
[hud-panel]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-45A62F95-AD75-4C8E-8B46-6D4D8110814C.htm
[viewcube-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-C1861E55-85FA-47F9-B4D2-71366875E56D.htm
[viewcube-overview]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-59D87B3C-5E1B-4CEE-A63B-6EE44EDCFE69.htm
