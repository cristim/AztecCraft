# Native editor MCP

The optional MCP server controls the new native Editor window launched by its client. Tools use that window's canonical native scene, history, selection, camera and viewport grid setting. It cannot attach to an already running instance. This candidate exposes the tools in the generated [tool catalogue](mcp-tools.md); it does not establish Maya scripting, plug-in or full-functionality compatibility.

The Save integration in this working branch is provisional. Its ordinary vendored
atomic engine and document adapter have separate consumer checks. Current
application/protocol verification and native Save QA remain required; document
layer results alone do not establish a working GUI Save journey.

## Build and launch

Install the prerequisites in [development instructions](development.md), then build explicitly:

```sh
cargo build --release --locked --features mcp
```

Configure the client to launch the resulting binary with `--mcp-stdio`. A native graphical session is required. The default desktop build and default CI package omit MCP; rebuilding without `--features mcp` replaces the binary at the same target path. Use the exact feature-enabled artifact.

An example subprocess configuration, subject to the client's own configuration format:

```json
{
  "mcpServers": {
    "azteccraft": {
      "command": "<absolute-path-to-mcp-enabled-azteccraft>",
      "args": ["--mcp-stdio"]
    }
  }
}
```

On Windows the executable is `azteccraft.exe`. No socket or HTTP service is exposed. The local subprocess launcher is the authority boundary, including access to local files through `open_scene`. Stdout is reserved for MCP messages; diagnostics use stderr.

Pinned rmcp 3.5.1 handles protocol decoding, discovery, initialization and request metadata. Official SDK tests exercise MCP 2026-07-28 Discover and legacy 2025-11-25 initialization. Cancellation tests exercise the legacy request-cancellation path. These tests do not establish compatibility with every MCP client.

## Tools and state

Material inspection adds an optional bounded `material` projection to each shape:
shader type, authoritative diffuse f64, complete shared-user count and separate
editing/rendering availability. Null means supported editing is unavailable, not
a guessed default. `set_material_diffuse` follows the same token, focus, differing
draft, modal/close and native-input guards as other mutations. Values may exceed
one; no source record is inserted and no other shader input is editable. The
isolated official SDK material edit/inspect/Undo/Save/reopen journey and full
client matrix pass. The corrected test performs actual protocol Open after Save,
requiring new document tokens and inspected material values; independent focused
verification reproduced it. Bounded macOS native data QA also passed.
SDK edit/inspection/History coverage is portable; Save/reopen assertions are
Unix-only because Windows native binary writing remains unsupported. Cold
verification of the committed code and CI are recorded in native-verification.md;
a saved Save/NCRV/Face/Grid composition check remains open. Material data changes do not enable rendered Phong output.

Call `inspect_scene` first. Every other tool requires `scene_token`, a UUID string from that inspection. IDs are unsigned 64-bit decimal strings, including values beyond JavaScript's exact integer range. JSON numbers are rejected for IDs. Unknown argument fields, invalid types and unknown tools are protocol errors.

| Tool | Arguments besides `scene_token` | Result |
| --- | --- | --- |
| `inspect_scene` | No arguments; no token required | Committed mesh/curve metadata including the active manipulation preview, typed document origin, retained-source counts/read-only reason, units, instance paths, selected identity, history, pending action, dirty/draft/focus/modal/closing flags, the native `status_message` (nullable, at most 512 characters, a fixed notice for file-error text that can carry a local path) and the camera pose: `target` and `eye` in scene world units, `distance`, `yaw_degrees` (about world Y, normally in [0, 360]; 0 puts the eye on +Z and 90 on +X), `pitch_degrees` (elevation, positive above the target, strictly between -90 and 90), `fov_y_degrees` (vertical perspective field of view) and `near_clip`/`far_clip`. `eye` = `target` + `distance` × (sin yaw · cos pitch, sin pitch, cos yaw · cos pitch). Read-only: no tool sets an absolute pose. The defaults are a 45 degree vertical field of view and a 0.01 to 10000 clip range; framing a large object can raise `far_clip`. They have not been compared with Maya's persp camera, whose Angle of View is horizontal |
| `set_active_tool` | `tool`: `select`, `move`, `rotate` or `scale` | Activate a supported native tool through shared guards |
| `set_tool_space` | `tool`: `rotate` or `scale`; `space`: `object` or `world` | Set that tool's Axis Orientation (`Operation::SetToolSpace`, the operation the native Tool Settings selector dispatches). Refused with `busy_manipulation` during any manipulation and `busy_editing` during a draft, modal or close; `inspect_scene` `manipulation.rotate_space` and `scale_space` report the current values |
| `begin_manipulation` | `oneOf` by `kind`, each branch closed. `move`: `node_id`, full `instance_path` (1 to 256 decimal IDs), `axis` (`x`, `y` or `z`, World). `rotate`: the same plus `axis` and `space` (`object` or `world`). `scale`: the same plus `handle` (`x`, `y`, `z` or `uniform`) and `space` | Begin a remote-owned Move, Rotate or Scale of the exact selected transform; returns `manipulation_id`. A field of another kind (`handle` on a rotate, `space` on a move) is a protocol error |
| `update_manipulation` | `manipulation_id`: decimal string; exactly one of `distance`, `angle` or `factor` (`oneOf`, closed branches) | Preview an absolute value from begin. `distance` (Move) is in the scene linear unit, `angle` (Rotate) in the scene angular unit (degrees or radians), `factor` (Scale) is unitless and greater than zero. A factor of zero or less is rejected at the schema/parser layer as `invalid_params` (the schema declares `exclusiveMinimum: 0`), so the core's zero-scale refusal is reachable only from a native drag. A parameter that does not match the active kind returns `manipulation_kind_mismatch` and leaves the preview unchanged |
| `finish_manipulation` | `manipulation_id`; `finish`: `commit` or `cancel` | One Undo at commit (labelled Move, Rotate or Scale), or exact begin-state restoration |
| `create_primitive` | `primitive`: `cube`, `plane`, `sphere`, `cylinder` or `cone` (the schema is one `oneOf` tagged by `primitive`); optional `name` (omitted: Maya default names such as `pCube1` with shape `pCubeShape1`; given: shape `<name>Shape`, #175); optional `options` object for that primitive, every field optional and defaulting to the values below | Created `node_id`; selects the new node |
| `select_node` | Optional `node_id` and `instance_path`, at most 256 decimal NativeId strings | Full root-to-transform prefix; ambiguous objects require a path. Null or omitted node clears selection and cannot include a path. `select_node` stays exact: it selects the named transform and is not limited by the selection mode |
| `set_selection_mode` | Required `mode`: `object`, `face`, `vertex`, `edge` or `hierarchy` (Root sub-mask only; Leaf and Template are unsupported) | Changes the transient selection mask; a valid remembered face, vertex or edge remains inactive outside its own mode. In Hierarchy mode a native viewport click selects the root transform of the hit object's hierarchy |
| `select_face` | Required `target`: complete target object or null | In Face mode, selects that exact owning instance and canonical face; null clears only the face |
| `select_vertex` | Required `target`: complete target object or null | In Vertex mode, selects that exact owning instance and native point index; null clears only the vertex |
| `select_edge` | Required `target`: complete target object or null | In Edge mode, selects that exact owning instance and native edge index; null clears only the edge |
| `rename_node` | `node_id`, `name` | Changed or unchanged authoring state; a direct shape whose name derives from the old name is renamed as in Maya (`pCubeShape1` under `pCube1` to `box` gives `boxShape`, #175) |
| `set_transform` | `node_id`, `translation`, `rotation_degrees`, `scale`, each an array of three finite f64 numbers | Translation in declared scene units, rotation in degrees, dimensionless scale |
| `edit_node` | `node_id`; optional `name`, `translation`, `rotation_degrees`, `scale` | One undo entry for the supplied name and individual channels; a name change renames the derived shape as `rename_node` does; selection stays unchanged |
| `extrude_face` | `node_id`, `shape_id`, `instance_path`, `face_index`, `distance` | Extrudes one current canonical face through the shared native history |
| `set_material_diffuse` | `shape_id`: decimal NativeId; `value`: finite nonnegative f64 | Edits the checked existing Phong diffuse scalar for all shapes sharing that shader, through one History command |
| `create_session_material` | `color`: three finite linear RGB channels, each 0 through 1 | Creates a session-only lambert (Maya 2025.1 default names `lambert<N>` with `lambert<N>SG`, N from 2; numbering in known-issues, #177) as its own undo step and replies `{outcome:"session_material", material_id}` |
| `assign_session_material` | `shape_id`: decimal NativeId; exactly one of `material_id` (existing) or `color` (creates one in the same undo step) | Assigns the whole shape, so every instance changes; replies `session_material` with the id, or `unchanged` |
| `set_session_material_color` | `material_id`; `color` as above | Recolours a created material; material 0, the initial shading group, is locked |
| `delete_node` | `node_id` | Changed authoring state; refused outside Object or Hierarchy selection mode; a node with children or a parent returns `unsupported_delete` (hierarchy delete is not implemented yet, #187) |
| `undo` | None | Changed or unchanged history state |
| `redo` | None | Changed or unchanged history state |
| `frame_selected` | None | Frames the selected object in Object mode, selected face corners in Face mode, the remembered point in Vertex mode or both endpoints of the remembered edge in Edge mode; no active face, vertex or edge returns `no_selection` |
| `frame_all` | None | Frames every mesh and curve instance with the live camera, ignoring the selection (Maya View > Frame All, hotkey A); a scene with none returns `nothing_to_frame` and keeps the camera. Session-only: no history entry, selection and scene unchanged; changed or unchanged outcome |
| `set_grid_visibility` | `visible`: required boolean | Changed or unchanged grid visibility in this window's viewport |
| `set_shading_mode` | `mode`: required, `"wireframe"` or `"smooth_shade_all"` | Changed or unchanged shading mode in this window's viewport |
| `set_sidebar_panel_visibility` | `panel`: required, `"channel_box"` or `"attribute_editor"`; `visible`: required boolean | Changed or unchanged visibility of that sidebar panel in this window; the dock shows one panel, so showing one hides the other |
| `select_attribute_editor_tab` | `tab`: required `{kind, id}`; `kind`: `"transform"`, `"shape"` or `"material"`; `id`: decimal id (the selected node for `transform`, a direct shape for `shape` and `material`) | Changed or unchanged Attribute Editor tab; refused as `invalid_input` unless `attribute_editor` is the active panel or when the tab does not resolve for the current selection |
| `orbit_camera` | `radians`: two finite numbers, yaw and pitch deltas | Changed or unchanged camera; pitch clamps near the poles |
| `pan_camera` | `world_units`: two finite numbers, camera right and up displacement in scene world units | Changed or unchanged camera target and eye |
| `dolly_camera` | `factor`: positive finite distance multiplier | Changed or unchanged camera distance; below one moves closer |
| `new_scene` | None | New empty document immediately, or a pending confirmation when dirty |
| `open_scene` | `path`: 1 to 4096 UTF-8 bytes, no NUL | Supported `.mb`, read-only static cubic `.ma`, or legacy `.aztec` candidate; applies immediately or stages an immutable candidate for confirmation |
| `save_scene` | None | Active Unix Maya binary Save, or reconciliation of an uncertain installation without another write |
| `save_scene_as` | `path`: 1 to 4096 UTF-8 bytes, no NUL | Fresh `.mb` destination only, never overwrite; an existing receipt is reconciled instead |
| `resolve_file_action` | `pending_action_id`: UUID; `decision`: `cancel`, `discard` or `save` | Resolve only that exact pending action; source-changed queued Open requires Cancel and reopening |
| `request_close` | None | Native close guard; dirty documents require confirmation, and approved remote closure waits for its own successful response flush |

`create_primitive` options (PROVISIONAL AztecCraft safety bounds, #5; Maya documents none, internal cm units; the bounds are published in the tool schema and come from `src/primitive_options.rs`): sizes are finite numbers from 1e-6 to 1e6; subdivision fields are whole numbers 1 to 128, except `subdivisions_axis` 3 to 128 and the sphere `subdivisions_height` 2 to 128; the resulting face count must stay within 20,000 (a budget the schema cannot express, so it is stated in the tool description and enforced as `invalid_params` by `validate`); `axis` is `x`, `y` or `z` (default `y`). Fields: cube `width`, `height`, `depth`, `subdivisions_width`, `subdivisions_height`, `subdivisions_depth`; plane `width`, `height`, `subdivisions_width`, `subdivisions_height`; sphere `radius`, `subdivisions_axis`, `subdivisions_height`; cylinder and cone `radius`, `height`, `subdivisions_axis`, `subdivisions_height`. Defaults are the previous meshes except the plane (cube 1x1x1; plane 1x1 with 10x10 subdivisions since #176; sphere radius 1, 20x20; cylinder and cone radius 1, height 2, 20x1). Unknown keys, keys of another primitive, `null` (for `options` or any of its fields; `name` still accepts null as before), non-objects, subdivision values that are not a JSON integer (3.0 is rejected) and out-of-range numbers fail with protocol `invalid_params` before any state is touched; the parse step calls the same `validate` as the core. A call with only `scene_token` and `primitive` builds these defaults (the plane is 10x10 since #176). Native Create-menu and shelf clicks use the last options created in the option-box window (session only), while omitted MCP fields always use these defaults.

Transforms use the [existing coordinate conventions](development.md). The `rotation_degrees` input and normalized transform inspection always use degrees; `authoring_transform.rotation` retains the document's declared angular units, reported in `units`. Translation retains declared linear units. A node whose rotate order, rotate axis, shear or pivot translations differ from Maya's defaults also reports a read-only `transform_attributes` object, and the key is omitted for every other node: `rotate_order` (`xyz`, `yzx`, `zxy`, `xzy`, `yxz` or `zyx`), `rotate_axis_degrees` (degrees; a degree scene reports the declared value unchanged), `shear` (unitless `[xy, xz, yz]`; the component order is provisional until checked against a Maya reference), `rotate_pivot_translate` and `scale_pivot_translate` (declared linear units), and `non_default`, the names of the attributes that differ. No tool sets these values; Maya ASCII scenes that carry them are read-only and Maya Binary extras are not parsed yet. Camera navigation uses centimeter view space. Each node reports `visible`, the transform's own recorded `.v` as `true`/`false`, or `null` when the source did not prove it; it is read-only and is not effective (inherited) visibility, and no tool sets it. Inspection exposes shape counts and smoothing capability, without mesh arrays, source bytes or private file paths. This is bounded metadata inspection, not a geometry dump.

`select_face.target` contains required `node_id`, `shape_id`, `instance_path` and
`face_index`. IDs are decimal strings; the full path has 2 to 257 IDs, ending in the
shape beneath the requested transform, and the face index is the current source
face ordinal before triangulation. A valid explicit target may name another
object or a currently occluded/clipped face. It does not invoke viewport picking
or silently change the selection mask. Invalid targets preserve selection;
Object mode rejects even a null face request. An omitted target is invalid.

`select_vertex.target` has the same shape with `vertex_index` in place of
`face_index`: the native point index (`NativeMesh::points` order, the same index space
as the viewport's world vertices), bounds-checked against the shape. As for faces, an
explicit target may name an occluded or off-screen vertex, picking is not invoked and
the mask is never changed silently. A request in any mode but Vertex, including a
null target, is `invalid_input`; failed requests preserve the selection, remembered
state and mode. Remote guards match `select_face` (token, active Move, closing,
modal, draft). Inspection adds `remembered_vertex` (the target or null), inactive
outside Vertex mode, revalidated after edits and Undo/Redo, and charged against the
inspection size budget.

`select_edge.target` has `edge_index` in place of `vertex_index`: the native edge
index, bounds-checked against the shape, with the same rules (Edge mode only, null
included; occluded and off-screen edges may be named; failed requests preserve
state; remote guards match `select_face`). Inspection adds `remembered_edge`, inactive
outside Edge mode, revalidated and charged against the size budget like the vertex.
Each component type keeps its own remembered pick: selecting an edge leaves the
remembered vertex untouched and it becomes active again in Vertex mode.

Natively, Vertex and Edge modes are reached with F9 and F10, the Select >
Components menu, the Status Line Vertex, Edge and Face buttons and a plain viewport
click (`pick_vertex`/`pick_edge` then the same `SelectVertex`/`SelectEdge` operations
and guards these tools use). The buttons dispatch the F9/F10/F11 mode intents, so
`set_selection_mode` covers them and no tool was added.

Inspection reports `selection_mode`, the complete `remembered_face` target or
null, and fixed `face_selection_policy: "visible_centers_only"`. A remembered
face is inactive in Object mode and is revalidated after edits and Undo/Redo.
Changing object/path or replacing the document clears it; successful New/Open
retains the current mask. This transient state does not edit the native scene,
file binding, history or dirty status. Native actions retain the existing valid
human-draft commitment policy; remote requests refuse any draft, valid or not (`src/mcp_tests/official_client/attribute_editor.rs "Human draft: any draft is busy_editing, valid or not."`).

Native Face selection uses 4x4 physical-pixel center markers and an 8x8 click box,
with center-ray visibility and a bounded intersection budget. Plain primary
click replaces or clears the face; modified selection and ambiguous coalesced
primary releases are unsupported. Alt camera navigation remains available.
F11 and Select > Components > Face enter Face mode; F8 goes from Object or Hierarchy to Face and from Vertex, Edge or Face to Object (provisional).
Exact Maya CBS, numeric, visual and event-order parity remain unverified. Native
Extrude remains disabled pending a real history/manipulator workflow; the
existing explicit `extrude_face` tool keeps its checked authoring semantics.

Supported static cubic Maya ASCII `.ma` scenes open through the same native
document lifecycle. Original script bodies remain inert retained data. Unknown
commands, node classes, geometry dependencies or unsupported curve classes refuse
the complete candidate before adoption. This subset is read-only: authoring and
Save are unavailable, while Outliner/`select_node`, navigation and
`frame_selected` remain available. Viewport curve picking and CV/component editing
are unsupported. Binary NCRV remains unsupported.

Inspection keeps mesh `shapes` intact and adds a bounded `curves` array. Each
curve has decimal `id`, `name`, degree, typed `form` (`open`), numeric
`control_vertices` and `knots` counts, f64 parameter `domain`, `read_only` and
`draw_available`. Each direct transform owner's `shape_ids` includes its curves.
No CV/knot arrays or script text are returned. `document_origin` is `untitled`,
`maya_binary`, `maya_ascii` or `legacy_aztec`; `retained_source_bytes` and
`retained_source_records` report counts only, with records including retained
trailing trivia. `read_only_reason` carries the core refusal message or null.

Curve `draw_available` means at least one currently visible instance successfully
prepared as draw geometry in one bounded whole-view preparation. Hidden curves or
whole-view preparation refusal report false while canonical inspection remains
available. This field does not mean the backend is initialized or healthy, that
GPU submission/framebuffer rendering succeeded, or that Maya fidelity is proven.
The separate viewport health fields retain their existing meaning.

Scoped local tests exercised the genuine legacy 2016 reference through the
official SDK's actual Editor Open/inspection/selection/framing path, including
strict catalog discovery and read-only edit refusals. Synthetic tests cover
dirty failed-Open baselines, input/token/focus/modal/close guards and the actual
Editor GPU callback. The Save/material-parent union also exercises ASCII Save
and Save As refusals and Save acknowledgements before queued ASCII source
revalidation. A separate earlier diagnostic native preview observed genuine
Open, Outliner selection, visible curve geometry and disabled controls; it did
not exercise a native text commit. Earlier matrix and isolated mutation evidence
do not verify every later source composition. The twenty-four-tool count describes
the catalog at the Face-selection integration parent; curves add no tools, and a later
integration parent must use its own count. [Development notes](development.md#read-only-static-cubic-maya-ascii)
record explicit fixture identity and consumer commands.

`edit_node` uses the Channel Box's sparse native History command. Each supplied
channel array has exactly three finite numbers or nulls. An omitted array or a
null component leaves that channel untouched; a null whole array is invalid.
Omitted `name` or `name: null` leaves the name untouched; an empty string attempts
a name change and follows normal validation. Only supplied rotations convert
from degrees to the document's declared angular unit, preserving untouched raw
radian words. `set_transform` remains a deliberate replacement of all nine
channels. Name and channel changes validate atomically, including unsupported
imported renames. An empty edit validates the node and returns unchanged after
the normal guards, without creating history or synchronizing fields. Identical
values preserve redo. These operations retain the document token, file binding,
camera and full instance selection.

`extrude_face` requires an explicit root-to-shape `instance_path` of 2 to 257
decimal NativeId strings, ending in `shape_id` beneath the transform identified
by `node_id`. Unlike `select_node`, this path includes the shape. `face_index`
is the current canonical face ordinal before triangulation. The document token
does not identify a topology revision: an old ordinal that remains in range may
refer to a different face, so inspect current state before editing. No selected
face or default path is inferred.

Distance is finite f64 in declared scene linear units, applied along the mesh's
local face normal; transforms are not baked. Negative distances are supported,
and zero distance still changes topology. Shared shapes change for all their
instances. Successful extrusion creates one undo entry and preserves selection,
camera, document token and unrelated objects. Imported structural edits and
unsupported topology or mesh metadata refuse without mutation. Extruding the
only face of a 1x1-subdivision plane refuses because all-faces extrusion is
unsupported; the default 10x10 plane extrudes like any quad face. The native face
extrusion control remains visibly unsupported; this tool exposes the existing
checked History command.

Successful tool results provide `structuredContent` and equivalent text content. Authoring outcomes are `scene`, `created`, `changed` or `unchanged`; file lifecycle outcomes are `document_applied`, `pending_confirmation`, `cancelled`, `close_requested`, `saved`, `save_uncertain` and `save_reconciled`. A scene result nests fields under `inspection`; a created result includes `node_id`. Authoring mutations share native undo history. Selection and camera operations do not create authoring undo entries or mark the document dirty. Navigation arguments use camera units, not pointer pixels. Invalid resulting camera ranges return `camera_error` without changing the camera; finite JSON numbers outside the camera's f32 range are also rejected by camera validation.

Tools that set an absolute state return unchanged when repeated and carry `idempotentHint: true`; the generated [tool catalogue](mcp-tools.md) marks which ones, and a drift test keeps it equal to the registry.

`set_grid_visibility` requires an explicit JSON boolean and the current scene
token. Missing, null, string, numeric and unknown fields refuse; there is no
remote toggle or inferred value. Repeating the same value returns unchanged.
Inspection reports authoritative `grid_visible` even without a GPU, independently
of `viewport_error`, `depth_complete` and `clipping_complete`. The setting starts
true, survives successful or canceled New/Open, and belongs to this viewport's
session. It does not edit NativeScene, create an undo entry, clear redo, mark the
document dirty or change its token. Native clicks may first commit a valid human
draft; remote setters reject drafts through the ordinary shared guards.

The fixed XZ grid uses Maya 2025.1's default grid options (#160): lines every unit out to
+/-12, major lines every 5 and brighter axes (extent reading and colours PROVISIONAL; see
known-issues). Grid settings (the Display menu shows Grid Options disabled; there is no MCP tool for them), global or
additional panels, persistence and a default shortcut are unsupported. This scope does not
establish Maya grid appearance or behavior parity.

`set_shading_mode` requires the current scene token and an explicit `mode` of
`wireframe` or `smooth_shade_all`; other strings (including `default_material`),
null, numbers and unknown fields refuse. Repeating a mode returns unchanged.
Inspection reports authoritative `shading` even without a GPU. `face_centres_hidden` is true when the last painted frame hid Wireframe
Face-mode centre markers because the occlusion check exceeded its work budget (false without a
GPU or before the first paint). `vertex_markers_hidden` is the same for Vertex-mode vertex
markers; at most one of the two is true in any frame, by selection mode. The setting starts
`smooth_shade_all`, survives New/Open and belongs to this viewport's session. It
does not edit NativeScene, create an undo entry, mark the document dirty or change
its token. It shares the Grid guards: stale token, any draft (valid or not; `src/mcp_tests/official_client/attribute_editor.rs "Human draft: any draft is busy_editing, valid or not."`), text focus,
modal, closing and an active Move refuse. Wireframe skips the surface fill:
triangle picking and the face-center occlusion filter are unchanged, and Face-mode
center markers are drawn only where that filter accepts them (no surface depth
exists to hide the rest). Maya's edge/marquee Wireframe picking is not
reproduced. Selected-face fill is not drawn in Wireframe; only the orange edges show. Wireframe on Shaded, Use default material, textured
shading and lights are unsupported and have no mode value. Native hotkeys 4 and 5
(2026 hotkey page) select the modes; 6 and 7 only set a status message. This scope
does not establish Maya shading or lighting parity.

`set_sidebar_panel_visibility` requires the current scene token, a `panel` and an
explicit JSON boolean `visible`. `panel` accepts `channel_box` and `attribute_editor`; any other
value, and missing, null, string, numeric and unknown fields, are refused as
`invalid_params` before dispatch, with no mutation and the token unchanged.
`true` makes the panel the active sidebar dock panel; `false` hides the dock when
that panel is active. Repeating a value returns unchanged. Inspection reports the
authoritative state as `sidebar_panels.channel_box` and `sidebar_panels.attribute_editor` (at most one is true). The setting is session-only,
survives New/Open, and does not edit NativeScene, create an undo entry, mark the
document dirty or change the token. It shares the native Status Line toggle's
typed setter and guards: native clicks may first commit a valid human draft;
remote calls reject drafts, modals and pending close.

The native dock transition table applies to every caller. With the current dock panel in the
rows: from Channel Box, `(channel_box,true)` and `(attribute_editor,false)` are unchanged,
`(channel_box,false)` hides the dock, and `(attribute_editor,true)` switches to the Attribute
Editor (changed, implicitly hiding the Channel Box). From Attribute Editor the cells mirror
this. From a hidden dock, `true` shows that panel and `false` is unchanged. Native Ctrl+A
(explicit Ctrl, never Command; documented in Maya as Ctrl+A on every platform, macOS Cmd
versus Ctrl unverified) dispatches the same setter with the next panel: Channel Box to
Attribute Editor, Attribute Editor to Channel Box, hidden to Attribute Editor (an approximation;
this is not Maya dock or tab parity).

`select_attribute_editor_tab` takes the scene token and `tab` with `kind` and a decimal `id`;
unknown kinds, missing or extra fields and non-decimal ids are `invalid_params`. A tab
that does not resolve (a node other than the selected one, a shape that is not a direct shape
of the selected transform, a material without a supported shader) or a call while the
Attribute Editor is not the active panel is `invalid_input` with no change. Only the first 16 direct shapes of a transform have shape and material tabs; a shape or material tab for a later shape is refused as `invalid_input` naming the cap, for native and remote callers alike. The tab is
view state only: it never enters history, resets to the transform tab on selection change, scene
replacement, node deletion, shape removal or when it stops resolving, and is reported as
`inspect_scene` `attribute_editor_tab` (`{kind, id}`, null while the Attribute Editor is hidden
or nothing is selected). Shape and material tabs drive the same single `material_shape`
selection as the native Materials shape choice. Guards match `set_sidebar_panel_visibility`.

Example create arguments after inspection:

```json
{
  "scene_token": "<scene-token-from-inspection>",
  "primitive": "cube",
  "name": "remote_cube"
}
```

Inspection also exposes a fixed-size `source_appearance` summary for the scene and each shape. `checked_viewer_source` means the supported unique-instance component, shading-group, Phong/file/placement and UV source associations were checked. It does not mean materials are evaluated or ready to render. Unsupported or ambiguous associations report `unavailable` with fixed issue flags; source-free scenes report `unbound`. The remaining evaluation, quadratic-filtering and color-processor issues stay explicit, and `appearance_incomplete` remains true. No texture path, UV array, source name or source payload is added to inspection.

The native Channel Box uses the same summaries. A selected transform reports all its directly attached shapes as `Direct shapes (N)`; a transform without direct shapes and an empty selection report `Scene totals (N)`. These labels do not imply subtree or selected-instance appearance evaluation.

Each new Editor and each applied New/Open replaces the scene token. A `document_applied` reply carries the new token. A dirty file action returns `pending_confirmation` with the old token, `pending_action_id` and kind; inspection exposes that same id and kind. Cancel clears only the matching pending action and retains the token. Discard applies the exact staged candidate once unless a prior Save decision marked its source changed; that blocked action requires Cancel and reopening. Stale or replayed action ids refuse without effects. Normal edits, undo and redo retain the token. The token identifies a document generation, not an edit revision or credential. Reinspect after a stale token, deleted ID, cancellation, timeout or lost acknowledgement; do not automatically retry a mutation.

## Native Save candidate

Save and Save As share the native Editor operations. Native actions first commit
a valid human draft; remote actions refuse focus, any draft (valid or not; `src/mcp_tests/official_client/attribute_editor.rs "Human draft: any draft is busy_editing, valid or not."`), pending input,
modal state and stale tokens. Save supports source-bound Maya binary scenes on
Unix with proved existing-record edits and, for translate only, insertion of an
absent leaf at a recorded site. Rotate, scale and siteless nodes refuse: `begin_manipulation`
with `absent_channel_leaf`, and `edit_node` and `set_transform` with
`scene_error` ("native ownership or edit effects are unsupported"). Save As from an opened scene derives its bytes from the
bytes parsed at Open, not from the destination or the file on disk. New, legacy
and Windows writing remain unsupported. Read-through-symlink documents can Save As to a fresh destination;
active Save through that link remains refused.

`saved`, `save_uncertain` and `save_reconciled` replies include `scene_token`,
`dirty` and `pending_action_status` (`none`, `retained` or `source_changed`).
Inspection reports `document_origin` (`untitled`, `maya_binary`, `legacy_aztec`),
`has_file`, `save_available`, `save_as_available` and
`save_reconciliation_required`, without exposing paths. Availability describes
an eligible attempt, not a promise that disk state is unchanged.

An uncertain outcome means installation occurred but durability or verification
was not acknowledged. The document retains its exact written snapshot and opaque
receipt, stays dirty, and does not change its active path or saved baseline.
The next Save or Save As only reconciles that receipt; it does not encode or
install again, even if a new destination was requested. Successful reconciliation
acknowledges the written snapshot while retaining any newer live edits as dirty.
A later Save is a separate write. Selection, camera, history and document token
are not replaced by saving.

A successful Save decision revalidates a queued Open source before consuming it.
If saving changed that source, the reply still acknowledges the Save with
`pending_action_status: source_changed`; the same pending action remains blocked
against Discard until Cancel and reopening. A newer dirty snapshot or expired
request retains the pending action. Cancel preserves uncertainty. Explicit
Discard warns that a destination may already contain written bytes and abandons
the receipt only after a successful document replacement; Close retains it until
actual Editor drop, including a failed close-response flush.

Each request has one absolute deadline shared by queue waiting and installation
guards. Cancellation, connection loss or expiry is checked before dispatch and
each actual install attempt. Actual disk outcomes are acknowledged in Editor
state even if the reply is lost. A fresh guard after Save prevents consuming a
pending New/Open/Close after cancellation, without undoing the real write.

## Human edits, errors and limits

The production viewport uses the same canonical geometry and camera as native
selection and the navigation tools. Inspection reports `viewport_backend` as
`wgpu_geometry_preview` for the initialized GPU pipeline, or `uninitialized` in
device-free test Editors. `depth_complete` and `clipping_complete` describe a
healthy initialized backend; both become false on a terminal backend failure.
`viewport_error` is null or one fixed category: `uninitialized`, `backend_failed`,
`camera_input`, `non_finite_clip` or `resource_limit`. Inspection reads the live
backend latch even while hidden, without exposing driver messages. A successful
submission clears a recoverable category; it cannot clear a terminal failure.

Rendering remains an opaque flat geometry preview. Materials, transparency,
normal shading, textures and the document color-management transform are not
implemented; `appearance_incomplete` and the persistent viewport HUD retain that
limitation. Remote camera/selection/grid changes appear in the next fresh native
frame, after the current immutable callback snapshot. Hiding the grid leaves
surfaces, object edges, picking and camera unchanged. Empty hidden-grid frames
retain the viewport background without uploading or drawing an empty vertex
buffer. There is no CPU fallback.

Remote mutations return `busy_editing` while any of these holds: a native text field is focused, a draft exists (this includes valid uncommitted drafts), native input is pending, a close is in progress, or a modal dialog is open. They do not commit unrelated human edits. `resolve_file_action` is the sole modal exception; text focus, drafts, pending native input, a close in progress and scene-token checks still apply. Inspection remains available and reports these states without returning draft text. Native inputs are processed before queued remote requests at the end of each UI frame, and remote changes request repaint.

Approved remote closure reports `closing: true` while waiting for its matching successful JSON-RPC response to flush. Ordinary mutations refuse during closure; readonly inspection stays available. An unrelated response, error, cancellation, deadline or failed write cannot authorize exit. Failed delivery withdraws approval, preserves document state and suppresses an automatic disconnect retry; an explicit native Close remains available. The resolver response owns this delivery barrier when Discard or a successful Save approves a pending Close. A flushed response establishes transport delivery, not proof that a client processed it.

Hidden/minimized windows dispatch through eframe's UI-thread logic callback without painting or forcing foreground. Visible windows still dispatch only after that frame's native UI input. Interactive native input retained while hidden sets the inspection flag `native_input_pending` and blocks remote mutation until visible UI processes it. Harmless focus changes and unheld pointer motion do not block idle hidden requests. Native Close while input is pending is canceled; restore/process the input and retry Close.

A successful tool call updates the native status line with the wording the matching native action uses: "Scene edited" for authoring edits, "Undo <command>"/"Redo <command>", "No change", "Scene ready", the active tool's help after `set_active_tool` for Rotate or Scale, "Material data edited" for `set_material_diffuse`, and "Move committed", "Rotate committed", "Scale committed" or "Edit cancelled" from `finish_manipulation`, as a native drag release or Esc reports. `begin_manipulation` and `update_manipulation` leave it unchanged, because native drags report their own live preview. Session-material tools, which have no native control, report "Scene edited". Selection, camera, view and file-lifecycle tools leave it as the native actions do. A remote success replaces earlier native feedback, including an error, as a later native action would. This follows Maya's Command Line, which reports a command's result whatever issued it (PROVISIONAL: Maya's docs do not discuss the origin). Refused and failed calls report only to the caller and leave the status line unchanged, so a script cannot overwrite the user's own feedback with its errors. Tool failures set `isError` and return `error.code` plus `error.message`. Codes include `nothing_to_frame`, `busy_manipulation`, `stale_manipulation`, `manipulation_owner`, `manipulation_error`, `manipulation_kind_mismatch`, `absent_channel_leaf`, `busy_editing`, `stale_scene`, `stale_pending_action`, `ambiguous_selection`, `too_large`, `no_selection`, `invalid_input`, `scene_error`, `unsupported_delete`, `file_error`, `view_error`, `camera_error`, `busy`, `unavailable` and `cancelled`. `file_error` is returned by `open_scene`, which takes a filesystem path, when loading the document fails; `save_scene_as` also takes a path but reports failures through the fixed `save_*` codes below. It always carries the fixed message "File operation failed (details withheld)" (decided by the typed `OperationError::File` variant, never by scanning text), because the underlying document and OBJ errors embed the filesystem path. `view_error` covers path-free failures to build or render the derived view (resource budget, unsupported polygons or transforms, non-finite world geometry) and keeps its actionable text. Other error text is path-free: `NativeSaveError::Io` carries only an `io::ErrorKind`, scene, history, camera and manipulation errors name ids and values, and `invalid_input` echoes of the native status come only from `commit_pending`, whose every failing branch overwrites the status with path-free draft or history validation text (a test covers a failed open followed by a failing draft). Save errors use fixed codes save_unsupported, invalid_destination, source_changed, destination_exists, save_cancelled, save_io and save_encode. `busy` covers queue saturation and the five-second UI reply deadline. The SDK may suppress a cancelled response. Cancellation before application skips a queued request; cancellation after a synchronous operation starts cannot undo it. Inspect to reconcile the result.

Prototype bounds are 64 KiB per ingress JSON line, 4096 UTF-8 bytes per input name, at most 16 retained RPC slots and 16 pending supported notification callbacks, 16 queued editor requests, and eight requests dispatched per frame. RPC admission remains held until reply write or connection close; callback permits remain held until completion/drop or close, including reused request IDs. A cancelled request whose reply the SDK suppresses retains its RPC slot until connection close. Enough abandoned calls can exhaust admission and close the session. A new connection launches a new Editor window; it cannot attach to the old window.

Unsupported and repeated notifications are filtered before SDK dispatch. Oversized ingress, invalid JSON, fatal SDK decoding errors, duplicate outstanding IDs or exhausted protocol admission close the connection. SDK compatibility handling can skip nonstandard messages; the adapter consumes those ignored lines without stalling later buffered requests.

Tool results are limited to 512 KiB and framed replies to at most 1 MiB, including the terminating newline. Inspection preflights source size before cloning nodes, then bounds serialized content. Large scenes can return `too_large` rather than partial inspection. These bounds do not promise inspection of arbitrary large scenes.

The [first-click text fix](../known-issues.md#first-click-text-input-can-be-lost) delivers same-frame click-and-type to the field egui hit-tested; the residual cases it lists still drop text. Tests prove the bridge adds no input loss compared with the same native baseline.

## Shutdown and verification

Ingress EOF or protocol failure closes the bridge immediately, skips queued mutations and invokes the existing native close guard before SDK handler draining, except after a failed remote-close delivery attempt as described above. A clean editor closes. Unsaved committed scenes, including valid drafts committed by native close handling, prompt Save/Discard/Cancel confirmation, with Save enabled only for active-save or receipt-reconciliation eligibility. An invalid draft keeps the GUI open with a validation error; the user must fix or cancel that edit before Close can reach confirmation. This deliberately departs from MCP's recommendation to exit promptly for a dirty document. A client can forcibly terminate its subprocess and lose unsaved work; universal shutdown compatibility is not claimed. Cancelling native close does not revive an EOF-closed protocol connection.

Approved native close cancels protocol work and uses a one-second bounded runtime shutdown rather than waiting indefinitely on silent stdin. The subprocess regression holds stdin open and checks bounded cleanup. Tests also cover actual Editor input/order/history, official SDK tool journeys, both lifecycles, cancellation, deadlines, floods, framing and stalled output. They use real Editor/egui state without an OS window. Native protocol/window verification and cross-platform graphical interaction remain separate evidence requirements.

EOF automatic close is deferred while native input remains unprocessed; its close request is retained until visible consumption. Native verification of the initial server reproduced background/minimized timeouts because its queue drained only in the UI callback. The revised actual `App::logic` path passes minimized/occluded-state tests, pending-input baseline comparisons and hidden close/cancellation tests. [Scoped macOS native verification](mcp-native-verification.md) confirmed minimized requests after the correction, while a later idle deadline and recovery leave broader responsiveness unverified.

Run the official client suite explicitly so it is not omitted by feature gating:

```sh
cargo test --locked --features mcp-test-client
cargo clippy --locked --all-targets --features mcp-test-client -- -D warnings
cargo clippy --locked --all-targets --features mcp -- -D warnings
cargo build --locked --features mcp
```

No-default core builds remain independent of desktop and MCP dependencies. See [build checks](builds.md) for CI and the exact dependency license references.

## Capability synchronization

Every existing and new user-facing capability and its relevant state must be accessible through MCP, including while catch-up is in progress. Each applicable feature slice includes its typed operation, MCP schema/handler and inspection changes, real Editor protocol tests and documentation before the completed feature gate. The feature owner coordinates protected editor boundaries and adapter review with the MCP owner. Do not advertise a tool before the operation exists or leave MCP support as a permanent later backlog.

The current catalog omits UVs, multiple-face gestures, GUI Export, native-UI material assignment, material rendering beyond the flat base colour, animation, simulation and scripting. Face mode and one full canonical face target are exposed through the same operations as native Center selection. Its single material-data edit is the bounded existing Phong diffuse scalar described above. `extrude_face` exposes the existing bounded single-quad operation through an explicit current target; native face-extrusion UI remains unsupported. Core-only APIs and unsupported controls do not imply an advertised MCP tool. The current binary decoder refuses shared-instance MB input, and the native UI cannot create instances; SDK ambiguity behavior has no reachable producer in this slice. Canonical path/ambiguity checks and reachable explicit-path protocol tests do not establish full instanced-file support.

Nine verified tools establish the initial server foundation, not catch-up with every existing operation. Existing capabilities without tools need reviewed follow-up slices; their absence does not defer MCP support for new applicable features, including the native face slice.

| Existing capability | Current MCP exposure |
| --- | --- |
| Object create/select/rename/transform/delete, history and framing | Nine tools listed above, including inspection |
| Native New/Open and pending confirmation | `new_scene`, `open_scene` and `resolve_file_action` share native candidate and lifecycle handlers |
| Read-only static cubic Maya ASCII curves | `open_scene` loads the admitted inert class; `inspect_scene` returns canonical class/counts/domain/read-only/prepared-draw metadata, and `select_node`/`frame_selected` use the same owner and camera. Curve picking, CV editing, native writing and broader ASCII evaluation remain unsupported |
| Native Close | `request_close` and matching pending resolution use the native close guard and response-delivery barrier |
| Native Save/Save As | Provisional `save_scene`/`save_scene_as` and matching pending Save; source-bound Unix binary only, current integration/native Save QA remains pending |
| Core OBJ export | No tool; file authority and create-new/no-clobber behavior need a dedicated slice |
| Polygon cylinder and cone creation | `create_primitive` with `cylinder` or `cone` dispatches the same `Operation::Create` as Create > Polygon cylinder/cone; the new node, its inspected counts, and face view, pick, select (including the true 20-gon caps through the same ViewScene n-gon path) and refused cap extrusion are covered by actual Editor protocol tests; provisional topology |
| Primitive creation options | `create_primitive` `options` (sizes, subdivisions, axis per primitive) dispatches `Operation::Create` with validated `PrimitiveOptions`, the same typed operation the native path uses; the tagged schema with published bounds, built mesh counts per primitive, invalid, null, mismatched and over-budget refusal as `invalid_params` with the scene unchanged, and the token, draft, modal, close and Maya-scene guards are covered by actual Editor protocol tests; PROVISIONAL bounds. The native Create menu option-box window is presentation only: it dispatches the same `Operation::Create` through the native pipeline, adds no operation, tool or inspected state, and does not block remote operations (only a focused dialog field does, like any text field). Its remembered options are session UI state, not scene state, so a native plain click uses them while an MCP call with omitted `options` uses the defaults. Native visual QA is pending |
| Native Hierarchy/Face/Center selection | `set_selection_mode` (`object`, `face`, `vertex`, `edge`, `hierarchy`), `select_face`, `select_vertex`, `select_edge` and selection inspection (`remembered_face`, `remembered_vertex`, `remembered_edge`) share authoritative typed state; `select_node` stays exact and is not mode-limited. Native Vertex and Edge modes are reached with F9, F10, the Select > Components menu and a plain click, which dispatch the same operations as `select_vertex` and `select_edge`; the Status Line Vertex, Edge and Face buttons dispatch the same operations, so they add no tool; UV and Vertex Face masks and multi-component gestures are unsupported |
| Core mesh extrusion | `extrude_face` exposes the checked single-quad History command with an explicit current face target; native face-extrusion UI remains unsupported |
| Native Select/Move tool and World-axis object translation | `begin_manipulation` (`kind` `move`), `update_manipulation` (`distance`) and `finish_manipulation` share the native transaction and guards; bounded tool/active preview/capture state is inspectable. Candidate native/SDK verification remains pending |
| Native Tool Box Select, Move, Rotate and Scale buttons | The icon-only Select (Q), Move (W), Rotate (E) and Scale (R) buttons dispatch `Operation::SetActiveTool`, the same typed operation as `set_active_tool` (`select`, `move`, `rotate` or `scale`), with the same closing and modal guards |
| Native Tool Box Rotate (E) and Scale (R) | Object and World ring/box drags and a numeric row in the native UI. Each tool keeps its own space, as Maya does: a fresh Rotate starts in Object and a fresh Scale in World (Maya 2026 Rotate and Scale tool settings pages state these defaults); `begin_manipulation` always names the space, so remote callers are unaffected. The per-tool space is read from `inspect_scene` `manipulation.rotate_space` and `scale_space` and set with `set_tool_space`, which shares `Operation::SetToolSpace` with the native selector. `set_active_tool` accepts `rotate` and `scale`; `begin_manipulation` (`kind` `rotate` or `scale`), `update_manipulation` (`angle` or `factor`) and `finish_manipulation` use the same typed operations, guards and history entry; `inspect_scene` `manipulation` reports the kind, space, axis or handle, baseline and current channels, units and the last refusal. Component rotate/scale, pivots, Gimbal/Parent/Normal/Custom spaces, view ring and free rotate are unsupported with no tool. Covered by Editor protocol and official SDK tests; native visual verification is pending |
| Native Tool Box Lasso, Paint Selection and Last Used | Visibly disabled and unsupported in the native UI ("not implemented yet" hover text); no operation and no MCP tool, and none is advertised. Last Used dynamic behaviour is also unsupported |
| Interactive camera orbit/pan/dolly | Three navigation tools use the same typed operations as native captured gestures; live camera state is inspectable |
| Viewport Grid visibility | `set_grid_visibility` shares the checkable native Grid button's typed setter; `grid_visible` exposes the authoritative session state. Fixed grid only; options, global/additional panels and persistence are unsupported |
| Viewport Shading (Wireframe, Smooth Shade All) | `set_shading_mode` shares the native Shading menu's typed setter; `shading` exposes the authoritative session state. Wireframe on Shaded, Use default material, Hardware Texturing and Use All Lights are visibly unsupported and have no tool or mode value |
| Status Line sidebar and selection toggles | The Channel Box and Attribute Editor toggles are supported through `set_sidebar_panel_visibility` (state in `inspect_scene` `sidebar_panels`); the Hierarchy (Root sub-mask only), Object and Component selection modes and the Vertex, Edge and Face component-type buttons use `set_selection_mode` (the tool's `face` mode is the MCP mapping for entering Face; the Status Line Component button instead keeps the current component mode and enters Face only from Object or Hierarchy; the type buttons are exclusive modes, not Maya mask toggles; there is no UV button). The menu-set selector is native-only and has no state, so no tool (only Modeling is active). The selection mode menu (presets) is native-only, disabled, stateless and has no tool. Modeling Toolkit, HumanIK and Tool Settings are visibly unsupported and have no tool |
| Attribute Editor panel, tabs and Ctrl+A | The panel (transform, direct shape and material tabs; Transform tab shares the Channel Box drafts and edits) is switched by `set_sidebar_panel_visibility` and its tab by `select_attribute_editor_tab`; state is `sidebar_panels.attribute_editor` and `attribute_editor_tab`. Transform and Diffuse edits use `edit_node`, `set_transform` and `set_material_diffuse`. Pin Tab, connected-node tabs, Notes, Extra Attributes, Add/Show/Hide Attributes, Presets and the other attribute sections are visibly unsupported and have no tool; Ctrl+A is a native key event dispatching the same setter |
| Session materials (#51) | `create_session_material`, `assign_session_material` and `set_session_material_color` use the shared typed operations and History commands. Session-only materials are not saved and apply to authored (Untitled) shapes only; whole-shape semantics cover every instance. Material ids are decimal table indices valid only at the current history position, because undo and redo can free and reuse them, so clients must never cache one across edits. `inspect_scene` reports `session_materials` (id, name, shading group, linear colour, editable; entry 0 is `standardSurface1` on `initialShadingGroup`, modelled as a placeholder lambert colour with no type field, and later entries are lamberts) and each shape's `session_material` id, both empty or null on sourced scenes. Typed error codes: `unknown_session_material`, `invalid_session_color`, `initial_session_material_locked`, `unsupported_edit` (sourced scene), `read_only_format` (Maya ASCII), `unknown_shape`, plus the shared `stale_scene`, `busy_editing` and `busy_manipulation`. Native UI is missing (blocked on the Attribute Editor, B3); the viewport shows the colour under the approximate default lighting only. This is a slice, not Maya material parity |
| Status Line file buttons | New, Open and Save use the existing `new_scene`, `open_scene` and `save_scene` tools; the buttons and the File menu share one native handler. Save As has no strip button |
| Status Line absent groups | Per-type selection masks, snaps, symmetry, construction history, render buttons, Hypershade and the input box are not drawn, so there is nothing to expose and no tool |
| GPU geometry viewport | `inspect_scene` exposes live backend/error/depth/clipping/appearance state; camera and object selection share the native frame consumer. Materials and complete appearance remain unsupported |

Initial catch-up remains incomplete while these implemented capabilities lack their reviewed MCP support. Full Maya parity remains a separate unmet objective.

## Coverage inventory

Static source audit of commit `b8d3dd7f985527881ecae186b76d794564708f79` (origin/main), read on macOS. No native run, no MCP session and no Maya comparison were performed, so this table says where a control or tool exists in the source and nothing about behaviour or parity. It covers the registered catalogue (`src/mcp/registry.rs` and `src/mcp/features/`) against the native UI (`src/app/menus.rs`, `panels.rs`, `shortcuts.rs`, `features/`, `src/viewport_ui.rs`) and the `inspect_scene` projection (`src/app/inspect.rs`). It is not a Maya feature inventory (#6) and says nothing about features the native UI does not draw.

Status values: `supported` means a registered tool or an `inspect_scene` field exposes it. `unsupported` means the native UI shows the control visibly unsupported or disabled, or the capability has no native control, so MCP needs nothing. `missing` means the native UI or editor state implements it and MCP lacks it; each missing row names the open issue that closes it. The Tools column holds only registered tool names; the test `inventory_tools_match_registry` fails when a name there is not registered or a registered tool appears in no row. Keep that column to tool names and put state fields in the State column.

| Area | Capability | Status | Tools | State | Issue |
| --- | --- | --- | --- | --- | --- |
| Inspection | Read committed editor state, outliner hierarchy, drafts and modal state | supported | `inspect_scene` | `nodes`, `dirty`, `has_draft`, `native_input_pending`, `text_focused`, `modal` | |
| Files | New scene (File, Status Line) | supported | `new_scene`, `resolve_file_action` | `pending_action` | |
| Files | Open `.mb`, read-only static `.ma`, legacy `.aztec` | supported | `open_scene` | `document_origin`, `read_only_reason` | |
| Files | Save | supported | `save_scene` | `save_available`, `save_reconciliation_required` | |
| Files | Save As | supported | `save_scene_as` | `save_as_available` | |
| Files | Close and quit guard | supported | `request_close` | `closing` | |
| Files | macOS application-menu Quit (Command-Q): dirty-scene prompt and Cancel natively verified, Discard code-traced, edge cases pending | supported | `request_close` | `closing` | Native menu action routed to the shared close guard; remotely reachable only as `request_close`. Menu click, clean scene, panels and minimized window are unverified. Dock Quit, logout and shutdown are not covered (known-issues) |
| Files | Export and OBJ export | unsupported | | | |
| Files | Maya ASCII and new-scene Save, ASCII writing | unsupported | | | |
| Create | Polygon cube, plane, sphere, cylinder, cone (Create menu, shelf) | supported | `create_primitive` | `nodes` | |
| Create | Other primitives, Bevel, shelf Materials, Mesh and other placeholder menus | unsupported | | | |
| Modeling | Extrude face (shelf button disabled; operation exposed) | supported | `extrude_face` | `shapes` | |
| Modeling | Construction history and Inputs | unsupported | | | |
| Nodes | Rename (Channel Box name) | supported | `rename_node` | `nodes` | |
| Nodes | Translate, rotate and scale channels; on an opened `.mb`, an absent translate leaf is inserted at save when the node has a recorded site, while rotate, scale and siteless nodes refuse (`absent_channel_leaf` from `begin_manipulation`, `scene_error` from `edit_node` and `set_transform`) | supported | `set_transform`, `edit_node`, `begin_manipulation`, `save_scene`, `save_scene_as` | `nodes`, `units` | #19 |
| Nodes | Delete (root leaf nodes of authored or new scenes) | supported | `delete_node` | `nodes` | nodes with children or a parent are refused with `unsupported_delete` (#187); on an opened `.mb` a root leaf is refused with `scene_error` because binary sources reject structural edits; an opened ASCII scene is refused with `scene_error` (read-only) |
| Edit menu | Delete by Type, Delete All by Type (visible disabled rows, see #187) | unsupported | | | |
| Nodes | Visibility edit | unsupported | | `nodes` | |
| Nodes | Rotate order, axis, shear and pivots (read only) | supported | | `transform_attributes` | |
| Selection | Select object, clear selection, Outliner click, viewport pick | supported | `select_node` | `selected_node`, `selected_instance_path` | |
| Selection | Selection mode (Object, Face, Vertex, Edge, Hierarchy), F8 to F11 | supported | `set_selection_mode` | `selection_mode` | |
| Selection | Face, vertex and edge component targets | supported | `select_face`, `select_vertex`, `select_edge` | `remembered_face`, `remembered_vertex`, `remembered_edge` | |
| Selection | Multi-select, marquee, Lasso, Paint Selection, UV mask | unsupported | | | |
| Selection | Select > Components > UVs (menu item) | unsupported | | | |
| Selection | Hierarchy Leaf and Template submasks (Status Line hover) | unsupported | | | |
| Camera | Orbit, pan, dolly | supported | `orbit_camera`, `pan_camera`, `dolly_camera` | `camera` | |
| Camera | Frame selected (F) | supported | `frame_selected` | `camera` | |
| Camera | Frame All (A, View menu) | supported | `frame_all` | `camera` | |
| Camera | Camera pose read (target, eye, distance, yaw/pitch, vertical field of view, clip planes) | supported | | `camera` | |
| Camera | Orthographic views, field-of-view edits, restoring a saved view | unsupported | | | |
| Tools | Select, Move, Rotate, Scale tool switch (Q W E R, Tool Box) | supported | `set_active_tool` | `manipulation.active_tool` | |
| Tools | Numeric Move, Rotate and Scale and drag manipulation, cancel | supported | `begin_manipulation`, `update_manipulation`, `finish_manipulation` | `manipulation` | |
| Tools | Rotate and Scale tool-settings space selector | supported | `set_tool_space` | `manipulation.rotate_space`, `manipulation.scale_space` | |
| Tools | Persistent Rotate axis and Scale handle selection (tool-settings combo) | missing | | | #216 |
| Tools | Gimbal, Parent, Normal, Custom and Component spaces; Move orientation modes other than World (listed disabled in the native Move row) | unsupported | | | |
| Tools | Lasso, Paint Selection, Last Used buttons | unsupported | | | |
| History | Undo and Redo | supported | `undo`, `redo` | `can_undo`, `can_redo` | |
| History | Undo and Redo command names (Edit menu hover) | missing | | | #188 |
| Materials | Phong diffuse edit (Channel Box, Attribute Editor) | supported | `set_material_diffuse` | `shapes` | |
| Materials | Session materials (create, assign, colour; MCP and `inspect_scene` only, no native UI) | supported | `create_session_material`, `assign_session_material`, `set_session_material_color` | `session_materials` | |
| Materials | Material rendering, shading networks, textures | unsupported | | | |
| Viewport | Grid visibility | supported | `set_grid_visibility` | `grid_visible` | |
| Viewport | Shading mode (Wireframe, Smooth Shade All) | supported | `set_shading_mode` | `shading` | |
| Viewport | Hardware Texturing, Use All Lights, Wireframe on Shaded, Use default material | unsupported | | | |
| Settings | Sidebar panel visibility (Channel Box, Attribute Editor, Ctrl+A) | supported | `set_sidebar_panel_visibility` | `sidebar_panels` | |
| Settings | Attribute Editor tab | supported | `select_attribute_editor_tab` | `attribute_editor_tab` | |
| Settings | Attribute Editor Pin Tab (disabled button) and read-only shape attributes | unsupported | | | |
| Settings | Modeling Toolkit, HumanIK, Tool Settings panels, menu sets, selection presets | unsupported | | | |
| Settings | Preferences and Working Units | unsupported | | | |
| Panels | Time Slider and animation controls, Layer Editor, Outliner filters | unsupported | | | |
| Panels | Status line message text | missing | | | #214 |

Outcome: the registered catalogue covers every implemented file, creation, selection, camera, tool, history, material, viewport and panel operation except the three missing rows (#188, #214, #216), each linked to its issue; the catch-up stays open until those close. Issue #161 asks for camera pose: `inspect_scene` `camera` now also reports orientation (yaw/pitch), the vertical field of view and the clip planes. Restoring a saved view (there is no absolute camera-set tool) remains open in #161.

## Protocol references

- [MCP stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)
- [MCP tool results](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [Pinned official Rust SDK](https://docs.rs/rmcp/3.5.1/rmcp/)

## World-axis Move candidate

Select and Move are real native tools, activated with their vector buttons or
unmodified Q/W outside text fields and dialogs. Move supports one selected full
transform instance in object mode, with World X/Y/Z shafts only. Rotation, scale,
plane/center handles, components, snapping, alternate orientations and modified
drags remain unsupported. Unsupported transforms, driven data, selected pivots,
unstable parents and source translations without retained existing words refuse
the gesture. This is a bounded application policy, not verified Maya parity.

The three manipulation tools share native Editor operations and one canonical History
transaction for Move, Rotate and Scale. `begin_manipulation` returns a decimal
`manipulation_id` bound to the current scene token and remote ownership. Numeric begin may retain the Select tool. Updates
supply an absolute value from begin, not an increment: a Move `distance` in the
document's declared linear unit, a Rotate `angle` in its declared angular unit
(Rotate and Scale never use the linear conversion) or a unitless Scale `factor`. A commit produces at most one Undo entry; zero commits and
cancellation preserve the existing Redo branch. Native geometry uses centimeters
directly, with one checked conversion for declared SDK distances.

During a gesture, inspection reads the live authoritative preview. Unrelated
remote mutations, Save, Undo/Redo and camera operations return
`busy_manipulation`. Only matching remote-owned updates/finish are accepted;
remote calls cannot finish a native drag. Viewport Grid visibility, shading mode, sidebar panel visibility, selection mode and
Face selection are likewise refused with `busy_manipulation` while any manipulation is
active. Human Escape, tool/selection/selection-mode/lifecycle
changes cancel a native drag before their existing native guard path. Move, Rotate
and Scale are Object-mode only: while Face, Vertex or Edge selection is active
`begin_manipulation` returns `invalid_input`, native handles are not drawn or hit, and `inspection.manipulation.kinds`
reports each kind unavailable with the reason. Native captured mapping,
camera, DPI, modifier, focus or release loss cancels and consumes the pointer
sequence. A finite geometric refusal, or a refused final preview update, on a valid explicit
release commits the last validated preview, with a visible refusal and last-valid
status; it never
applies a substitute motion.

`inspection.manipulation` reports the active tool; `units` (`distance` and
`angle` as in `inspection.units`, `factor` `unitless`); `kinds`, one entry per
kind with `available` and the core's `unavailable_reason` (a kind is available
when any of its handles could begin; Rotate and Scale use the current
`rotate_space` and `scale_space`, so a world-axis scale on a rotated node, or a
world rotation of a node with non-positive scale, reports the refused handles.
Inspection reports availability for the tool's current space, while
`begin_manipulation` takes an explicit `space`, so a caller passing a different
space can get a different result); the `active` manipulation
with `manipulation_id`, `kind`, node and instance path, `space`, `axis` or
`handle`, `provenance`, the `parameter` name, its `value` and `unit` (after a remote update, `value` is the requested number exactly as given, in scene units; after a native drag it is derived from the internal parameter), `channels`
(`translate`, `rotate` or `scale`) with the stored `baseline_channels` at begin
and the live `current_channels`; and `refusal`, the reason the scene refused the
last update of the active manipulation (the preview is unchanged), cleared by the
next accepted update. Its optional `viewport` is Move-only and explicitly
labelled `last_native_viewport_capture`. It separates visible axis geometry and
unavailable reasons from the last captured pointer's optional hit. Missing or
stale capture is unavailable. Visible geometry is not a global pickability or
Ready claim, especially in tiny viewports and the excluded center region.
Baseline Scalars and source bytes are never exposed.

This source candidate includes native-event and official-SDK regression tests.
Their execution, native framebuffer/desktop proof and current-parent publication
remain separate completion gates until recorded below; source wiring alone is
not verification.
