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

## Remote Delete is not refused in Face selection

**Severity:** medium
**Area:** editor operations
**Issue:** cristim/AztecCraft#83

The native UI refuses Delete in Face selection ("Face deletion is unsupported") in
`native_operation`, above the shared pipeline. `remote_operation_guarded` has no such check, so
the MCP `delete_node` tool deletes the object in Face mode. The guard matrix records the current
behavior (`delete(face_mode)` cells); fix it in the shared pipeline and update the snapshot.

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

Undo/redo stores unbounded NativeScene snapshots, so memory grows with scene size
and edit count. The native model retains separate transform/shape identities,
instance hierarchy, f64 authoring channels and original source bytes. Triangle
and quad derived views are supported; arbitrary loops/holes remain retained but
unsupported for projection. Imported editing is limited to proved source-bound
numeric changes; missing-record insertion and structural rewrites refuse safely.
The provisional Unix binary Save/Save As candidate preserves proved existing
records and reconciles uncertain installations. It does not support Maya ASCII,
Windows writing or structural rewrites, and still needs native Save QA.
Recovery, autosave and camera/history persistence remain
pending. Legacy JSON v1 and OBJ remain separate library utilities; their geometry
tests don't establish native persistence. GUI Export remains disabled. Structural
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
are unsupported. Native Extrude remains disabled until a real history and
manipulator workflow exists.

Ambiguous coalesced primary releases are refused with an actionable single-click
retry rather than guessed coordinates/modifiers. This limitation was reproduced
with an actual Shift click followed by an unmodified drag release in one frame.
Single-release event-time modifiers and pure Alt navigation are preserved. The
attempted combined menu activation/key/viewport-click sequence is blocked by the
pinned popup routing; separate menu+F8 and F11+F8+viewport-click tests do not prove
that combined journey. Native window, other-platform input and Maya comparison
remain separate verification requirements. See [selection policy](.project-docs/development.md#face-center-selection).

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
**Area:** Move tool, MCP lifecycle

A remote-owned Move has no timeout and is not tied to the MCP client session. If
the client disconnects it stays active, preview included, until a matching
`finish_move` from a later client with the same scene token and move ID, or a human
cancel (Escape, tool/selection change, New/Open or close). Until then every other
mutating operation, Grid toggle and selection-mode change reports
`busy_manipulation`. Replacing the scene (New or Open) also resets the active tool
to Select; Maya keeps the current tool across scene changes.

If the baseline or source of an active Move cannot be validated at finish or
cancel (believed unreachable), the transaction is still cleared first, so the
history is not left busy. The scene then keeps its preview values and the
baseline is lost: the document is dirty rather than silently restored, and the
Channel Box is resynchronized to the scene.

## Status Line is a partial subset (#59)

The strip shows:

- a menu-set selector where only Modeling is active; Rigging, Animation, FX and Rendering are disabled;
- New, Open and Save buttons, which share the File menu's handlers and enable state;
- Hierarchy (disabled), Object and Component selection modes. Component means Face, the only
  implemented component mode;
- five sidebar toggles, of which only Channel Box / Layer Editor works. The others are disabled.

Groups without AztecCraft behavior are not drawn rather than shown as inert controls: selection masks,
snaps, symmetry, construction history, render buttons, Hypershade and the input box. The right-end
cluster therefore follows the last drawn group.

- The F2 to F6 menu-set hotkeys are deferred.
- No Layer Editor exists, so the toggle shows or hides only the Channel Box.
- The toggle is disabled during an active Move, including a Move with no native gesture, like other edits.
- Save As has no strip button and stays in the File menu.
- Open's native dialog calls `rfd` directly and cannot be click-tested, so equivalence between the strip
  and menu Open is checked only for enable state and hover text.
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

`NativeScene` does not model a node's `inheritsTransform` attribute. World-space
Rotate relies on the importers refusing it: the ASCII importer rejects any `.it`
line (and `.ro`, `.ra`, `.sh`), and the binary importer rejects any undeclared
transform record with such a key (`it`, `inheritsTransform`, `ro`, `ra`, `sh*`),
so these scenes do not open. Maya's on-disk encoding of these attributes is not
confirmed by any fixture in the repo; the tests only pin the refusal of the keys.
A scene is therefore safe from `inheritsTransform` off only while that importer
behavior holds. Follow-up: `ro` (non-xyz rotate order), `ra`, `sh*` and
`opm` must stay refused if the importers ever accept them, because Euler
decomposition and `matrix()` assume xyz order without them.

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

## Rotate and Scale handle limits are provisional

The Rotate ring angle comes from a ray-plane hit. Just above the edge-on limit
(|ray . ring normal| of 1e-3, about 0.06 degrees from the plane) the hit is
far away and the angle can swing sharply per pixel; below it the drag is refused as
Degenerate. The limit, the 72 px handle size, the 1 px Scale pivot refusal and the
100 px uniform-scale reference are provisional, not taken from Maya, and are to be
tuned with the Task 5 UI wiring. Handle frames must be right-handed; the caller
must refuse mirrored frames. A zero or negative Scale ratio is reachable by an
ordinary drag and the core refuses it; the UI must show that refusal.
