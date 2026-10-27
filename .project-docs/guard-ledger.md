# Operation guard ledger

Contributor guide to how an `Operation` reaches the editor. Two pipelines exist and stay explicit.
The behavior is recorded cell by cell in `tests/snapshots/guard_matrix.txt` (test
`guard_matrix_matches_committed_snapshot`); `entry_point_tests` fails when a new call site bypasses
them. Anchors below are `path "symbol or quoted string"`, so they survive line drift; grep the
quoted text in the named file. Update an anchor when you rename or move the code it names. Regenerate the matrix with `UPDATE_SNAPSHOTS=1` only for an intended behavior change.

## Pipelines

Remote, `remote_operation_guarded` (`src/app/dispatch.rs "fn remote_operation_guarded"`), checks in this order:

1. `Inspect` returns before any guard (`src/app/dispatch.rs "matches!(request.operation, Operation::Inspect)"`).
2. Scene token mismatch is `StaleScene` (`src/app/dispatch.rs "request.expected_scene_token != Some(self.scene_token)"`).
3. An active Move, Rotate or Scale is `BusyManipulation` unless it is Remote and the operation is
   `UpdateMove`, `UpdateManipulation` or `FinishMove` (`src/app/dispatch.rs "active.provenance != ManipulationProvenance::Remote"`). This is default-deny.
4. Closing, modal (all but `ResolveFileAction`), focused text edit, any draft or pending native
   input is `BusyEditing` (`src/app/dispatch.rs "|| self.native_input_pending"`).
5. Then `apply_operation_guarded` with provenance Remote (`src/app/dispatch.rs "ManipulationProvenance::Remote,"`).

Native, `apply_operation` (`src/app/dispatch.rs "fn apply_operation("`) calls `apply_operation_guarded` (`src/app/dispatch.rs "fn apply_operation_guarded"`) with
provenance Native and no token or draft checks. In order:

1. A Delete or DeleteSelection in Face, Vertex or Edge mode is refused first (see Behavior that stays inside
   handlers). Then `guard_move_dispatch`, with side effects (`src/app/dispatch.rs "self.guard_move_dispatch(&operation, provenance)?"`, `src/app/features/move_tool.rs "fn guard_move_dispatch"`). For
   `SetActiveTool`, `Select`, `ModifySelection`, `SelectByRule`, `DeselectAll`, `SetSelectionMode`, `SelectFace`, `NewScene`, `OpenScene`
   and `RequestClose` it cancels any Move (`cancel_move`, `src/app/features/move_tool.rs "fn cancel_move"`) and consumes the
   pointer sequence only on a real tool change that cancelled nothing
   (`src/app/features/move_tool.rs "self.consume_pointer_sequence()"`). Other operations during a Move are `BusyManipulation` except
   `UpdateMove`/`FinishMove`.
2. Closing is `BusyEditing` (the first `is_closing()` check in `src/app/dispatch.rs "fn apply_operation_guarded"`), so a native `NewScene` while closing cancels the
   Move and then fails.
3. `SetGridVisible`, `SetShadingMode`, `SetSidebarPanelVisible` and `SelectAttributeEditorTab` while modal are `BusyEditing` (the `is_modal()` check right after it in `apply_operation_guarded`). `SetToolSpace`, `SetToolHandle` and `SetManipulatorSize` are refused while modal by the per-variant modal list in `apply_operation_guarded` (`src/app/dispatch.rs "| Operation::SetToolHandle(_)"`, after the `is_modal()` match); the remote path reaches the same refusal earlier through the `is_modal()` check in `remote_operation_guarded`.
4. File operations are decided in one match (`src/app/dispatch.rs "Operation::OpenScene { path } | Operation::SaveSceneAs { path }"`), before any draft commit. For
   `OpenScene` and `SaveSceneAs`, path validity (`src/app/dispatch.rs "Scene path must contain 1 to 4096 bytes and no NUL"`) runs first. `OpenScene` then
   reaches `open_candidate` with no modal check (`src/app/dispatch.rs "return self.open_candidate(path)"`); `SaveSceneAs` is refused while
   modal (`src/app/dispatch.rs "SaveDestination::As(path)"`, preceded by the modal check) and `SaveScene` likewise (`src/app/dispatch.rs "Operation::SaveScene => {"`). `NewScene`,
   `RequestClose` and `ResolveFileAction` decide internally (`src/app/dispatch.rs "Operation::NewScene => return self.new_document()"` and the two arms after it).
5. `SetActiveTool`, `SetToolSpace`, `SetToolHandle`, `SetManipulatorSize`, the Move, Rotate and Scale operations and every selection-changing operation (`Select`,
   `ModifySelection`, `SelectByRule`, `DeselectAll`, `SetSelectionMode`, `SelectFace`, `SelectVertex`, `SelectEdge`) are `BusyEditing` while modal
   (the modal `matches!` after the file-operation match in `apply_operation_guarded`), whoever the caller. The refusal is in the pipeline, not only in the UI checks in
   `src/app/panels.rs` and runs before validation and before the draft commit, so a refused selection change never
   commits a draft (`native_selection_changes_refuse_modal_before_committing_drafts`). Nothing in the modal's own
   resolve path dispatches a selection operation. Create, Delete and Undo/Redo also move the selection as a side
   effect but stay allowed under a modal (see Observed behavior).
6. Validation before the draft commit: `selection_path` (also for `ModifySelection`, which additionally needs Object or Hierarchy mode, as does `SelectByRule`, refused in Face, Vertex and Edge mode) and Select-clear-with-path
   (`src/app/dispatch.rs "Clearing selection cannot include an instance path"`), Face mode and face-target resolution for `SelectFace` (`src/app/dispatch.rs "fn require_component_selection"`, `src/app/dispatch.rs "SelectionMode::Face,"`).
7. `commit_pending` (`src/app/dispatch.rs "fn commit_pending"`) commits a valid human draft first, except for `UpdateMove`,
   `UpdateManipulation` and `FinishMove` (`src/app/dispatch.rs "&& !self.commit_pending()"`). On failure the operation returns `InvalidInput(status)`
   and the status text and error flag change.
8. The operation handler runs and `sync_fields` refreshes Channel Box drafts.

## Behavior that stays inside handlers

These are not expressible as a shared guard and must not be hoisted without a new matrix review.

- `may_install` is threaded through `save_document` (`src/file_ui.rs "fn save_document"`) and
  `resolve_file_action_guarded` (`src/file_ui.rs "fn resolve_file_action_guarded"`, `src/file_ui.rs "&& !may_install()"`). A refusal is
  `NativeSave: save cancelled before installation`. Only a dirty Maya binary document reaches it
  (matrix states `no_install`, `no_install_modal`).
- Read-only Maya ASCII is decided by the scene (`authoring_read_only_reason`,
  `src/native_scene.rs "fn authoring_read_only_reason"`) and surfaces as `ReadOnlyFormat`; Move, Rotate and Scale map it in
  `begin_error` (`src/app/features/manipulation_tool.rs "fn begin_error"`).
- `EditNode` with no fields returns `Unchanged` after only checking the node (`src/app/dispatch.rs "Operation::EditNode {"`).
- Delete and DeleteSelection in Face, Vertex or Edge mode are refused by the first guard of `apply_operation_guarded`
  (`src/app/dispatch.rs "fn apply_operation_guarded"`), so native and remote callers refuse alike;
  `native_operation` no longer carries a separate pre-check and
  `native_operation_reports_face_mode_delete_refusal` covers the UI status line. The remote
  pipeline's outer state guards (stale token, busy editing, busy manipulation) still take priority,
  so a remote `delete(face_mode)` in those states reports the state error. Because the refusal is
  first inside the pipeline, it also runs before `guard_move_dispatch`, the closing and modal
  checks, and `commit_pending`: native delete in a component mode reports this refusal even in the
  closing, move, modal, draft and text-focus states, and remote cells without an outer state guard
  (`read_only_ascii`, `binary_dirty`, `no_install`) report it instead of the read-only or ownership
  error. The Move, Rotate and
  Scale Face-mode refusal is in the shared target check
  (`check_manipulation_target`, `src/app/features/manipulation_tool.rs "fn check_manipulation_target"`) and applies to both pipelines (`begin_move(face_mode)` cells).
- The viewport click path (`src/app/panels.rs`) dispatches Object, Hierarchy, Face, Vertex and Edge picks through
  `apply_operation`, not `native_operation`, so a pick takes the shared guards like any other native operation.
- The `read_only_ascii` fixture is a curve-only scene with no mesh. `set_material_diffuse`,
  `select_face(face_mode)` and `extrude_face` therefore fail there with a missing-shape error, not
  `ReadOnlyFormat`; read-only refusal is shown by `create`, `delete`, `rename` and the other
  edits that pass target validation.

## Observed behavior (recorded, not endorsed)

The matrix records these asymmetries; treat a change as a behavior change that needs review.

- While a file modal is open, `SetSelectionMode` to the mode already current returns `BusyEditing`,
  not `Unchanged`: the modal refusal (item 5 above) runs before the handler's same-mode check
  (test: `native_selection_changes_refuse_modal_before_committing_drafts`, `src/app/tests.rs`). The
  Outliner row click (`Operation::Select`, or `ModifySelection` with Shift or Ctrl, through `native_operation`) is refused the same way by the
  dispatch rule (#141), pinned by `outliner_modified_clicks_are_refused_while_a_file_modal_is_open`.
- Inspect: remote `inspect` is `Ok` in every state. Native `inspect` is `BusyManipulation` during
  a Move and `BusyEditing` while closing, and `Ok` under a modal.
- Native mutations (`create`, `delete`, `rename`, `undo`) succeed under a modal pending file
  action; the remote pipeline refuses all of them as `BusyEditing`.
- Native `set_active_tool` while closing consumes the pointer sequence (the Move guard runs
  first) and then fails with `BusyEditing`.
- Native `delete` with a changed material draft commits the draft first, then fails; the status
  becomes "Material data edited" and the scene has changed even though the result is an `Err`.
- `SetShadingMode` follows Grid: session-only, idempotent (`Unchanged` when the mode already matches),
  never dirty, never a scene-token change; guards are those of `SetGridVisible`. Handler is in
  `src/app/dispatch.rs`; matrix cell is `set_shading_mode(wireframe)`.
- `SetSidebarPanelVisible` follows Grid: remote refuses a stale token, a Move (`BusyManipulation`), a
  draft of any kind (valid or not; `src/mcp_tests/official_client/attribute_editor.rs "Human draft: any draft is busy_editing, valid or not."`), text focus, modal and closing (`BusyEditing`); native refuses closing and modal
  before any draft commit (the closing and modal checks at the top of `src/app/dispatch.rs "fn apply_operation_guarded"`), is
  `BusyManipulation` during a manipulation (`src/app/features/move_tool.rs "fn guard_move_dispatch"`), and otherwise commits a valid
  draft first and refuses an invalid one (`src/app/dispatch.rs "&& !self.commit_pending()"`). The handler is
  `src/app/dispatch.rs "Operation::SetSidebarPanelVisible { panel, visible }"`; matrix cells are `set_sidebar_panel_visible(channel_box,false)` and
  `set_sidebar_panel_visible(attribute_editor,true)`.
- `SelectAttributeEditorTab` has the same guards as `SetSidebarPanelVisible` (modal check in `apply_operation_guarded`, the `commit_pending` gate `src/app/dispatch.rs "&& !self.commit_pending()"`, Move default-deny).
  It is view state only and never enters history. After the draft commit it is refused as
  `InvalidInput` unless the Attribute Editor is the active dock panel (no hidden state that
  inspection cannot show), when the tab does not resolve, or when the shape is beyond the tab
  cap. The handler is `src/app/features/attribute_editor.rs` (`select_attribute_editor_tab`).
  Setting `AttributeEditor` visible hides the Channel Box (one dock panel), so
  `set_sidebar_panel_visible(attribute_editor,true)` is `Changed` from the default state. The
  matrix has 3 new samples per state: `set_sidebar_panel_visible(attribute_editor,true)`,
  `select_attribute_editor_tab(shape)` (Attribute Editor active) and
  `select_attribute_editor_tab(shape,panel_hidden)`. MCP reaches them as
  `set_sidebar_panel_visibility` (`panel` `attribute_editor`) and `select_attribute_editor_tab`,
  covered cell by cell in `src/mcp_tests/official_client/attribute_editor.rs`.
- The File menu and the Status Line file buttons share `file_actions` (`src/app/menus.rs`), the one
  native caller of `Operation::SaveScene`; New and Open go through `request_new` and `open_dialog`.
  Enable state, disabled reason and handler come from the same helper, so the guards are those of
  the menu path, and the entry-point allowlist names `file_actions` instead of `menus`.
- Select > Deselect All and Alt+D share `deselect_all` (`src/app/features/selection.rs`), the one
  native caller of `Operation::DeselectAll`, so the entry-point allowlist names it instead of the
  menu bar; MCP reaches the same operation as `deselect_all`.
- Human-draft commits (`commit_pending`) are the only history mutation besides operations; the
  entry-point test names them.

## Safety net

| Check | Where |
| --- | --- |
| Tool catalog bytes | `tests/snapshots/mcp_tool_catalog.json` |
| Inspection bytes for four editor states | `tests/snapshots/inspection_*.json` |
| Operation x pipeline x state matrix, all platforms | `tests/snapshots/guard_matrix.txt` |
| Matrix cells that write files (Save, Save As, Resolve(save)), Unix and Windows | `tests/snapshots/guard_matrix_file_writing.txt` |
| Entry points | `src/entry_point_tests.rs` |

Snapshots are public: they hold no scene
tokens, UUIDs, absolute paths or home directories (`src/snapshots.rs` rejects those).
