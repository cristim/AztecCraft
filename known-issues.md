# Known limitations

## Material data editing is a bounded candidate

The native Common Material Attributes/Diffuse control and `set_material_diffuse`
tool edit only an existing checked Phong scalar whose complete shared-shader
consumer closure is supported. Missing/default records, connected overrides,
unsupported consumers and graph edits are unavailable. Values use native f64
words and one shared History value; the slider does not clamp untouched source
data. No rendered material, texture lookup, assignment or color processor is
provided. Isolated material widget/SDK and default/headless/client checks pass;
the corrected protocol reopen journey was independently reproduced and bounded
macOS native edit/Undo/Save/reopen passed. Committed cold verification, hooks/CI
and published Save/NCRV/Face/Grid composition remain pending.

## Attribute Editor first slice is an approximation

**Severity:** low
**Area:** editor UI
**Issue:** cristim/AztecCraft#60

The Attribute Editor models Maya's Ctrl+A switch as one right dock that cycles between the
Channel Box and the Attribute Editor (hidden to Attribute Editor); it is not Maya dock, tab or
floating-window parity. Its Transform Attributes list comes only from the Maya 2023 page and is
provisional. Tabs are the selected transform, its direct mesh shapes and their supported
materials, not Maya's connected-node tabs. Only the first 16 direct shapes of a transform get shape and material tabs; selecting a later one is refused as `invalid_input`, natively and over MCP, and the panel states how many shapes are omitted. Pin Tab, Notes, Extra Attributes, Add/Show/Hide
Attributes, rotate and scale pivots, limits, visibility editing and material assignment are
visibly unsupported, and Shear, Rotate Order, Rotate Axis and the pivot translations are shown
read-only with no edit control. Maya's default modifier is Ctrl on every platform; AztecCraft
uses an explicit Ctrl modifier rather than its `modifiers.command` convention, and the macOS
Cmd-versus-Ctrl behavior is unverified. Native visual verification is pending.

## Reference baseline unavailable

**Severity:** high
**Area:** parity verification

The exact Maya version/platform and reference installation have not been established. Identical behavior and visual parity cannot be verified yet. Build functionality without claiming parity; acquire a pinned reference before parity acceptance.

## Complete parity inventory not yet enumerated

**Severity:** high
**Area:** release gate

The original first-push restriction required 80–90% of all Maya 2026 functionality, including simulation, rendering, scripting, and plugin compatibility. The user's later explicit push/merge instruction authorizes early prototype publication, superseding that restriction. The full-functionality target remains unmet, and the complete inventory must still be enumerated and verified against a reference. Don't calculate a percentage from an arbitrary feature list.

[The initial inventory seed](.project-docs/inventory-seed.md) provides source-backed candidate workflows, but its 100 entries aren't a complete denominator. Component versions, cloud services, collection components and third-party plug-in scope still need to be pinned. Overall coverage remains unmeasured.

## Desktop and platform verification pending

**Severity:** high
**Area:** desktop application

The [macOS packaged-debug verification record](.project-docs/native-verification.md) now covers cube creation, numeric editing, Save As/Open, unsaved-change cancellation/discard, no-clobber protection and malformed-open state preservation at source commit `6c96345`. Native file choices were supplied through held panels using OCU `choose_file`; full interactive panel navigation remains pending. Windows/Linux native interaction and packaged launch checks, GUI OBJ export and Maya comparison also remain pending. Subsequent builds need their own verification before these results are generalized.

The native-model transition has separate provisional macOS evidence: five supplied
Maya binary scenes opened and displayed geometry, including orbit navigation.
An acute CPU tessellation defect was reproduced and corrected; material, depth
and clipping fidelity remain incomplete. Imported-scene GUI editing and the new
native lifecycle still require final independent native evidence. Earlier JSON
Save As verification does not establish native Maya writing. The current
Unix binary Save candidate uses the ordinary vendored atomic engine. Its
document-layer repeat-save checks are separate from current application checks
and pending native Save QA.

One macOS abort was observed during an automation-intercepted Open panel. The
initiating panic and trigger were not reproduced; the same binary subsequently
passed hooked and unhooked Open/Cancel. The source uses nested default-mode
pumping, but that observation does not establish the cause or a fix. Native
verification must distinguish this observation from a confirmed diagnosis.

## Core authoring limits

**Severity:** medium
**Area:** history, documents and viewport

Undo keeps the newest 50 NativeScene snapshots and silently drops older ones
(Maya's documented default Finite queue size, Preferences > Undo; the limit is
fixed, not configurable, pending #178; Maya 2017 Help is the newest page found
stating the default, so 2026/2027 are unconfirmed: PROVISIONAL). Redo is
unchanged. The dirty state compares the live scene with the saved scene by value,
so dropping old entries cannot make an edited scene read clean. Memory still
scales with scene size times 50. Autosave and persisting history remain pending.

Selection, camera, tool and panel changes do not create undo entries in
AztecCraft (`.project-docs/mcp.md` already says selection does not). Whether Maya
puts selection changes on its undo queue is unverified and deferred to #33
(reconcile provisional application policies with Maya defaults).

Measurement: on macOS (Darwin 25.1, arm64 release build),
`cargo test --release --test history_memory -- --ignored --nocapture` with 1000
cubes (8000 vertices) and 300 renames grew process RSS by 667,760 KiB on a072221
(unbounded) versus 125,712 KiB with the limit. Single run, this synthetic scene
and rename edit only; not a general claim. To reproduce the baseline, copy
`tests/history_memory.rs` onto main a0722217 and run the same command. The test
reads RSS with `ps -o rss=`, so it is Unix-only (`#[cfg(unix)]`).

The native model retains separate transform/shape identities, instance
hierarchy, f64 authoring channels and original source bytes. Triangle
and quad derived views are supported; arbitrary loops/holes remain retained but
unsupported for projection. Imported editing is limited to proved source-bound
numeric changes; missing-record insertion and structural rewrites refuse safely.
The provisional Unix binary Save/Save As candidate preserves proved existing
records and reconciles uncertain installations. It does not support Maya ASCII,
Windows writing or structural rewrites, and still needs native Save QA.
Recovery, autosave and camera/history persistence remain
pending. Legacy JSON v1 and OBJ remain separate library utilities; their geometry
tests don't establish native persistence. Structural
validation doesn't repair geometry or guarantee rendering fidelity.

The camera core is connected to a depth-tested opaque GPU geometry viewport with hardware clipping. Materials, transparency, normal shading, textures, document color management and reference navigation parity remain pending. Backend failures remain explicit with no CPU fallback. These limits are described in [development instructions](.project-docs/development.md).

Checked source appearance summaries now distinguish supported immutable source
associations from unavailable or unbound shapes in the Channel Box and MCP
inspection. They do not evaluate materials, resolve texture assets, prove
quadratic-filter equivalence or provide a color processor. The geometry preview
continues to report incomplete appearance; correct display and Maya comparison
remain acceptance work.

The extrusion API supports one quad with signed object-space normal displacement,
including zero-distance topology. All-face extrusion, including a one-quad plane,
is explicitly unsupported until a pinned Maya fixture establishes default behavior.
Region/world extrusion, divisions, offset/taper, UVs/normals, construction-history
nodes and native Extrude controls remain pending.

The synthetic extruded-cube example and core journey tests exercise geometry,
history, document persistence, OBJ export and camera picking. They don't replace
native face-tool interaction checks or comparison against a pinned Maya scene.

The Channel Box Visibility row and `inspect_scene` `nodes[].visible` show only a
transform's own `.v`: an attribute absent from the source reads as on (the decoders'
default, not yet compared with a pinned Maya scene), and inherited visibility,
shape `.v` and visibility editing are not reported or supported.

## Maya binary curve and remaining container coverage

**Severity:** high
**Area:** native documents

Public container inspection retains the observed FOR8/NCRV framing and opaque
curve records. Native decode/Open refuses NCRV, including empty groups, to avoid
silently omitting curve geometry. The next unsupported form in two retained
vendor streams is JOIN; other unseen layouts and curve semantics remain pending.
This framing coverage does not establish complete container parsing, scene Open,
NURBS rendering, native writing or Euler behavior.

## Convex n-gon display is best effort

ViewScene shows faces with five or more corners when they are hole-free, strictly
convex and wound once. Strongly skew (non-planar) n-gons may be accepted yet
overlap in projection or visit a corner twice (a skew loop such as `[0,1,2,0,3,4]` can
pass the convexity and winding checks; planar input cannot); display only, no safety
impact. Concave and holed faces
still refuse the whole view. Imported .mb convex n-gons now display, where before they
made the whole view refuse. Polygons with extreme coordinates (about |c| >= 1e154 or
<= 1e-200) are refused with NonFiniteWorld or UnsupportedPolygon, not displayed.

## Face Center selection is a provisional subset

**Severity:** medium
**Area:** component input and viewport

Face mode supports one exact canonical face target, visible depth-tested center
markers and plain primary-click replacement/clearing. The fixed 4x4 marker,
8x8 click box, corner-mean center, center-ray visibility and 1,000,000-intersection
budget are application policies, not verified Maya defaults. Nonplanar faces can
hide their own centers. Center visibility uses a 1e-4 relative linear
view-depth tolerance, so occluders within about 1e-4 of the center's view depth
(z-fighting range, e.g. stacked planes 1e-3 apart at distance 50) do not hide it;
unobstructed finite degenerate centers can be selected.
Other component masks, multiple-face gestures, marquee, hover and marking menus
are unsupported. The same holds in Vertex and Edge modes: a Shift or Ctrl click is refused
with ModifiedClick and leaves the selection unchanged. Maya supports modified clicks to add
to or toggle the component selection; AztecCraft selects one vertex, edge or face only. Native Extrude remains disabled until a real history and
manipulator workflow exists.

Ambiguous coalesced primary releases are refused with an actionable single-click
retry rather than guessed coordinates/modifiers. This limitation was reproduced
with an actual Shift click followed by an unmodified drag release in one frame.
Single-release event-time modifiers and pure Alt navigation are preserved. The
attempted combined menu activation/key/viewport-click sequence is blocked by the
pinned popup routing; separate menu+F8 and F11+F8+viewport-click tests do not prove
that combined journey. Native window, other-platform input and Maya comparison
remain separate verification requirements. See [selection policy](.project-docs/development.md#face-center-selection).

## Vertex and Edge picking and drawing constants are provisional

**Severity:** low
**Area:** component input and viewport

F9, F10, the Select > Components entries and a plain click reach `Camera::pick_vertex`
and `pick_edge` (#29; the Status Line Vertex, Edge and Face buttons dispatch the same modes). F8 leaves Vertex and Edge for
Object and enters Face from Object and Hierarchy; Maya's F8 return target is
UNCONFIRMED, so this is provisional. A click on empty space clears the active type's
pick, also UNCONFIRMED for Maya.
The Channel Box shows only the parent object's channels in Vertex, Edge and Face modes; there is
no component feedback (coordinates or attributes) for the picked vertex or edge.
The 10 px vertex click box, the 6 px edge click distance, the 5 px vertex marker, the
3 px selected-edge strip and the depth tolerance below are original provisional values,
not verified Maya defaults; Maya's click tolerance and preselection defaults are
UNCONFIRMED for 2026. Colours follow only the Maya 2026 colour words (purple-ish
vertex, highlighted when selected, light-blue edge, light-orange selected edge) with
original RGB values. Candidates are ranked by pointer distance, then depth, then
source order, which is also an application policy. In Vertex and Edge modes component
feedback replaces Object feedback, so the selected object's faces are not tinted and
selected curves are not highlighted.

Test limits for this slice: the `sync_fields` owner-path test fakes `selected_path`
because no real instanced scene can be built from the bin crate, so it proves the
clearing rule, not a live instance. The `remembered_face` term in the MCP test
`baseline()` (`src/mcp_tests/official_client/vertex_edge.rs`) is defensive; no SDK test
can currently fail on it, so it is not claimed as tested.

The vertex and edge depth tolerance is the flat constant `VERTEX_MARKER_DEPTH_TOLERANCE`
= 6e-3. Its unit is a relative fraction of the linear view depth, the distance along the
view axis from the eye: marker bias moves a point toward the eye by that fraction of its
eye distance, and occlusion treats a hit as blocking only when its linear view depth is
below `depth * (1 - 6e-3)`. It is neither normalized device depth nor world units. Marker
bias, selected-edge strip bias, Wireframe marker filtering and pick occlusion share it, so
draw and pick agree; the face centre keeps 1e-4. Readbacks (Apple M1 Pro, Metal, side-2
cube at the origin, near 0.01, far 10000, yaw 0.6, pitch 0.4, 1024 px square, 45 degree
fov): every corner with a front-facing face showed all 25 marker pixels and no hidden
corner drew at the default distance 8 and at 120 for tolerances 6e-3 through 8e-3; 5e-3
left one silhouette corner at 20/25, and 2e-2 let a hidden corner show at 120, so the
margin is thin. The 5e-3 and 2e-2 bracket comes from manual readbacks with the constant
temporarily edited; no committed test reproduces it (the ignored readbacks pin only the
6e-3 outcome). Those numbers come from the Apple M1 Pro Metal adapter only. CI's
software Vulkan job runs the GPU tests, but it was not used to choose or bracket this
value; other adapters were not run. Limits: the tolerance is relative to distance, so
plates thinner than about 0.05 units at the default distance 8 (6e-3 times 8) can show,
or let a click pick, the vertex behind them; objects only a few px across can have a
hidden corner picked; and at distances beyond about 500 a flat 6e-3 under-covers the
Depth32Float resolution (no readback needed more at d=120 or d=300 with small-object
runs, but the buffer's relative resolution grows with distance). A pixel-derived bias is
the planned follow-up (#135).

Wireframe hides occluded vertex markers like face centres, but draws the selected edge
and every edge through surfaces. When that check is over budget `vertex_markers_hidden`
(Vertex mode) or `face_centres_hidden` (Face mode) is true. The renderer draws every
`ViewInstance` edge; the bin crate has no loose-edge fixture, so the face-less case is
covered by the carried edge list and the picker tests, not pixels.

## Selection changes are refused under a file modal

**Severity:** low
**Area:** component input and guards

Selection changes (Outliner row, F8 to F11, Status Line buttons, picks, MCP) are refused with
`BusyEditing` while a file modal is open (#141), including `set_selection_mode` to the current
mode. The refusal text is caller-neutral ("The editor is busy with a text edit, dialog,
close or pending input; try again when it finishes"); the
`busy_editing` MCP code is unchanged. The native Status Line check
covered MCP-driven mode switches on one macOS debug build only
(`.project-docs/native-verification.md`); Object mode and button clicks were not captured.

## Static ASCII curves have bounded read-only coverage

**Severity:** high
**Area:** native documents and viewport

The supported legacy 2016 static cubic ASCII class now has inert native Open,
canonical f64 curve state, evaluated GPU line geometry, conservative framing and
bounded MCP metadata. Source/SDK/UI/renderer tests cover this class; they do not
admit general Maya ASCII, arbitrary units/history/drivers, rigs, imported
instancing or Maya 2026 behavior. Unknown effects refuse the complete candidate.
Scene scripts remain retained inert data.

ASCII authoring and writing, viewport curve picking and CV/component editing are
unsupported. Use Outliner/MCP object selection, navigation and framing for
inspection. Binary NCRV is still refused. An earlier curve prototype has scoped
matrix, independent review/mutation and diagnostic macOS native Open/display
evidence; the Save/material-parent union has its own focused SDK/UI/GPU and
normal-hook checks. Later selected-parent composition, its CI and native preview
remain separate gates. Native text-commit refusal was not exercised, and no exact
visual or functional parity is claimed.
Small curves far from the origin are refused, not drawn, and Open fails, when
the f32 viewport grid at their coordinates is coarser than about 1% of the curve
size (a ~3.6cm curve is drawn at x=2e6cm and refused at x=3e6cm). Remedy
pending: a camera-relative or f64 viewport path.
[Development notes](.project-docs/development.md#read-only-static-cubic-maya-ascii)
record the supported class, limits and explicitly invoked genuine consumer.

## Existing Maya plug-in compatibility unverified

**Severity:** high
**Area:** scripting and extensions

The Maya scripting/API surface and existing plug-ins aren't implemented or tested in the current slice. Establish architecture/API/ABI boundaries and run existing script and plug-in fixtures against both applications. AztecCraft-specific extensions alone can't prove Maya compatibility.

## First-click text input can be lost

**Severity:** medium
**Area:** native text fields

With pinned egui 0.36.2, a previously unfocused name field drops a Text event delivered in the same frame as the initial pointer click. Its TextEdit builder processes text only when the field already has focus (builder.rs:572), before pointer handling requests focus (builder.rs:815). The same real Editor event sequence reproduces this without MCP. Text delivered in the following frame is inserted.

The MCP regression compares native and bridge-connected Editors with identical events and proves equal drafts, focus, scene and history, plus BusyEditing rejection of queued remote changes. It proves no additional input loss from MCP; it does not fix the native first-click defect. Native input handling and Maya reference comparison remain outstanding.

## Native MCP idle responsiveness remains unverified

**Severity:** medium
**Area:** native protocol lifecycle

Native testing of the initial nine-tool server reproduced repeated reply deadlines while the window was backgrounded, including a deterministic minimized-window inspection failure. Desktop interaction resumed dispatch. Pinned eframe uses `App::logic` without `App::ui` for hidden windows; the original bridge drained only in UI rendering.

The revised hidden logic path and retained-input guard pass actual trait-callback tests, and [scoped macOS native verification](.project-docs/mcp-native-verification.md) confirmed minimized requests at source `36dffdf`. In the combined icon/MCP build `541c72c`, an idle request reached its UI reply deadline. A main-thread sample taken during investigation included Metal drawable acquisition, without establishing overlap with the timed-out request or a five-second acquisition wait; the next request recovered in the same process and scene token. The underlying cause remains unproven. These results do not establish unconditional idle/background responsiveness or other-platform native behavior.

## macOS application quit guard unverified

**Severity:** medium
**Area:** native document lifecycle

Pinned winit 0.30.13 gives the default Quit menu item the `terminate:` action and Command-Q equivalent (macOS menu.rs:66–73). Its application termination callback calls window-exit notification and internal exit (app_state.rs:65–71,166–172), while window Close separately queues CloseRequested (window_delegate.rs:144–148). Native testing of Command-Q with a synthetic dirty scene remains pending; no data loss has been demonstrated. The hidden-dispatch change guards viewport Close and does not establish interception of application termination.

## Bounded World-axis Move candidate

Move currently implements one selected object transform, World X/Y/Z arrows and
one guarded preview transaction through native UI and MCP. Plane/center/component
handles, snapping, other orientations and Rotate/Scale tools are unavailable.
Selected pivots, driven data, unsupported/unstable parents and translations with
unproved retained source words reject editing. MA and general source authoring
remain subject to their existing read-only/source guards. The 72-physical-pixel
geometry and near-parallel threshold are application policies, not Maya parity.

Native/SDK tests for this candidate are in the workspace and mcp-test-client
suites; CI for the current head is the record of their status. Native
framebuffer/desktop and genuine source-bound Save consumer checks remain open.
The prior changed-parent full MCP-client run observed an unchanged transport
notification-flood outer-timeout failure; its single isolated reproduction
passed. The full run remains failed and the timing cause unproved.

## Absent default transform leaves refuse Move on Maya binary scenes

**Area:** Move tool, Maya binary save

Maya `.mb` files omit default transform leaves (translate 0, rotate 0, scale 1),
and the binary encoder has no reviewed path to insert a missing leaf
(`src/native_scene/binary/encode.rs` refuses an absent origin). Move therefore
refuses a Binary-source object whose translate leaf is absent with the typed
`AbsentChannelLeaf` error (MCP code `absent_channel_leaf`; per axis, in the UI
status and in the Move inspection reason) instead of accepting an edit that could
never be saved. The refusal rate on real Maya files is unmeasured but is expected
to be high, because default leaves are routinely omitted; the Move tool is
effectively limited to objects that already carry a translate leaf. Source-free
scenes (new, OBJ) are not refused but cannot be saved as Maya binary. Leaf
insertion is a separate follow-up (#19).

## Move lifecycle gaps

**Severity:** low
**Area:** Move, Rotate and Scale tools, MCP lifecycle

A remote-owned Move, Rotate or Scale has no timeout and is not tied to the MCP client session. If
the client disconnects it stays active, preview included, until a matching
`finish_manipulation` from a later client with the same scene token and manipulation ID, or a human
cancel (Escape, tool/selection change, New/Open or close). Until then every other
mutating operation, Grid toggle and selection-mode change reports
`busy_manipulation`. Replacing the scene (New or Open) also resets the active tool
to Select; Maya keeps the current tool across scene changes.

If the baseline or source of an active manipulation cannot be validated at finish or
cancel (believed unreachable), the transaction is still cleared first, so the
history is not left busy. The scene then keeps its preview values and the
baseline is lost: the document is dirty rather than silently restored, and the
Channel Box is resynchronized to the scene.

## Status Line is a partial subset (#59)

The strip shows:

- a menu-set selector where only Modeling is active; Rigging, Animation, FX and Rendering are disabled;
- New, Open and Save buttons, which share the File menu's handlers and enable state;
- Hierarchy (Root sub-mask only), Object and Component selection modes. The Component button enters Face
  from Object and Hierarchy, keeps the current mode in Vertex, Edge and Face, and stays pressed in all three;
- Vertex, Edge and Face component-type buttons (#29), which dispatch the F9, F10 and F11 operations. Their
  order and icons are original and the Maya 2026 icons and order are UNCONFIRMED. They are exclusive modes,
  not Maya's per-type selection mask toggles; there is no UV or Vertex Face button;
  Like F8 to F11 and the other mode buttons they cancel an active native Move, Rotate or Scale
  manipulation rather than being disabled, but are disabled while a remote manipulation is active;
- a selection mode menu (presets), shown disabled with no state (visibly unsupported; real presets need masks);
- five sidebar toggles, of which only Channel Box / Layer Editor works. The others are disabled.

Groups without AztecCraft behavior are not drawn rather than shown as inert controls: per-type selection masks,
snaps, symmetry, construction history, render buttons, Hypershade and the input box. The right-end
cluster therefore follows the last drawn group.

- The F2 to F6 menu-set hotkeys are deferred.
- No Layer Editor exists, so the toggle shows or hides only the Channel Box.
- The toggle is disabled during an active manipulation, including a remote one with no native gesture, like other edits.
- Save As has no strip button and stays in the File menu.
- Open's native dialog calls `rfd` directly and cannot be click-tested, so equivalence between the strip
  and menu Open is checked only for enable state and hover text.
- Hierarchy supports the Root sub-mask only; Leaf and Template are unsupported.
- Grouped hierarchy exists only in opened `.ma`/`.mb` files. Delete on a group is refused there, because
  structural edits are refused for source-bound scenes. On flat scenes Hierarchy selects what Object selects.
- F8 from Hierarchy goes to Face and from Vertex or Edge to Object (PROVISIONAL; the 2026 docs only document Object/Component).
- The selection mode menu (presets) is disabled and stateless until masks exist (#29).
- Mode intents in one frame (buttons, F8, F9, F10, F11) apply before Delete, so Delete sees the new mode.
- `select_node` stays exact and is not limited by the selection mode.
- Nothing was compared against a pinned Maya 2026 install.

Tracked in the UI parity inventory (`.project-docs/ui-parity-inventory.md`, section 2).

## Poly Sphere is provisional (#43)

Create > Polygon sphere and `create_primitive` with `sphere` build a radius-1,
20x20, Y-axis sphere (382 points, 780 edges, 400 faces: 40 pole triangles and
360 quads). The radius and subdivisions come from the polySphere node reference;
the command page lists radius 0.5, and Maya 2026 defaults are unverified (#5).
Vertex and face order and winding are not claimed to match Maya. Edges are hard,
so the viewport looks faceted. There is no options dialog, no UVs or normals, and
new nodes are not persisted (Save is unavailable for created scenes, as for the
cube). Pole triangles cannot be extruded; body quads can. The new node is named
`pSphere1` with shape `pSphere1Shape`, whereas Maya names the shape `pSphereShape1`.
The legacy quad-only JSON/OBJ scene refuses spheres with a typed error.

## World-space Rotate

`NativeScene` does not model a node's `inheritsTransform` (`it`) or `offsetParentMatrix`
(`opm`) attributes. World-space Rotate relies on the importers refusing them: the ASCII
importer rejects any `.it` or `.opm` line, and the binary importer rejects any undeclared
transform record with such a key (`it`, `inheritsTransform`, `opm`), so these scenes do not
open. Maya's on-disk binary encoding of these attributes is not confirmed by any fixture in the
repo; the tests only pin the refusal of the keys. A scene is therefore safe from
`inheritsTransform` off only while that importer behavior holds.

The ASCII importer does read `.ro`, `.ra`, `.sh`, `.rpt` and `.spt` into the transform, and
the view, picking and handle frames use them. Those scenes are read-only, so nothing edits
them. When a scene carries them, Rotate (object and world) refuses a non-xyz rotate order or a
non-zero rotate axis (`RotateUnsupported::RotateOrder` and `RotateAxis`), and World Scale
refuses a non-zero shear (`ScaleUnsupported::Shear`), because Euler decomposition and the
channel deltas assume xyz order without them. The binary importer does not parse these
extras yet. The remaining gap is Maya-style rotation for these cases (plan Task 2d).

World-space Rotate refuses parents that are not a rotation times a uniform
positive scale (relative tolerance 1e-9) and nodes with non-positive scale (a
policy choice consistent with Move).

The World ring treats the channel that moves most as intended and snaps changes
below 1e-12 rad on the other channels back to the baseline. That is a heuristic: with
a parent rotated 45 degrees about Z, a World X ring of 1e-11 degrees changes
rotateX and rotateY by about the same amount, and one of them is snapped away.

## Poly Cylinder and Poly Cone are provisional (#43)

Create > Polygon cylinder and cone, and `create_primitive` with `cylinder` or `cone`,
build radius-1, height-2 (y from -1 to 1), Y-axis solids with 20 axis divisions,
1 height division and no cap subdivisions. The cylinder is the sc=0 variant (node-page
default) with 40 points, 60 edges, 22 faces and two true 20-gon caps; the command page
lists sz 1 instead, so a Maya-default cylinder may have different counts, and the
cap topology of both solids is derived, not verified against Maya 2026 (#5). The cone
has 21 points, 40 edges and 21 faces (20 side triangles and a 20-gon base) with its apex
at (0, 1, 0) and its base at y=-1. Vertex and face order and winding are not claimed to
match Maya. Edges are hard, so the viewport looks faceted. There is no options dialog,
no UVs or normals, and new nodes are not persisted. Caps and cone side triangles cannot
be extruded; cylinder side quads can. A pick on any cap triangle selects the whole cap,
and its centre marker is the corner average. New nodes are named `pCylinder1` and
`pCone1` with shapes `pCylinder1Shape` and `pCone1Shape`, whereas Maya names the shapes
`pCylinderShape1` and `pConeShape1`. The legacy quad-only JSON/OBJ scene refuses both
with a typed error. No shelf icons exist for them.

## DefaultLightingApprox is not Maya default lighting

`DefaultLightingApprox` (`src/shading.rs`) is a headlight Lambert plus ambient floor, not Maya's
default lighting. The 2026 Lighting page was unreachable, so the light direction and whether it
follows the camera are unverified. It is wired into `Submission::new` as one flat colour per
face, with the normal taken to world space by the inverse transpose of the world matrix. The
ambient floor (0.4) is chosen for legibility, not measured from Maya (provisional). Back faces
receive only the ambient floor (no two-sided lighting, culling off), so an open plane looks dark
from behind; Maya's two-sided default is unverified.

## Wireframe shading keeps triangle picking

Wireframe skips the surface fill but picking stays the CPU triangle ray test and face-center
filter, so a click inside a face still selects it. Face-mode center markers are drawn only for
centers that filter accepts. The selected-face fill disappears in Wireframe, leaving only the
orange edges (current behaviour, not changed). Maya's Wireframe picks by edge or marquee.
Edges are always drawn in Smooth Shade All (Maya's default is off). The 4 and 5 hotkeys are confirmed by
the 2026 hotkey page; the Shading menu rows follow the 2026 panel-menu page.

Wireframe Face-mode centre markers cost O(active-shape faces x scene triangles) per frame, uncached.
Over the 1_000_000 work budget (about 710 faces in a single mesh) the frame hides the centre markers
and the viewport overlay shows "Face centres hidden: too many faces for occlusion check"; edges and
grid still draw. Hidden, not unfiltered, because unfiltered markers would show unreachable centres
the picker rejects. Picking is unaffected apart from its own budget error. A cache or spatial index
would lift the limit.

## Rotate exactness is partial (#128)

A Rotate that changes exactly one channel by the requested delta stores `baseline + delta` in the scene's declared unit, bit for bit (object-space X ring, single-channel numeric entry and MCP `update_manipulation`, in degree and radian scenes, and Y/Z or world rotations whose decomposition changes one channel within the snap tolerance of that value). A numeric or MCP angle is carried in the scene's angular unit alongside its degree preview value, so a radian scene does not round trip it through degrees. A ring drag has no declared angle: its degree angle is the request. One case is computed rather than exact, and may differ from `baseline + requested` by about 1 ulp:

- A Y/Z ring or world rotation that genuinely changes several channels (Euler decomposition).

Untouched channels are preserved bit for bit. Inspection echoes the requested value as given after a remote update, so it does not reveal these differences.

## Rotate and Scale handle limits are provisional

The Rotate ring angle comes from a ray-plane hit. Just above the edge-on limit
(|ray . ring normal| of 1e-3, about 0.06 degrees from the plane) the hit is
far away and the angle can swing sharply per pixel; below it the drag is refused as
Degenerate. The limit, the 72 px handle size, the 1 px Scale pivot refusal and the
100 px uniform-scale reference are provisional, not taken from Maya, were not changed
by the UI wiring and use physical pixels (they ignore HiDPI scaling). Maya 2026 back-half
ring behaviour is unverified.

Move arrows and Scale shafts and boxes draw at 72 px times the sine of the angle between
the axis and the view ray, so an end-on axis is short and picks exactly where it is drawn
(#155). Plain perspective foreshortening was chosen because a search of the Maya 2026
manipulator help found no documented minimum length, fading or hiding of end-on axes; the
rule is unverified against Maya itself. It is a first-order approximation: exact end-on.
Off-centre it draws short. For a pinhole camera, a small displacement perpendicular to a view ray `a` off
the camera axis, at distance D, shows on screen scaled by 1/(D cos a) out of the ray/axis
plane and 1/(D cos^2 a) in it (checked with a finite-difference projection), against 1/D
for the same displacement at the same distance on-axis. The constant 72 px times the sine
is therefore short by between 1 - cos(a) and 1 - cos^2(a), depending on the axis
direction: 29-50% at the edge midpoints and 42-67% in the corners of a square viewport at
a 90 degree field of view, approaching 100% as a nears 90 degrees. The finite length of the
handle adds a separate, signed, field-of-view-dependent error this rule does not model.
Scale plane squares stay at
full length while the shafts shorten, so they can stick out past the tips; they are disabled
and unpickable, so this is cosmetic.

A Rotate drag whose samples are refused for a stretch (edge-on ring, pointer behind the
camera) can lose whole turns. Each accepted sample adds the shortest signed step from the
last accepted angle, so a pointer that circles the pivot by more than half a turn while
samples are refused resumes on the wrong multiple of 360 degrees. Nothing is lost for
refusals by the scene, because the drag keeps tracking the angle itself.

Handle frames must be right-handed and perpendicular. `HandleFrame::new` refuses a
mirrored one (`MirroredFrame`) or a skewed one (`SkewedFrame`), which an Object-space
frame under a negative or uneven scale parent produces. The tool shows "Rotate unavailable"
or "Scale unavailable" with the reason, and World space still works. A zero or negative Scale
ratio is reachable by an ordinary drag; the core refuses it with InvalidFactor, the status
line shows the refusal, and the drag continues from the last valid preview (release commits
that preview).

Rotate and Scale share one Object/World selector; Maya keeps one per tool. The selector,
the axis choice and the numeric field are a row above the viewport standing in for the
unsupported Tool Settings panel. Gimbal, Parent, Normal, Custom and Component are listed
disabled. The MCP `update_manipulation` takes an absolute Rotate angle in the scene
angular unit and the native numeric field takes degrees; both reach the core as degrees.

## Grid lines over primitive faces are expected (#111, closed)

#111 closed as expected behaviour: origin-centred primitives straddle the y=0 grid plane, so grid
lines in front of the lower faces are correctly drawn; pinned by
`headless_gpu_grid_is_hidden_by_opaque_faces_but_shows_through_wireframe`.

## Selection tint is distinct on screen only for tested lighting (#122)

`selected_base` guarantees a gap of at least 3 on one channel between the selected and the
unselected base colour, which keeps the lit pixels different at the 0.4 ambient floor, obliquely and
front-on (grid test over the neighbourhood of the tint plus a 52-step colour grid). It is a display
legibility rule, not a Maya selection colour, and it does not cover colour management or future
lighting models.

## Session materials are a first, MCP-only slice (#51)

`create_session_material`, `assign_session_material` and `set_session_material_color` edit a
session-only lambert table. It is not saved (Untitled scenes have no save), sourced binary scenes
refuse it with `unsupported_edit` and Maya ASCII scenes with `read_only_format`; it stays separate
from the saved Phong `set_material_diffuse`. There is no native UI until the Attribute Editor
branch lands. Assignment is whole-shape only: per-face assignment and instance-specific groups
are follow-ups. Ids are table indices reused after undo, so clients must not cache them. The
table holds at most 512 materials, far inside the `inspect_scene` budget (tested); creating past it returns `session_material_limit`. The default shader is unverified for Maya 2026 and linear colour
is shown as sRGB bytes, so no parity is claimed. No MCP test creates two instances of one shape
(no instancing operation exists yet); that case is covered by core and `ViewScene` unit tests.

Right-drag dolly direction is PROVISIONAL (#154): dragging up moves the camera back and
dragging down moves it in, following a user report. The Maya help pages for tumble, track
and dolly (help.autodesk.com/cloudhelp/2023/ENU/Maya-Basics/files/GUID-941480C1-9BDA-4DB3-8EA8-113A0D6FAF1F.htm;
and the 2026 keyboard shortcuts page, which lists only the Alt bindings) give no direction for the Alt + right-drag, so it is unverified against
Maya. Horizontal drag does not dolly. AztecCraft now requires Alt for
right-drag dolly and middle-drag track (#158), as Maya does; plain middle/right drags are unbound.
The mouse wheel dolly without Alt is PROVISIONAL: the Maya 2026 shortcut page lists no wheel
shortcut. Alt-drag may be grabbed by some Linux window managers; reconfigure the WM.

## MCP file-error text is not redacted in code

**Severity:** low
**Area:** MCP
**Issue:** cristim/AztecCraft#221

`.project-docs/mcp.md` says file errors redact private paths, but `tool_error` in
`src/mcp/tools.rs` returns `OperationError::File` text unchanged, and `DocumentError::Io`
text includes the path. No remote operation takes a path today, so remote calls cannot
currently reach such text. `inspect_scene.status_message` withholds file-error status text
(`Editor::set_error_status`); the tool-error channel does not.

## OBJ export limits

File > Export All and Export Selection (and MCP `export_obj`) write only the
displayed polygon geometry: world-space positions in centimetres and polygon faces,
one `o` group per instance. Hidden or non-displayed instances are not exported
(PROVISIONAL: the Maya docs do not say whether Export All includes them). Normals,
UVs, materials, smoothing groups, curves and Maya's OBJ Export Options are not
written or offered (the option boxes are disabled), and export never overwrites an
existing file. The view of the whole scene is built first, so one mesh the viewport
cannot display (for example a concave n-gon) fails every export, including Export
Selection of another object. The Windows save dialog may first ask whether to
replace an existing file; confirming still fails with "destination already exists"
and the file is left untouched. The output has not been compared with
Maya's objExport plug-in; OBJ import is unsupported.
