# Development

## Checked material scalar editing candidate

M1 stores one authoritative effective f64 diffuse value for each supported shared
Phong shader and existing eight-byte source span. Immutable appearance metadata
and original source bytes remain separate. The Channel Box's Materials/Common
Material Attributes section targets an explicit direct mesh shape; several direct
shapes require a choice. Connected Color and material rendering stay unavailable.
Diffuse text retains the unrounded source value, including unusual original
values; edits must be finite and nonnegative and may exceed one. Slider bounds
apply only to interaction. Its original document token and shape target travel
with the draft through the existing commit, focus, Escape, selection, Undo and
Save barriers. Remote mutations cannot commit that human draft.

The isolated branch passed default248, headless192 and MCP-client314 tests, plus
formatting and strict default/headless/client/production lint. Actual widget and
SDK tests cover draft targeting, precise Enter/Escape, History and repeated Save.
SDK edit/inspection/History assertions remain portable; persistence assertions
run on Unix and Windows, matching the native writer's supported platforms.
The independent viewer source-word oracle passed; three isolated core mutations
each failed at their intended assertions and passed after exact restoration.
Those private-resolver sensitivity runs are separate from the ordinary matrix.
The corrected SDK test and independent verifier also exercised actual protocol
Open of saved outputs and the new document token/origin. Root's bounded macOS
native edit/Undo/Redo/Save As/Open journey passed, with independent byte checks
limited to the authorized eight-byte word and unchanged original fixtures.
Cold verification of the committed source on main (`7ca0ebf`, 2026-10-19, see
[native-verification.md](native-verification.md)) reproduced the tests above and a
validation and a byte-patch mutation failure, and CI on that commit passed. A saved Save/NCRV/Face/Grid
composition is NOT covered and cannot be built today: binary Save refuses scenes
with curves and ASCII Save is refused. Data editing does not imply evaluated Phong output,
textures, normals or color management.

Install Rust 1.99.0 with Cargo, rustfmt and Clippy. The repository pins the same
toolchain for local rustup users and CI. The user has authorized publishing the
early prototype, superseding the original first-push restriction. Full Maya
functionality coverage and exact parity remain unmeasured and unmet.

| Command | Purpose |
| --- | --- |
| `cargo run --locked` | Build and launch the default desktop target |
| `make check` | Check formatting, lint, run tests and build including desktop |
| `make fmt` | Format Rust source |
| `make test` | Run core workflow and desktop UI event tests |
| `cargo test --locked --no-default-features` | Run core suites without the desktop target or a window |
| `cargo test --locked --test scene_workflows` | Run scene tests only |
| `cargo test --locked --test history_workflows` | Run command history tests only |
| `cargo test --locked --test document_workflows` | Run retained legacy JSON utility tests |
| `cargo test --locked --test viewport_workflows` | Run camera/projection/picking tests only |
| `cargo test --locked --no-default-features --test obj_workflows` | Run OBJ export tests only |
| `cargo fmt --manifest-path vendor/craft-atomic/Cargo.toml --all -- --check` | Check vendor formatting with inherited upstream settings |
| `cargo clippy --manifest-path vendor/craft-atomic/Cargo.toml --all-targets --locked -- -D warnings` | Lint the standalone vendor crate |
| `cargo test --manifest-path vendor/craft-atomic/Cargo.toml --locked` | Run actual vendor unit/integration/doc tests |
| `cargo fmt --manifest-path crates/macos-quit-menu/Cargo.toml --all -- --check` | Check the macOS Quit-menu crate formatting |
| `cargo clippy --manifest-path crates/macos-quit-menu/Cargo.toml --all-targets --locked -- -D warnings` | Lint the macOS Quit-menu crate |
| `cargo test --manifest-path crates/macos-quit-menu/Cargo.toml --locked` | Run the crate tests: the platform-neutral terminate logic runs on every OS, the `harness = false` terminate hook test only on macOS (a no-op elsewhere) |

[Build and security notes](builds.md) describe the prepared three-platform CI,
dependency audit, code line coverage and pre-commit configuration. Remote CI and
native platform results need retained run evidence rather than inference from
the configured matrix. Code line coverage is separate from Maya workflow
coverage and remains unsuitable for a functionality badge or parity claim.

### Download CI builds

CI builds are workflow artifacts, not releases, and do not appear on the Releases page (creating releases needs separate authorization). To get binaries for a commit, open Actions, then the Rust checks run for that commit, then its Artifacts section. Artifacts are named `azteccraft-<platform>-<arch>-<commit>` for macOS aarch64 and x86_64, Linux x86_64 and Windows x86_64, and expire after 7 days. GitHub wraps each artifact in a zip, so the download is a zip containing the real archive (`.zip` for macOS and Windows, `.tar.gz` for Linux) plus `manifest.json` with SHA-256 hashes. They are default-feature builds without MCP.

The builds are not notarized. macOS: unzip the archive, try to open the app once, then choose System Settings, Privacy & Security, Open Anyway; the button appears for about an hour after the attempt ([Apple's instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac)). From a terminal, `xattr -dr com.apple.quarantine AztecCraft.app` removes the quarantine instead. Right-click, Open no longer works on macOS 15 and later. The bundle is ad-hoc signed for integrity only, which is not a Developer ID signature. Windows: SmartScreen warns about an unrecognized publisher; choose More info, then Run anyway. Linux: run `chmod +x azteccraft` if the executable bit was lost. The x86_64 macOS build is cross-compiled and not launched in CI. CI only runs the binary's usage exit, so graphical launch is unverified.

Optional local hooks: install `pre-commit`, then run `pre-commit install` and
`pre-commit run --all-files`. Rust checks use the pinned repository toolchain.
The lightweight private-key hook is not a comprehensive secret or dependency audit.

The crate provides a reusable scene library and a native binary behind the
default `desktop` feature. eframe 0.36.2/egui use wgpu with X11/Wayland enabled,
and rfd supplies file dialogs. Desktop launch needs a window system and compatible
graphics backend. Linux needs the selected window/dialog backends' system
packages, and each claimed platform needs clean build and native interaction
checks. Core-only commands use `--no-default-features` and require no display or
runtime services.

The prototype provides menus/shelf, Outliner, Channel Box, projected mesh drawing,
selection, name/numeric edits, undo/redo, delete and framing. Alt-left drag orbits,
Alt-middle drag tracks (pans), Alt-right drag dollies, the wheel dollies without Alt,
and plain middle/right drags are unbound (reserved for #37 and #32), and F frames the active object or face when
a text field isn't consuming keys. Ctrl/Cmd-Z undoes and Ctrl/Cmd-Shift-Z redoes.
New/Open use the shared native document lifecycle. This candidate enables
eligible Unix and Windows Maya binary Save/Save As through that same lifecycle and ordinary
vendored writer; current integration and native Save QA remain pending. No derived JSON save is substituted.
Unfinished modeling/animation/material controls are disabled.
Docking and full Maya interaction parity aren't implemented or verified.

The checkable Grid button controls this single viewport's fixed XZ ground grid.
It defaults on and retains its session value across New/Open, including canceled
file actions. It shares `SetGridVisible` with MCP `set_grid_visibility`; the
explicit boolean setter is idempotent and inspection returns `grid_visible`.
This is visual state outside NativeScene and authoring History. It has no default
shortcut, grid options (Display > Grid Options is disabled), additional/global panels or
settings persistence. The grid follows Maya 2025.1's default options; its extent reading,
colours and Maya reference parity are PROVISIONAL (see the #160 entry in known-issues.md).

`NativeScene` is the editor and History's sole mutable authoring state. It owns
separate transform/shape identities, instance DAG edges, f64 channels and pivots
in declared units, and immutable original source bytes with typed edit intent.
`ViewScene` derives bounded read-only triangle/quad instances and evaluated static
cubic curve segments, preserving original face indices and full instance paths.
Unsupported evaluation or topology refuses
projection or editing explicitly; it isn't flattened into a second authoring mesh.
Local mesh points and sparse offsets remain separate centimeter-valued storage;
both are promoted to f64 before addition/world transformation. Native values
outside f32 view representation remain authoring data and yield an explicit view error.

The retained legacy `Scene` utility owns root nodes with stable IDs,
read-only mesh access and validated rename/transform mutations. Cube and plane
primitives are unit quads in a right-handed Y-up coordinate system. Transforms
apply scale, XYZ Euler rotation in degrees, then translation. XYZ rotates about
the fixed X, Y, then Z axes, matching Maya's default row-vector `RX * RY * RZ`
convention as documented in its
[transformation API](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/class_m_transformation_matrix.html).
With glam column vectors this uses `RZ * RY * RX`. Negative and zero
scales are supported; a reflected transform also reverses geometric winding.
Hierarchy editing (grouping, reparenting) remains a subsequent task. The Hierarchy selection mode only selects root transforms; grouped hierarchy exists only in opened `.ma`/`.mb` files, where structural edits such as Delete on a group are refused.

`mesh::Mesh` stores quad vertex/index arrays and remains available as `scene::Mesh`.
Scene validation requires finite local/world coordinates, index bounds and four
distinct indices per face. It preserves unused vertices, empty meshes, coincident
or zero-area geometry, nonplanarity, inconsistent winding and non-manifold states.
Loading edited legacy arrays remains supported. This quad-only utility is not
the native editor's authoring model.

`Scene::extrude_face` and the native `Command::ExtrudeFace` subset extrude one quad by signed
object-space displacement along its oriented Newell normal, with one segment.
The cap replaces the selected face at the same index and four side quads are
appended. Zero or rounded-to-coincident displacement still creates undoable
topology. A mesh with only one face, such as the legacy Scene plane or a
1x1-subdivision native plane, returns an explicit unsupported-all-faces error;
its Maya default behavior needs a pinned fixture. Undefined normals, invalid
indices/distances and coordinate overflow
leave the scene/history unchanged. World/region extrusion, divisions, taper,
offset, UVs/normals, construction-history nodes and native Extrude controls remain
pending. Tests establish this subset's geometry, not Maya parity.

`examples/extruded-cube.aztec` was generated through cube creation and two
History extrusion commands: front face 1 by +0.5, then its right side face 7 by
+0.5. Integration tests reconstruct the 16-vertex/14-face closed quad mesh and
verify undo/redo, save/reload, reflected OBJ cap/side geometry and picking outside
the original cube. File → Open provides a native inspection path; it does not
add native face-editing controls or prove Maya equivalence.

## Face Center selection

F11 or Select > Components > Face enters Face mode; F8 goes from Object or
Hierarchy to Face and from Vertex, Edge or Face to Object (provisional). The active transform contributes only directly attached
triangle/quad shapes in its exact instance path, not descendant meshes under a
selected group. A plain primary click replaces the face or clears it when no
eligible center is hit. The shared typed operations and inspection are available
through [MCP selection tools](mcp.md#tools-and-state).

Each marker sits at the arithmetic mean of the ordered face corners, accumulated
in f64 before conversion to view space (marker and click-box sizes: rows 1 to 3 of
the [policy constants table](#application-policy-constants)). Homogeneous clipping, viewport dimensions and scale use
the same snapped physical capture as rendering. A ray through each eligible
center, rather than the offset click, checks all rendered triangles, including
the face itself, within the rendered near/far range. A closer surface blocks the
center; planar depth equality is admitted. Center-ray occlusion compares linear
view depth with a relative tolerance (row 5), so occluders closer than that to the
center (z-fighting range) do not hide it. A nonplanar face can hide its own
center, while an unobstructed finite degenerate center can remain selectable.
The GPU markers use the existing depth attachment and comparison path.

Picking preflights eligible centers times all occluder triangles with checked
arithmetic and refuses work above the cap in row 4. Nearest surviving
camera depth wins; exact ties use full instance path, transform/shape IDs and
original source face index. These fixed application policies are in the
[policy constants table](#application-policy-constants); they have not been verified against Maya's
Center/CBS behavior or numerical boundaries.

Object mode suppresses component feedback but remembers a still-valid face on
the same object/path. Switching object/path or successfully replacing a document
clears it; New/Open retain the current mask. Edits and Undo/Redo revalidate the
target and clear dangling identities without restoring them on Redo. Selection
does not modify authoring history, source bytes or dirty state. F/frame_selected
in Face mode frames the selected face corners even if its center is occluded or
clipped; no face returns an error instead of framing the object. In Object and Hierarchy
modes F frames the selected parents only and Ctrl+F includes their descendants. Existing native
valid-draft commitment and remote draft-refusal rules remain in force.

Shift/Ctrl component selection, marquee, multiple faces, other component masks, preselection
hover and marking menus are unsupported. Ambiguous coalesced primary-release
frames refuse selection with a single-click retry message; they do not guess an
event owner or commit a draft solely for that refusal. Alt camera navigation
remains on its established route. Delete refuses the whole object under any
component mode (Face, Vertex or Edge), natively and over MCP, with the same
shared guard; return to Object or Hierarchy selection to delete the object.
Native Extrude stays disabled pending real history/manipulator behavior.

Focused actual framebuffer proofs can be selected with `cargo test --locked
--bin azteccraft headless_gpu_face_ -- --ignored --test-threads=1`. CPU event,
protocol and framebuffer tests establish their exercised callers, not an OS
window journey or Maya parity. The attempted genuine combined menu
activation/key/viewport-click sequence was blocked by pinned egui's popup
routing. The retained proofs cover menu+F8 and F11+F8+viewport click separately, plus blocked-popup
nonmutation. Full coalesced-event support and platform/reference comparison
remain follow-up work.

## Vertex and Edge selection

F9 and F10, Select > Components > Vertex / Edge, the Status Line Vertex, Edge and
Face buttons and a plain primary click enter and use Vertex and Edge modes (#29);
`set_selection_mode` (`vertex`, `edge`), `select_vertex` and `select_edge` reach the
same typed operations over MCP. Each component type keeps its own remembered pick,
active only in its mode. Picks are single, replace-only and clear on an empty click
(UNCONFIRMED for Maya). In component modes Shift/Ctrl clicks are refused with `ModifiedClick`; UV, Vertex
Face (Alt+F9), Multi-Component and masks are unsupported. F8 leaves Vertex and Edge
for Object (provisional). Constants and limits are in
[known-issues.md](../known-issues.md#vertex-and-edge-picking-and-drawing-constants-are-provisional);
nothing here is compared against Maya.

Where the code lives:

| Concern | Location |
| --- | --- |
| Target resolvers | `resolve_vertex_target`, `resolve_edge_target` in `src/native_scene.rs` |
| Display edges and vertex data | `ViewInstance::edges` (`view_edges`) in `src/view_scene.rs` |
| Picking and occlusion | `Camera::pick_vertex`, `Camera::pick_edge`, `VERTEX_MARKER_DEPTH_TOLERANCE` in `src/viewport.rs`; pixel wrappers in `src/viewport_ui.rs` |
| Markers and edge drawing | `vertex_markers`, `selected_edge`, `edge_quad` in `src/viewport_renderer.rs` |
| Typed operations | `Operation::SelectVertex` / `SelectEdge` in `src/app/dispatch.rs` (modal and closing refusal in the guard above the handlers) |
| Keys, click | F9 / F10 and the viewport click in `src/app/panels.rs`; Select > Components in `src/app/menus.rs` |
| Status Line buttons | `COMPONENT_TYPES` in `src/app/features/status_line.rs`; icons in `src/ui_icons.rs` |
| MCP | `select_vertex`, `select_edge`, schemas and inspection in `src/mcp/features/selection.rs`; SDK tests in `src/mcp_tests/official_client/vertex_edge.rs` |

Tests. App, Editor and GPU tests live in the bin target, so use `--bin azteccraft`; a
`--lib` filter silently matches zero. Icon goldens (`src/ui_icons/tests.rs`) are in the
lib target. SDK tests need `--features mcp-test-client`.

```
cargo test --locked --bin azteccraft --features mcp-test-client vertex
cargo test --locked --bin azteccraft --features mcp-test-client edge
cargo test --locked --lib ui_icons
cargo test --locked --bin azteccraft --features mcp-test-client vertex_edge -- --include-ignored --test-threads=1
```

The last command also runs the GPU readbacks in `src/viewport_renderer/tests/vertex_edge.rs`,
which need an adapter and fail without one. They pin the 6e-3 outcome only; the 5e-3 and
2e-2 bracket in known-issues came from manual readbacks. Passing adapter tests are not
a native window journey.

Regenerating committed outputs: `UPDATE_SNAPSHOTS=1` rewrites `tests/snapshots/*` (guard
matrix, tool catalog, inspection bytes; review the diff and confirm existing cells are
unchanged), and `UPDATE_DOCS=1 cargo test --features mcp registry::tests::tool_doc`
rewrites `.project-docs/mcp-tools.md`.

Native evidence for the Status Line buttons is recorded in
[native-verification.md](native-verification.md#vertex-edge-and-face-status-line-buttons-native-check).

## History

`History::scene()` returns the authoritative `NativeScene`. Its explicit commands
include one mixed name/nine-channel-intent transaction and shape-targeted extrusion;
snapshots retain native precision, identities and the shared immutable source Arc.
Failed edits and unchanged values
don't create history entries or clear redo. A changed edit after undo clears the
redo branch, while node ID reservations stay at their highest allocated value so
IDs from discarded branches aren't reused. Memory grows with scene size and edit
count; the undo queue keeps a fixed number of newest entries (row 21 of the
[policy constants table](#application-policy-constants)), and no history persistence exists yet.

## Application policy constants

This table is the single place that records each fixed application policy value, what
evidence exists for it and which test pins it. Other documents point here instead of
repeating the values. It claims no Maya parity. A row has one of three statuses:

- **verified-by-docs**: the Maya online help states the same value and the quote below
  reproduces on the retrieved page. Only the part the quote covers is verified.
- **provisional**: the help is silent (or documents an unrelated object), so the
  choice is ours and a test pins it. The test makes a later change visible; it is not
  evidence about Maya.
- **differs-from-docs**: the help states a value for the same kind of setting, or for the
  closest documented equivalent (for example a menu-created camera, stated in the row), and
  ours is different. #33 changes no behaviour; the divergence is recorded in the row.

Evidence mapping (acceptance criterion 3 of #33):

- **Release.** The quotes come from the Maya 2027 online help (`cloudhelp/2027` URLs).
  `reference.md` pins documentation release 2027.2; the help pages do not say which update
  they describe, so a difference that exists only in 2027.2 would not show here.
- **Retrieved.** 2026-10-26 (row 14's Line pick size entry re-checked against the raw HTML on 2026-10-27). Each page was fetched as raw HTML and the quoted text
  compared against it character by character. A claim that did not reproduce was dropped.
  Quotes are shortened to the part that matters; link text inside a sentence is omitted.
- **Platform.** The pages do not distinguish platforms or display scales.
- **Fixture.** None. No Maya build was run for any row except the grid, whose
  value comes from Maya 2025.1 `mayapy` (`grid -q`), as recorded in `known-issues.md`.
- **Scope.** Each quote covers only the named preference, option or attribute. For
  example the Pick Range quote is about when a manipulator handle highlights; our one
  hit distance serves both the highlight and the click, which the docs do not state.

Units: pixel values are physical pixels of the snapped viewport capture (the pointer is
multiplied by the pixel ratio before comparing), not UI points.

Excluded: other resource budgets that admit or refuse work and have no Maya counterpart
(`VIEW_BUDGET` in `src/app/features/manipulation_tool.rs`, the segment, depth and work
limits in `src/view_scene/curves.rs`, the 16 MiB document input limit). Preferences
the app does not implement are not constants: Select dead space (docs: "By default, Select dead space is set to 4 pixels."), Preselect size, the Handle size and Line size
preferences, and Tumble scale (docs: "The default value is 1.").

Sources:
[Manipulators preferences][maya-manip],
[Resize the manipulator][maya-resize],
[Undo preferences][maya-undo],
[Selection preferences][maya-sel],
[Polygons options][maya-poly],
[Create Camera options][maya-cam],
[Tumble Tool options][maya-tumble],
[`grid` command][maya-grid].

| # | Policy | Value | Defined in | Status | Docs evidence | Pinned by |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Face-center marker | 4 px square | `src/view_scene.rs` `FACE_CENTER_MARKER_PIXELS` | provisional | Polygons: "Show a small square to indicate the face center." No size or unit. | `marker_sizes_are_pinned_to_their_provisional_pixel_values`, `face_submission_highlights_only_exact_identity_and_charges_physical_markers` |
| 2 | Face-center click box | 8 px edge, 4 px each side of the pointer | `src/view_scene.rs` `FACE_CENTER_CLICK_PIXELS` | provisional | Selection: "you must click the box at the center of the face to select the entire face". Click box size: "This option controls the size of the selection area around the mouse pointer, or click box." No value or unit. | `center_click_hits_triangle_and_quad_centers_but_not_the_face_body` |
| 3 | Face center | mean of the ordered corners, summed in f64 | `src/view_scene.rs` `ViewFace::center` | provisional | The docs do not define the center. | `twenty_gon_center_marker_is_the_corner_average_and_clickable` |
| 4 | Center-ray work cap | 1,000,000 eligible centers times occluder triangles; more is refused | `src/viewport.rs` `MAX_FACE_CENTER_INTERSECTIONS` | provisional | No Maya counterpart (a pick cost limit). | `face_center_work_budget_accepts_exactly_the_limit_and_rejects_one_more`, `center_work_boundary_is_enforced_on_the_actual_picker` |
| 5 | Face-center depth tolerance | 1e-4 of linear view depth | `src/viewport.rs` `FACE_CENTER_DEPTH_TOLERANCE` | provisional | Numerical, no docs analogue. | `depth_tolerances_are_pinned_independently`, `center_depth_tolerance_admits_rounding_gaps_but_not_half_percent_occluders` |
| 6 | Vertex and edge depth tolerance | 6e-3 of linear view depth | `src/viewport.rs` `VERTEX_MARKER_DEPTH_TOLERANCE` | provisional | Chosen from readbacks recorded in `known-issues.md`, not from docs. | `depth_tolerances_are_pinned_independently` |
| 7 | Vertex marker | 5 px square | `src/view_scene.rs` `VERTEX_MARKER_PIXELS` | provisional | Polygons: "Adjusts the size that vertices are drawn in the scene view. The default size is 3. The range is from 0.2 to 10." The unit is not stated, so no conflict is claimed. | `marker_sizes_are_pinned_to_their_provisional_pixel_values` |
| 8 | Vertex click box | 10 px edge, 5 px each side | `src/view_scene.rs` `VERTEX_CLICK_PIXELS` | provisional | Same Click box size quote as row 2, no value. | `vertex_pick_uses_the_click_box_and_reaches_loose_vertices`, `vertex_and_edge_thresholds_include_their_exact_limit` |
| 9 | Edge click distance | 6 px from the segment | `src/view_scene.rs` `EDGE_CLICK_PIXELS` | provisional | Same as row 8. | `edge_pick_uses_screen_distance_and_reaches_face_less_edges`, `vertex_and_edge_thresholds_include_their_exact_limit` |
| 10 | Selected-edge strip | 3 px wide | `src/view_scene.rs` `SELECTED_EDGE_PIXELS` | provisional | The docs give no width for the selected-edge highlight. | `marker_sizes_are_pinned_to_their_provisional_pixel_values`, `selected_edge_is_a_three_pixel_strip_on_top_in_its_own_colour_and_stale_indices_draw_nothing` |
| 11 | Vertex and edge candidate ranking | pointer distance, then depth, then source order | `src/viewport.rs` `pick_vertex`, `pick_edge` | provisional | Not documented. | `vertex_pick_prefers_the_nearest_to_the_pointer_then_the_lowest_index`, `coincident_edges_resolve_to_the_lowest_edge_index` |
| 12 | Face tie order | nearest depth, then instance path, transform ID, shape ID, source face index | `src/viewport.rs` `pick_face_center` | provisional | Not documented. | `center_pick_keeps_two_direct_imported_shapes_and_excludes_descendant_shapes`, `center_work_boundary_is_enforced_on_the_actual_picker` (both fail when the tie comparison is reversed in a scratch copy) |
| 13 | Foreground occlusion of a face center | a strictly closer surface hides it; planar equality stays visible; a nonplanar face can hide its own center | `src/viewport.rs` `unoccluded_depth` | provisional | Not documented. | `center_visibility_uses_center_ray_even_for_offset_clicks_and_all_faces_occlude`, `center_planar_equality_is_visible_but_nonplanar_quad_can_occlude_its_own_center`, `center_visibility_uses_rendered_near_far_clipping_for_positive_occluder_hits` |
| 14 | Manipulator hit radius | 8 px (Euclidean), shared by hover, click, Move, Rotate, Scale and the unsupported-handle probe | `src/move_handles.rs` `HIT_RADIUS`, used in `src/manipulator_handles.rs` | value verified-by-docs (8 px default); that it is a Euclidean radius, shared by hover and click and all three tools, and measured in physical pixels, is provisional | Manipulators: "Sets the range within which the cursor must land before the manipulator handle is highlighted. The range is set to 8 pixels by default." Also "Line pick size Determines the line thickness used when picking the rotate manipulator rings. The pick size should be the same as line size, so you can identify which handle will be picked by the size of the ring." (no default, and it is tied to the unimplemented Line size preference. The docs give no Pick Range for the rings; applying the 8 px Pick Range to them is this application's own choice; PROVISIONAL). The docs do not say whether the range is a radius or a box, or physical or logical pixels. | `rotate_and_scale_pick_radius_stays_eight_pixels_at_every_size`, `the_move_pick_radius_stays_eight_pixels_at_every_size`, `front_axes_share_exact_physical_drawing_and_hit_geometry`, `ring_pick_threshold_is_the_pixel_distance_to_the_nearest_segment` |
| 15 | Move handle length | 72 px times the size | `src/move_handles.rs` `ARROW_LENGTH` | provisional | Manipulators: "Handle size Specifies the size of the handle. The range is from 4 to 100." No default or unit. | `move_length_scales_with_size_above_the_floor_and_sizes_below_it_draw_alike`, `axis_length_is_the_full_length_times_its_projected_fraction` |
| 16 | Handle length floor | 36 px (half of row 15) | `src/move_handles.rs` `MIN_HANDLE_LENGTH` | provisional | Not documented. | `move_length_scales_with_size_above_the_floor_and_sizes_below_it_draw_alike` |
| 17 | Manipulator size range | 0.1 to 10 | `src/move_handles.rs` `ManipulatorSize::MIN`, `ManipulatorSize::MAX` | verified-by-docs for the range; how it maps onto our drawn size is provisional | Manipulators: "Specifies the size of the manipulators. The range is from 0.10 to 10.00." | `manipulator_size_accepts_only_finite_values_in_range_and_rounds_to_milli` |
| 18 | Manipulator size default | 1.0 | `src/move_handles.rs` `ManipulatorSize::DEFAULT` | provisional | The page states the range but no default. | `manipulator_size_accepts_only_finite_values_in_range_and_rounds_to_milli` |
| 19 | Size key steps | grid 1.25^k, k from -10 to 10, three-decimal display, held key steps again, session only | `src/move_handles.rs` `SIZE_STEP`, `GRID_LIMIT` | provisional | Resize: "you can use the plus (+) and minus (-) key to grow and shrink it". No step, repeat or persistence. | `key_steps_from_any_grid_value_return_to_it_bit_exactly`, `every_milli_size_steps_to_the_adjacent_visibly_different_grid_value`, `an_exact_tie_steps_by_its_shown_value_and_keeps_the_raw_grid_value`, `key_limits_are_the_ends_of_the_grid` |
| 20 | Rotate and Scale handle geometry | box half 5 px, centre box half 7 px, ring of 64 segments, view ring 1.2 times, edge-on limit 1e-3 (the absolute dot product of the ray and the ring normal, about 0.06 degrees from the plane), pivot grab 1 px, uniform reference 100 px, plane handles at 0.3 to 0.5 | `src/manipulator_handles.rs` constants | provisional | Manipulators: "Plane handle offset Sets the distance between the plane handles and the center of the manipulator." No value. Nothing else is stated. | box `scale_boxes_share_drawing_and_pick_geometry`; ring `ring_pick_threshold_is_the_pixel_distance_to_the_nearest_segment`; view ring `hover_status_names_pickable_refused_and_unsupported_handles_and_restores_the_help`; edge-on `edge_on_limit_sits_between_five_ten_thousandths_and_two_thousandths`; pivot `scale_refuses_a_start_too_close_to_the_pivot`; uniform `uniform_scale_grows_right_and_up_from_a_reference_distance`; plane near `scale_plane_handles_are_drawn_but_only_reported_unsupported`. exact literals for the centre box half, both plane edges, the view ring factor and the edge-on limit `handle_geometry_constants_are_pinned_exactly`. |
| 21 | Undo queue | newest 50 snapshots kept, oldest dropped silently, fixed size | `src/history.rs` `UNDO_LIMIT` | default 50 is verified-by-docs; a fixed queue is differs-from-docs | Undo: "If Finite is the selected Queue setting, specify here the number of undo operations you can perform. The default is 50." and "Select Finite to limit the number of undo operations you can perform (specified in the Queue Size box). The default is Finite." The page also offers "Select Infinite to perform an unlimited number of undo operations." We offer neither choice. What happens to the oldest entry when the queue fills is not stated. | `undo_limit_is_pinned_to_fifty`, `undo_keeps_only_the_newest_limit_entries_and_redo_is_intact` |
| 22 | Undo granularity | one entry per edit and per Move release; selection, selection mode, camera, tool and panel operations add none | `src/history.rs` `History::execute` | provisional | Undo: "When this option is active, all consecutive time/playback commands are considered a single undo step." Nothing says whether selection is undoable. | `selection_mode_camera_tool_and_panel_changes_add_no_undo_entry`, `real_axis_press_absolute_motion_release_makes_one_undo_at_fractional_dpi` |
| 23 | Move cancellation | Escape, a modifier change, focus loss, pointer loss and a window change cancel a Move preview; an Escape that closes a menu does not also cancel it | `src/app/shortcuts.rs` `cancel_on_escape`, `src/app/features/move_tool.rs` `cancel_move` | provisional | No docs for Move. | `escape_modifiers_focus_and_pointer_loss_cancel_real_captured_gestures`, `stationary_release_in_changed_window_origin_size_or_dpi_cancels_without_fallthrough`, `escape_closing_an_open_menu_leaves_the_move_in_flight_and_a_second_escape_cancels_it` |
| 24 | Default camera pose | target 0, yaw 0.6 rad, pitch 0.4 rad, distance 8 | `src/viewport.rs` `impl Default for Camera` | provisional | The pages checked state no default perspective camera pose. | `default_camera_is_pinned_bit_for_bit` |
| 25 | Default field of view and clip planes | film-fit lens: focal length 35 mm, Horizontal Film Aperture 1.4173 in and Vertical Film Aperture 0.9449 in, film fit Fill; vertical fov 37.85 degrees at viewport aspect up to about 1.49995 (1.4173 / 0.9449) and horizontal fov 54.43 degrees above it; near 0.1, far 10000 | `src/viewport.rs` `impl Default for Camera`, `src/viewport/lens.rs` | near, far, focal length and aperture match the Create Camera defaults; the persp camera's own defaults, the Fill threshold and the choice of Fill are PROVISIONAL | Create Camera Options, Near and Far: "The default setting for Near Clip Plane is 0.1 and for Far Clip Plane is 10000." Focal Length: "The default value is 35." Film Aperture: "The Camera Aperture attribute determines the relationship between the Focal Length attribute and the Angle of View attribute. The default values are 1.4173 and 0.9449." Film fit: "Controls the size of the resolution gate relative to the film gate. If the resolution gate and the film gate have the same aspect ratio, then the Film Fit setting has no effect. The default setting is Fill. Fill Fits the resolution gate within the film gate." ([maya-cam]). That page is about cameras created from the menu, not the default perspective camera. PROVISIONAL points: (a) the angle of view tan(half angle) = aperture_in * 25.4 / (2 * focal_mm) is not stated by the docs; it matches the sentence that the aperture sets the relationship between focal length and angle of view. (b) The docs state no Fill formula; we read Fill as: viewport aspect at or above the film aspect (1.4173 / 0.9449) matches horizontally, below it vertically, and treat the viewport aspect as the resolution gate. (c) The heading lists Horizontal then Vertical aperture but the text says "The height and width"; we take 1.4173 as horizontal, which gives the 3:2 film. (d) The camera node page (Maya-Tech-Docs `camera`) lists `filmFit` default 1, which by its own enum ("0 fill, 1 horizontal") is Horizontal, and `mcp.md` records that Maya's persp Angle of View is horizontal, so a horizontal-locked persp is plausible; Fill follows the Create Camera Options text. Not modelled: Horizontal, Vertical and Overscan fits, film offset, camera scale, lens squeeze and overscan (see known-issues). Headless and MCP-only sessions that never painted frame at aspect 1.0. | `default_camera_is_pinned_bit_for_bit`, `film_lens_fill_matches_horizontal_at_or_above_film_aspect_and_vertical_below`, `default_camera_projection_uses_the_film_fit_at_each_aspect`, `frame_all_distance_follows_the_effective_fov`, `inspect_fov_tracks_the_viewport_aspect` |
| 26 | Pitch limit | plus or minus (pi/2 minus 0.001) rad; orbit clamps; `Camera::new` and `set_pose` reject | `src/viewport.rs` `MAX_PITCH` | provisional | Tumble Tool: "The Tumble Tool revolves the camera by varying the azimuth and elevation angles in a perspective view." No limit. | `new_rejects_pitch_beyond_the_orbit_limit`, `set_pose_accepts_the_pitch_limit_and_names_the_bound_in_the_error`, `set_pose_rejects_each_invalid_field_and_leaves_the_camera_unchanged` |
| 27 | Navigation gains | orbit 0.007 rad/px; pan 0.002 times distance per px; Alt-right dolly exp(0.01 per px); wheel exp(0.002 per px); no distance limits beyond positive and representable | `src/viewport_ui.rs` | provisional | Tumble Tool: "The slider range is 0.01 to 10. The default value is 1." for Tumble scale. No baseline in pixels per degree, so our gains cannot be compared. | `captured_typed_navigation_preserves_orbit_pan_and_both_dollies_and_drains_once` |
| 28 | Grid | extent 12 units from the origin, a line every unit, a major line every 5 | `src/viewport_renderer.rs` `GRID_SIZE`, `GRID_MAJOR_EVERY` | size provisional (reading it as a half-length is ours); major spacing and divisions differs-from-docs; minor spacing matches | `grid` command: "Sets the size of the grid in linear units. The default is 12 units." and "Sets the spacing between major grid lines in linear units. The default is 10 units." and "Sets the number of subdivisions between major grid lines. The default is 10." Ours is 5 divisions per major line against the docs 10; the minor spacing is 1 unit in both (docs 10 / 10, ours 5 / 5). Maya 2025.1 `mayapy` `grid -q` reported size 12 and major lines every 5, which is the only build-backed value here. | `grid_follows_mayas_default_size_major_lines_and_divisions` (the test name says more than this row supports) |
| 29 | Shading ambient floor | 0.4 of the base colour | `src/shading.rs` `AMBIENT_FLOOR` | provisional | Not documented. | `back_facing_and_grazing_fall_to_the_ambient_floor`, `unusable_normals_or_views_give_the_ambient_floor_not_nan` |
| 30 | Move arrowhead | 12 px long, 4 px half width | `src/move_handles.rs` `ARROWHEAD_LENGTH`, `ARROWHEAD_HALF_WIDTH` | provisional | Not documented. | `arrowhead_dimensions_are_pinned_to_their_provisional_pixel_values` |
| 31 | Move centre exclusion | 10 px around the origin is not an axis pick | `src/move_handles.rs` `CENTER_EXCLUSION` | provisional | Not documented. | `front_axes_share_exact_physical_drawing_and_hit_geometry` |
| 32 | Selection cap | 4096 selected objects | `src/selection.rs` via `MAX_TRANSFORM_PATHS` | provisional | Not documented; reuses the transform-path cap. | `selection_cap_is_the_transform_path_cap`, `add_past_the_cap_is_an_error_and_does_not_mutate` |
| 33 | Selection colours | member tint (107, 137, 157) and edge (242, 163, 70); lead tint (117, 167, 117) and edge (140, 240, 140) while more than one object is selected; a lone selection uses the member colours | `src/viewport_renderer.rs` `SELECTION_TINT_RGB`, `SELECTED_EDGE_RGB_OBJECT`, `LEAD_TINT_RGB`, `LEAD_EDGE_RGB` | provisional | Maya's "Select objects or components" page says the last selected object is drawn in a different colour; it gives no colour values, and what a lone selection looks like is not stated. Ours are original values. | `selection_colours_are_pinned`, `members_are_tinted_the_lead_differs_and_unselected_is_untouched`, `single_selection_pixels_are_unchanged` |

Tally: 33 rows. Verified-by-docs parts: the manipulator size range (17), the Undo
default of 50 (21) and the Pick Range value of 8 px (14, its radius and unit interpretation stay provisional). Row 25 matches the Create Camera values for focal length, aperture, near and far; its Fill rule is provisional. Differs-from-docs parts: the
fixed undo queue (21) and the grid major spacing and divisions (28). Everything else is provisional.
Changing any value requires updating its pinning test and this row in the same commit.

[maya-manip]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Customizing/files/GUID-8740471F-BD72-4BDC-87B4-4DE870AD51E5.htm
[maya-resize]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-09DEFD6A-E8F9-4606-9D81-1CD45E01B59A.htm
[maya-undo]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Customizing/files/GUID-DD494155-AC94-4C48-8A36-58267FECBB0A.htm
[maya-sel]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Customizing/files/GUID-8C8BA1F4-59EE-4A02-BFE7-9193C0D6F5BE.htm
[maya-poly]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-B7F70ACE-0F3F-483B-83B5-D9711D6CBAAC.htm
[maya-cam]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-35B872B1-840E-4DEE-B80F-F2715B1E8BF0.htm
[maya-tumble]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-A3305185-DFBD-42BF-8A70-59208C2C9887.htm
[maya-grid]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/grid.html

## Native documents

`document::load_native` stages Maya binary `.mb`, supported static cubic Maya
ASCII `.ma`, or legacy `.aztec` data from one
bounded regular-file handle, retaining the exact parsed Arc and opened-file
baseline. Unix reads use nonblocking open followed by a regular-handle check so
special files cannot wait for a writer. No equivalent Windows deadline is claimed.
The 16 MiB input limit is application policy. Legacy validation uses these same
bytes rather than rereading the path. The local
Unix and Windows binary writer preserves supported existing-record edits
(Windows differences are listed in known-issues.md); document-layer
repeat-save checks do not establish the pending native Save journey.

Open validates/prepares an off-document candidate and frames its visible geometry
before replacement or an unsaved confirmation. Discard applies the exact staged
bytes, even if the path changed; a modal resize reframes that candidate using the
current aspect before consuming it. Failed decode/view/framing preserves the old
document. New/Open/Close use explicit eligible Save/Don't Save/Cancel (MCP decision
`discard` for Don't Save) with a pending-action UUID; MCP also requires the current scene token. A successful Save
acknowledges its written snapshot before revalidating queued Open. A source
detected as changed during Save blocks that pending Open until Cancel/reopen.
Uncertain installations retain a
receipt and keep the document dirty; reconciliation verifies the original write
without reinstalling, while later edits remain dirty against that saved snapshot.
UI and MCP use the same handlers; [the MCP contract](mcp.md) documents the response
delivery barrier for remote Close. Full native file-dialog navigation, native
editing of imported scenes and other-platform interaction need separate evidence.

### Read-only static cubic Maya ASCII

ASCII Open uses a bounded inert command decoder, never a MEL evaluator. The
admitted class is clamped open cubic, nonrational three-dimensional curves with
finite f64 CVs/knots, strict positive interior spans (repeated interior knots,
i.e. C0 curves, are refused as Unsupported), supported parent transforms and the
explicit startup/metadata envelope. The retained legacy 2016 reference
uses centimeter/degree/film units. Unsupported classes, commands, drivers,
history, units or unknown effects refuse the whole candidate. This is not general
Maya ASCII, rig or Maya 2026 compatibility. Original bytes, including quoted
script bodies, remain inert and unchanged.

NativeScene owns the canonical curve identity, parent edges and f64 source
values. ViewScene evaluates supported spans and prepares derived world-space
segments; the CV hull is used only as a conservative framing bound. Its chord
tolerance is application policy, max(1e-6cm, world CV-hull diagonal*1e-4) plus an
f32 allowance min(max|world coord|*f32::EPSILON, 0.01*diagonal), not a Maya
tessellation constant. Error checks include actual emitted f32 endpoints. A curve
whose f32 viewport grid is coarser than about 1% of its size (the ~3.6cm
specimen: drawn at x=2e6cm, refused at 3e6cm) is refused and Open fails, never
drawn as noise. Unrepresentable or over-tolerance conversion refuses instead of
relaxing the bound. Candidate-validation refusals that name an unsupported view
fact (today only repeated interior knots are reachable from ASCII) report
Unsupported rather than Malformed; structural errors stay Malformed.
Each curve instance is capped at 65,536 segments and depth 24, with one million
metered preparation visits shared across the whole derived view and the existing
derived byte budget. Parsing separately caps input at 16 MiB, tokens/statements at one
million each, accounted buffers at 128 MiB and metered work at 50 million. These are
operational admission policies, not heap or timing guarantees.

ASCII documents are read-only. The core authoring refusal also drives disabled
native name/channel/Create/Delete/Save/Save As controls and their visible reason;
focused disabled TextEdits suppress actual text interaction. Selection through
the Outliner or `select_node`, navigation and `frame_selected` stay available.
Viewport curve picking, CV/component editing, animated/history-driven evaluation
and native ASCII writing are unsupported. Binary NCRV remains refused independently of this
ASCII path. MCP exposes bounded canonical curve metadata and source counts,
without CV arrays, script text or file paths; see [the contract](mcp.md).

Scoped local checks exercised independent uniform/nonuniform spline oracles,
actual f32 output bounds, shared resource limits, native document/lifecycle
admission, disabled UI text events, Outliner selection, official SDK
Open/inspect/select/frame and an actual Editor GPU callback. The GPU callback
check uses a 128px framebuffer; later 640×480 and 1920×1080 UI layouts check
submission payloads only. An earlier curve prototype passed default, headless
and MCP-client suites plus strict checks. Independent retained-evidence review
and an isolated closed-segment-clamp mutation established baseline PASS,
intended FAIL and restored PASS; the probe used an archive-verified private copy
of the same dependency version because the local registry source was incomplete.

A scoped macOS diagnostic preview opened the genuine reference, displayed its
curve and grid, selected the curve through the Outliner with selected coloring,
and showed disabled authoring controls and the read-only reason. Frame Selected
was clicked with the view already fitted; no camera change is claimed. This used
a private file-panel hook. Native text-commit refusal was not exercised; the
focused automated text-event regression covers that protection. These observations
do not establish installed-tool behavior, other platforms or Maya parity.

The Save/material-parent union separately passed focused document, UI, bounded
metadata, actual SDK and Editor GPU checks, strict client checks and normal
hooks. It preserves binary Save acknowledgements and revalidates queued ASCII
sources on Save, while plain Discard retains staged-byte behavior. ASCII Save
and Save As refuse through actual SDK calls without writing a destination.
Composition with later feature parents, its final hooks/CI and native preview
remain separate acceptance gates.

The explicitly invoked genuine consumer is the unmodified 82,189-byte
[Autodesk maya-usd legacy 2016 curve scene](https://github.com/Autodesk/maya-usd/blob/381650c6bcd695cebae15e2cd73f5f2a7125abb2/test/lib/usd/translators/UsdExportNurbsCurveTest/UsdExportNurbsCurveTest.ma),
Git blob `9faa26c2a5e30f3250f86d76b2ca89a601b1f089`,
SHA256 `943735c17b5bac97d75c3326a274702eb5e84cd5de71afeb4f9a70df43d541cc`.
It is retained outside the repository. Set `AZTECCRAFT_MA_CURVE_FIXTURE` to the
explicit retained file after a separate SHA256 precheck. Decoder source-word
checks additionally require an independent private
`AZTECCRAFT_MA_CURVE_AUDIT`; they do not treat production parsing as the oracle.
No reference scene, embedded script or private manifest is distributed.

```sh
cargo test --locked --no-default-features --lib document::native_open_tests::genuine_ascii_native_open_and_frame_keeps_retained_source -- --ignored --exact
cargo test --locked --features mcp-test-client --bin azteccraft app::mcp_editor_tests::official_client::ascii::genuine_ascii_sdk_open_inspect_select_frame_uses_actual_editor -- --ignored --exact --test-threads=1
cargo test --locked --features mcp-test-client --bin azteccraft app::gpu_viewport_tests::headless_gpu_ascii_editor_open_select_frame_reaches_actual_curve_callback -- --ignored --exact --test-threads=1
```

The first two commands consume the real file; the last uses an original small
synthetic curve to exercise the actual Editor framebuffer. None executes source
scripts. The native observation above belongs to the earlier curve prototype;
it does not verify every later combined source parent.

The standalone legacy `document::load`/`save_new` utilities retain validated JSON
v1 quad scenes and create-new/no-clobber writes via vendored `craft-atomic`.
They are tested independently and aren't used to save native editor state.
The local candidate's guarded active-file binary Save is provisional. General
recovery, autosave, history and camera persistence remain pending.

## OBJ export

`obj::export_new` writes world-space vertices and quad faces, with per-object
index offsets and winding reversed for reflected transforms. Object names use
UTF-8 percent encoding to avoid whitespace/directive injection. Export is staged
to a new destination through `craft-atomic` and preserves occupied paths. It
exports geometry only, without UVs, normals or materials. File > Export All and
Export Selection and MCP `export_obj` use the native-scene writer `obj::export_native_new`.
OBJ import isn't implemented.

## Camera core

`src/viewport.rs` implements orbit/pan/dolly, perspective projection, rays,
triangle-based nearest-object picking and framing. It is exported by the library
and used by the native viewport UI.
Coordinates are right-handed/Y-up. Normalized device coordinates use X/Y in
[-1, 1], Y up and depth in [0, 1], with pixel mapping in `src/viewport_ui.rs`.
Tests exercise projection/ray agreement, transforms, clipping, framing, nearest
hits and invalid inputs. The native viewport submits homogeneous clip positions
through eframe's wgpu device, with Depth32Float depth testing and hardware
clipping. It paints opaque flat geometry and source boundary edges, with no CPU
fallback. Appearance, document color management and comparison with Maya remain
pending. The persistent HUD and MCP inspection report backend and appearance
limitations. Device-free test Editors remain explicitly uninitialized.

Run actual framebuffer proofs explicitly with `cargo test --locked --bin
azteccraft --features mcp-test-client headless_gpu -- --ignored --test-threads=1`.
These require an adapter and fail when it is absent. CI additionally runs this
suite with Mesa software Vulkan. Ordinary numeric submission tests do not
establish rasterization or platform/native-window fidelity.

For the Grid slice, `cargo test --locked --bin azteccraft --features
mcp-test-client grid_` selects editor, schema, SDK and submission tests. Run its
actual empty-frame and visibility proof explicitly with `cargo test --locked
--bin azteccraft --features mcp-test-client
viewport_renderer::tests::headless_gpu_grid_callback_clears_cold_and_warm_empty_frames_and_restores_pixels
-- --ignored --exact --test-threads=1`. The SDK camera/selection/grid callback
suite remains part of the explicit `headless_gpu` command above. Native clicks,
window resize, file retention and the eventual Face integration need their own
exact-build evidence; a passing adapter test does not supply it.

An empty mesh is unpickable; framing it returns an error and preserves the camera.
Unused vertices are retained by native documents and OBJ export. Structural
validation preserves degenerate geometry; those states don't establish drawable
surface or normal fidelity.

`glam` supplies vector/quaternion math, and `serde`/`serde_json` implement the
document layer. Direct versions and the resolved dependency graph are pinned.
`craft-atomic` is the single path dependency at `vendor/craft-atomic` with recorded
[source, patch and update provenance](../vendor/craft-atomic/UPSTREAM.md).
Debug symbols and incremental builds are disabled
in development/test profiles to limit local disk use.

The standalone vendor test lock uses only registry version/source/checksum tuples
already in the root lock. Root consumer tests do not run dependency tests; the
existing three-platform CI matrix runs both. Packaging captures all four complete
vendor license/notice files from the selected manifest's project root before
creating output, with a 64 KiB per-file limit and a 256 KiB total bound. Inputs
must be stable regular files during capture. Captured bytes accompany each binary
and its archive under `Contents/Resources/licenses/` on macOS or
`azteccraft/licenses/` on Linux/Windows: the root `LICENSE-MIT`,
`LICENSE-APACHE` and `NOTICE`, then `craft-atomic/` (the vendor files) and
`egui-default-fonts/` (the font license files copied verbatim from
`epaint_default_fonts` 0.36.2). Rejected notice inputs leave no new output
directory.
This prerequisite does not implement native Save.

## Maya Binary container inspection

`maya_binary::container::inspect` inspects an immutable byte buffer and retains
the original bytes, header bits and padding with checked chunk spans. It admits
41 explicitly observed FOR8/LIS8 group layouts, rejects unseen group layouts and
malformed nested extents, and keeps unknown leaf payloads inert. Its 16 MiB,
100,000-chunk and 64-level limits are application resource policy, not Maya
format limits. Container inspection alone doesn't establish scene semantics or
native Open, display or Save compatibility.

The observed FOR8/NCRV layout rounds child payload sizes to eight bytes and
retains NRBC payloads opaquely, including header bits and nonzero padding.
Native decoding and Open explicitly refuse every NCRV group, including empty
groups, until curve semantics are supported. Two retained vendor streams advance
from NCRV refusal to the next unsupported JOIN form; neither scene is admitted.
After a separate regular-file/size/SHA256/Git precheck, the ignored
`private_ncrv_vendor_inputs_reach_the_next_unadmitted_join` container test requires
explicit `AZTECCRAFT_MB_NCRV_ANIM` and `AZTECCRAFT_MB_NCRV_LAYOUT` input paths.

The layout observations were compared with
[MayaSceneKit's pinned raw parser](https://github.com/yamahigashi/MayaSceneKit/blob/95a11db062689285604b3a647385dc9388402865/crates/maya-scene-kit-formats/src/mb/parser.rs)
and independently audited private streams. Its parser engine and generated MEL
metadata are not included. Pinned `nom` 8.0.0 supplies complete big-endian integer
primitives under its MIT license, copyright 2014-2019 Geoffroy Couprie.

Public tests use independently authored synthetic containers. The ignored
`private_fixture_manifest_partitions_all_bytes` integration test requires an
explicit external manifest through `AZTECCRAFT_MB_FIXTURE_MANIFEST`; invoke it
with `cargo test --test maya_binary_container --no-default-features --locked
private_fixture_manifest_partitions_all_bytes -- --ignored --exact`. It checks
bounded file reads, byte sizes, chunk counts and complete partitions. Fixture
SHA256 verification is a separate recorded precheck, not performed by that test.
Private assets and manifests are not distributed or silently counted as tested.

The crate-private semantic parser reads CREA, DBL3/DBLE, SLCT, CWFL, RELA and
MESH records into immutable ranges over that same source. It checks counted
blocks, the observed corner grammar, UV references, trailers and enclosing
creation or selection ownership. Its metadata allocations have a checked
32 MiB application limit. Endpoint flags, identity bytes and normal words stay
unaltered; their unresolved meanings are not assigned authoring properties.
Production parsing and validation are unconditional; descriptive spans used only
by the independent trace proof are stored only in test builds.

`native_scene::binary::decode` constructs a checked `NativeScene` off-document
using that same parser and immutable source Arc. It binds effective last
transform assignments to their original numeric words, retains earlier source
atoms, checks static geometry dependencies and preserves base f32 points and
sparse f32 offsets separately. Effective points use f64 addition. Unknown edge
smoothing and authored normal assignments remain unresolved; appearance is
incomplete. Custom definitions are retained inert and admitted only with the
supported framing, owner/type matching and checked appearance connections.

Binary candidates also retain immutable checked appearance associations over
the same source bytes. Supported bindings preserve full face coverage, GPID and
shading-group links, unique instance ownership, explicit and packed UV words,
source-corner UV indices and the Phong/file/place2d input graph. Message and
partition associations are retained without treating them as value drivers.
Saved inputs and documented compound defaults have distinct provenance.
Unsupported appearance leaves geometry available with fixed diagnostic issues;
malformed recognized framing and resource exhaustion refuse the candidate.
The additional metadata and temporary graph tables have an 8 MiB policy inside
the existing decoder budget and one million checked association-work steps.
Native Channel Box and MCP inspection read bounded summaries without reparsing
or texture IO. Numeric edits, History snapshots and encoder revalidation retain
these associations; no material, sampler or color processor is implemented.

The decoder applies a 128 MiB accounted-buffer policy and a checked 50 million
validation-work-unit policy before canonical construction. These are operational
limits, not a process heap guarantee, timing promise or Maya format limits.
Errors expose only a fixed category and an optional source byte offset. The
decoder performs no IO or live document replacement. The document/editor adapter
uses this decoder for native and MCP Open into the same NativeScene/History;
supported source-bound numeric edits preserve original bytes plus typed intent.
Structural imported edits remain unsupported. The provisional binary writer
supports proved existing-record edits and the insertion of an absent translate
leaf at a recorded site (`binary-leaf-insertion.md`), with native Save QA
pending. The ignored differential
`native_scene::binary::encode::tests::private_differential` strips one `t`
record at a time from the private corpus, reinserts it and requires byte
identity; run it with `cargo test --lib private_differential -- --ignored
--nocapture` and `AZTECCRAFT_MB_FIXTURE_MANIFEST` set. A codec success alone doesn't
prove appearance or Maya parity.

The ignored unit test
`maya_binary::semantic::tests::private_wire_fixture_spans_and_mesh_trace_match_audit`
requires both `AZTECCRAFT_MB_FIXTURE_MANIFEST` and `AZTECCRAFT_MB_WIRE_RANGES`.
Run it explicitly with `cargo test --lib --no-default-features --locked
maya_binary::semantic::tests::private_wire_fixture_spans_and_mesh_trace_match_audit
-- --ignored --exact` after a separate SHA256 precheck. It compares source spans
with an independent external trace and checks mesh loops in the fixtures.

After a separate SHA256 precheck, the explicitly ignored production consumer
test requires `AZTECCRAFT_MB_FIXTURE_MANIFEST`, `AZTECCRAFT_MB_DISPLAY_RANGES`
and `AZTECCRAFT_MB_WIRE_RANGES`, and runs with `cargo test --lib
--no-default-features --locked
native_scene::binary::tests::private_native_candidates_and_views_match_independent_counts
-- --ignored --exact`. It checks canonical scene and prepared `ViewScene`
geometry/visibility counts and source Arc identity. Independent external ranges
also check parent bindings, units, scalar origins and exact internal words,
retained f32 base-point bits and exact f64 sparse-offset sums.
Its library success does not constitute a native Open or appearance proof.

## World-axis Move source candidate

The native toolbox now has Select/Move vector buttons and unmodified Q/W.
World-only X/Y/Z drags use the same exact selected instance and canonical History
transaction as the generic manipulation MCP tools. Preview changes are authoritative,
Channel Box fields remain synchronized and locked during the transaction, and a
release commits at most one Undo entry. Escape and capture/lifecycle changes
restore the exact begin authoring intent. Physical viewport origin/size, camera
projection and fractional DPI mapping are captured and compared before update or
release; renderer submission uses the same captured physical aspect.

Only object translation is implemented. Plane/center handles, components,
rotation/scale manipulation, snapping and alternate orientations remain
unsupported. Missing source translation leaves, selected pivots, driven data and
unstable or unrepresentable transforms refuse editing. Pixel sizes, ray thresholds
and last-valid release behavior are provisional application policies (rows 14 to 16
and 22 to 23 of the [policy constants table](#application-policy-constants)).

Source-only tests were authored for real Q/W/buttons and egui pointer paths,
absolute XYZ and outside-viewport motion, one Undo, stationary release under
window/DPI changes, cancellation/source-word preservation, strict SDK parameters
and actual SDK preview/commit/cancel/Save/reopen. Runtime, native framebuffer and
desktop verification are pending on this candidate; no full Maya parity or
current-parent publication is claimed by this paragraph.

## Rotate and Scale tools (native UI)

E and R select the Rotate and Scale tools, and their Tool Box icons are live. While
either is active, a row above the viewport holds the Object/World selector, an axis
choice and a numeric field with Apply (Enter also applies). Gimbal, Parent, Normal,
Custom and Component are listed disabled with the hover text "unsupported". The row
stands in for the unsupported Tool Settings panel.

A ring or box drag begins `BeginRotate` or `BeginScale` and previews through
`UpdateManipulation` (degrees or a unitless factor), then commits through `FinishMove`.
They go through the same guards as Move (draft commit, modal, close, scene token, owner,
`BusyManipulation`), and one drag is one history entry named Rotate or Scale; the numeric
field uses the same three operations. The Channel Box follows the preview. Hovering a
handle writes its state, or the core's refusal, to the status line; pressing a refused or
unsupported handle shows the same text and consumes the click so it cannot select. A
zero or negative Scale factor reached by a drag is refused by the core, shown in the
status and keeps the last valid preview. Undo and Redo report the entry they act on
("Undo Rotate"). The generic `begin_manipulation`, `update_manipulation` and
`finish_manipulation` MCP tools drive the same operations; see `mcp.md`.

## Independent Maya ASCII geometry roster

The test-only roster oracle validates geometry declaration names, ownership and
completion after framing the entire input with the inherited 16 MiB byte limit.
It returns observed shape order, declared geometry names, the actual byte seam
and moved raw tail kinds/tokens. Tail material and connection semantics remain
unvalidated. Token transport comparisons use the separate handwritten tail fixture.
The additional owned vectors do not establish a process heap limit.

Run `cargo test --locked --no-default-features --test maya_ascii_roster` for its
standalone caller. Exact array values/order, topology and material graph validity
require later oracles; this scope does not enable Save, native/MCP operations or
establish Maya acceptance. Later writer proofs must also check deep geometry on
the exact pre-tail byte segment with the unchanged strict prefix decoder.

## Independent Maya ASCII material declarations

The test-only declaration oracle consumes the complete geometry roster once,
validates supported Lambert/color/shadingEngine pairs and observed name
uniqueness, and preserves finite unquoted f64 color text in the application
range [0, 1]. It returns the inherited actual byte seam and shape order plus
moved raw connection vectors. Connections remain unvalidated graph data.

Run `cargo test --locked --no-default-features --test maya_ascii_material_declarations`
for its standalone caller. This does not enable Save or native/MCP operations,
validate deep geometry or material graphs, or establish Maya acceptance.
Reserved/default-name eligibility and Maya float3 storage remain unproved.
