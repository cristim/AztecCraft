# Development

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
middle drag pans, right drag/wheel dollies, and F frames the selected object when
a text field isn't consuming keys. Ctrl/Cmd-Z undoes and Ctrl/Cmd-Shift-Z redoes.
New/Open use the shared native document lifecycle. Save As is visibly disabled
until genuine native writing is available; no derived JSON save is substituted.
GUI Export and unfinished modeling/animation/material controls are disabled.
Docking and full Maya interaction parity aren't implemented or verified.

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
offset, UVs/normals, construction-history nodes and native face controls remain
pending. Tests establish this subset's geometry, not Maya parity.

`examples/extruded-cube.aztec` was generated through cube creation and two
History extrusion commands: front face 1 by +0.5, then its right side face 7 by
+0.5. Integration tests reconstruct the 16-vertex/14-face closed quad mesh and
verify undo/redo, save/reload, reflected OBJ cap/side geometry and picking outside
the original cube. File → Open provides a native inspection path; it does not
add native face-editing controls or prove Maya equivalence.

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
bytes rather than rereading the path. Genuine Maya writing remains pending in
this source slice.

Open validates/prepares an off-document candidate and frames its visible geometry
before replacement or an unsaved confirmation. Discard applies the exact staged
bytes, even if the path changed; a modal resize reframes that candidate using the
current aspect before consuming it. Failed decode/view/framing preserves the old
document. New/Open/Close use explicit Discard/Cancel with a pending-action UUID;
MCP also requires the current scene token. Save is unavailable and cannot discard.
UI and MCP use the same handlers; [the MCP contract](mcp.md) documents the response
delivery barrier for remote Close. Full native file-dialog navigation, native
editing of imported scenes and other-platform interaction need separate evidence.

### Read-only static cubic Maya ASCII

ASCII Open uses a bounded inert command decoder, never a MEL evaluator. The
admitted class is clamped open cubic, nonrational three-dimensional curves with
finite f64 CVs/knots, strict positive interior spans, supported parent transforms
and the explicit startup/metadata envelope. The retained legacy 2016 reference
uses centimeter/degree/film units. Unsupported classes, commands, drivers,
history, units or unknown effects refuse the whole candidate. This is not general
Maya ASCII, rig or Maya 2026 compatibility. Original bytes, including quoted
script bodies, remain inert and unchanged.

NativeScene owns the canonical curve identity, parent edges and f64 source
values. ViewScene evaluates supported spans and prepares derived world-space
segments; the CV hull is used only as a conservative framing bound. Its chord
tolerance is application policy, max(1e-6cm, world CV-hull diagonal*1e-4), not a
Maya tessellation constant. Error checks include actual emitted f32 endpoints.
Unrepresentable or over-tolerance conversion refuses instead of relaxing the
bound. Each curve instance is capped at 65,536 segments and depth 24, with one million
metered preparation visits shared across the whole derived view and the existing
derived byte budget. Parsing separately caps input at 16 MiB, tokens/statements at one
million each, accounted buffers at 128 MiB and metered work at 50 million. These are
operational admission policies, not heap or timing guarantees.

ASCII documents are read-only. The core authoring refusal also drives disabled
native name/channel/Create/Delete/Save As controls and their visible reason;
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
submission payloads only. These checks do not establish genuine-file native-window pixels, final
integrated-stack behavior, full matrix/cold verification or Maya parity.

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
cargo test --locked --features mcp-test-client --bin azteccraft app::mcp_editor_tests::official_client::genuine_ascii_sdk_open_inspect_select_frame_uses_actual_editor -- --ignored --exact --test-threads=1
cargo test --locked --features mcp-test-client --bin azteccraft app::gpu_viewport_tests::headless_gpu_ascii_editor_open_select_frame_reaches_actual_curve_callback -- --ignored --exact --test-threads=1
```

The first two commands consume the real file; the last uses an original small
synthetic curve to exercise the actual Editor framebuffer. None executes source
scripts. Genuine-file native Open/display QA and final source-union verification
remain separate acceptance gates.

The standalone legacy `document::load`/`save_new` utilities retain validated JSON
v1 quad scenes and create-new/no-clobber writes via vendored `craft-atomic`.
They are tested independently and aren't used to save native editor state.
Overwrite-save, recovery, autosave, history and camera persistence remain pending.

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
Missing-record insertion, structural imported edits and genuine Maya Save remain
unsupported. A decoder success alone doesn't prove appearance or Maya parity.

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
