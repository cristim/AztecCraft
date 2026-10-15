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
`ViewScene` derives bounded read-only triangle/quad instances, preserving original
face indices and full instance paths. Unsupported evaluation or topology refuses
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
Hierarchy editing remains a subsequent task.

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

F11 or Select > Components > Face enters Face mode; F8 or Object/Component
toggles Object/Face. The active transform contributes only directly attached
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
center; planar depth equality is admitted. A nonplanar face can hide its own
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
remains on its established route. Native Delete cannot delete the whole object
under Face mode; explicit MCP delete_node retains its object-target meaning.
Native Extrude stays disabled pending real history/manipulator behavior.

Focused actual framebuffer proofs can be selected with `cargo test --locked
--bin azteccraft headless_gpu_face_ -- --ignored --test-threads=1`. CPU event,
protocol and framebuffer tests establish their exercised callers, not an OS
window journey or Maya parity. The attempted genuine combined menu
activation/key/viewport-click sequence was blocked by pinned egui's popup
routing. The retained proofs cover menu+F8 and F11+F8+viewport click separately, plus blocked-popup
nonmutation. Full coalesced-event support and platform/reference comparison
remain follow-up work.

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

`document::load_native` stages Maya binary `.mb` or legacy `.aztec` data from one
bounded regular-file handle, retaining the exact parsed Arc and opened-file
baseline. Unix reads use nonblocking open followed by a regular-handle check so
special files cannot wait for a writer. No equivalent Windows deadline is claimed.
The 16 MiB input limit is application policy. Legacy validation uses these same
bytes rather than rereading the path. Maya ASCII remains unsupported. The local
Unix binary writer preserves supported existing-record edits; document-layer
repeat-save checks do not establish the pending native Save journey.

Open validates/prepares an off-document candidate and frames its visible geometry
before replacement or an unsaved confirmation. Discard applies the exact staged
bytes, even if the path changed; a modal resize reframes that candidate using the
current aspect before consuming it. Failed decode/view/framing preserves the old
document. New/Open/Close use explicit eligible Save/Discard/Cancel with a
pending-action UUID; MCP also requires the current scene token. A successful Save
acknowledges its written snapshot before revalidating queued Open. A changed source
blocks that pending Open until Cancel/reopen. Uncertain installations retain a
receipt and keep the document dirty; reconciliation verifies the original write
without reinstalling, while later edits remain dirty against that saved snapshot.
UI and MCP use the same handlers; [the MCP contract](mcp.md) documents the response
delivery barrier for remote Close. Full native file-dialog navigation, native
editing of imported scenes and other-platform interaction need separate evidence.

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
and its archive at `Contents/Resources/licenses/craft-atomic/` on macOS or
`azteccraft/licenses/craft-atomic/` on Linux/Windows. Rejected notice inputs leave
no new output directory. This prerequisite does not implement native Save.

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
