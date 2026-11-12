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
run only on Unix, matching the native writer's supported platforms.
The independent viewer source-word oracle passed; three isolated core mutations
each failed at their intended assertions and passed after exact restoration.
Those private-resolver sensitivity runs are separate from the ordinary matrix.
The corrected SDK test and independent verifier also exercised actual protocol
Open of saved outputs and the new document token/origin. Root's bounded macOS
native edit/Undo/Redo/Save As/Open journey passed, with independent byte checks
limited to the authorized eight-byte word and unchanged original fixtures.
Committed cold, ordinary hooks, CI and published combined-parent verification
remain pending. Data editing does not imply evaluated Phong output,
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
middle drag pans, right drag/wheel dollies, and F frames the active object or face when
a text field isn't consuming keys. Ctrl/Cmd-Z undoes and Ctrl/Cmd-Shift-Z redoes.
New/Open use the shared native document lifecycle. This candidate enables
eligible Unix Maya binary Save/Save As through that same lifecycle and ordinary
vendored writer; current integration and native Save QA remain pending. No derived JSON save is substituted.
GUI Export and unfinished modeling/animation/material controls are disabled.
Docking and full Maya interaction parity aren't implemented or verified.

The checkable Grid button controls this single viewport's fixed XZ ground grid.
It defaults on and retains its session value across New/Open, including canceled
file actions. It shares `SetGridVisible` with MCP `set_grid_visibility`; the
explicit boolean setter is idempotent and inspection returns `grid_visible`.
This is visual state outside NativeScene and authoring History. It has no default
shortcut, grid options, additional/global panels or settings persistence. Grid
spacing, extent, colors and Maya reference parity remain unverified.

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
topology. A mesh with only one face, including the primitive plane, returns an
explicit unsupported-all-faces error; its Maya default behavior needs a pinned
fixture. Undefined normals, invalid indices/distances and coordinate overflow
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

Each marker is a 4x4 physical-pixel box at the arithmetic mean of the ordered
face corners, accumulated in f64 before conversion to view space. The click box
is 8x8 physical pixels. Homogeneous clipping, viewport dimensions and scale use
the same snapped physical capture as rendering. A ray through each eligible
center, rather than the offset click, checks all rendered triangles, including
the face itself, within the rendered near/far range. A closer surface blocks the
center; planar depth equality is admitted. Center-ray occlusion compares linear
view depth with a 1e-4 relative tolerance, so occluders closer than that to the
center (z-fighting range) do not hide it. A nonplanar face can hide its own
center, while an unobstructed finite degenerate center can remain selectable.
The GPU markers use the existing depth attachment and comparison path.

Picking preflights eligible centers times all occluder triangles with checked
arithmetic and refuses work above 1,000,000 intersections. Nearest surviving
camera depth wins; exact ties use full instance path, transform/shape IDs and
original source face index. These fixed application policies have not been
verified against Maya's Center/CBS behavior or numerical boundaries.

Object mode suppresses component feedback but remembers a still-valid face on
the same object/path. Switching object/path or successfully replacing a document
clears it; New/Open retain the current mask. Edits and Undo/Redo revalidate the
target and clear dangling identities without restoring them on Redo. Selection
does not modify authoring history, source bytes or dirty state. F/frame_selected
in Face mode frames the selected face corners even if its center is occluded or
clipped; no face returns an error instead of framing the object. Existing native
valid-draft commitment and remote draft-refusal rules remain in force.

Shift/Ctrl selection, marquee, multiple faces, other component masks, preselection
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
(UNCONFIRMED for Maya). Shift/Ctrl clicks are refused with `ModifiedClick`; UV, Vertex
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
count, and no history limit or history persistence exists yet.

## Native documents

`document::load_native` stages Maya binary `.mb`, supported static cubic Maya
ASCII `.ma`, or legacy `.aztec` data from one
bounded regular-file handle, retaining the exact parsed Arc and opened-file
baseline. Unix reads use nonblocking open followed by a regular-handle check so
special files cannot wait for a writer. No equivalent Windows deadline is claimed.
The 16 MiB input limit is application policy. Legacy validation uses these same
bytes rather than rereading the path. The local
Unix binary writer preserves supported existing-record edits; document-layer
repeat-save checks do not establish the pending native Save journey.

Open validates/prepares an off-document candidate and frames its visible geometry
before replacement or an unsaved confirmation. Discard applies the exact staged
bytes, even if the path changed; a modal resize reframes that candidate using the
current aspect before consuming it. Failed decode/view/framing preserves the old
document. New/Open/Close use explicit eligible Save/Discard/Cancel with a
pending-action UUID; MCP also requires the current scene token. A successful Save
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
exports geometry only, without UVs, normals or materials. OBJ import and the GUI
Export action aren't implemented.

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
Missing-record insertion and structural imported edits remain unsupported. The
provisional binary writer supports only proved existing-record edits, with native
Save QA pending. A codec success alone doesn't
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
and last-valid release behavior are provisional application policies.

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
