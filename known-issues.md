# Known limitations

## OBJ import is a first slice (#22)

**Severity:** medium
**Area:** interchange
**Issue:** #22

File > Import... and MCP `import_obj` add the meshes of an OBJ file to the scene as one undo step. No
parity with Maya is claimed and nothing was compared with Maya's OBJ reader (#5).

- **Meshes only.** `v` and `f` records, `o` and `g` groups (one new object per group, Multiple
  Objects semantics). Normals, UVs, materials, smoothing groups and `.mtl` files are counted in
  `ignored_records` and dropped. The Group and Single Object options and other import types do not
  exist; the option box beside Import... is disabled.
- **Opened Maya files refuse it.** A scene opened from `.mb` or `.ma` cannot take structural edits,
  so Import returns the same refusal Create gives. It works in Untitled, authored and legacy
  `.aztec` scenes.
- **Strict whole-file refusal.** Free-form geometry, unknown records, overlapping groups (`g a b`),
  bad or forward indices, faces with fewer than 3 corners or a repeated vertex, non-finite or huge
  numbers, invalid UTF-8, CR-only line endings and a file without a face are refused with a line
  number. Faces with 5 or more corners must also be convex and without holes, because the viewport
  refuses to draw the scene otherwise; the error says "triangulate the model and export again". Some
  real-world files will be refused until a repair mode exists.
- **Names.** Only the first token of an `o` line is used (`o a b` takes `a`). Names are reduced to
  letters, digits and `_`, prefixed with `_` when empty or digit-led, and numbered to avoid every
  node and shape name and Maya's default scene names. The exporter's `%XX` escapes are not decoded.
  Faces before any group take the file name.
- **Geometry.** Positions are stored as single precision in world space; unreferenced vertices are
  dropped; edges are shared and hard; point order follows the OBJ.
- **Every imported object is a root.** OBJ has no hierarchy, so none is restored (PROVISIONAL; matches
  Maya's Multiple Objects reading of groups).
- **Selection after import** is the new roots, in file order with the last as lead. The undo entry
  is named "Import". The scene token does not change.
- **The import runs on the UI thread.** A file at the limits blocks the window while it is read,
  checked for display and copied once for undo. There is no progress and no cancel.
- **Limits** are PROVISIONAL and not derived from Maya: 32 MiB per file, 64 KiB per line, 262,144
  vertices, 262,144 faces, 1,048,576 corners, 1024 corners per face, 1024 objects, 256-byte names,
  coordinates up to 1e12 (rows 34 to 42 of the policy table in `.project-docs/development.md`).
- **Cleanup.** The 256 MiB view budget now has one definition (`VIEW_BUDGET`); test-only copies in
  `src/viewport_renderer/tests.rs` remain.
- **Not natively verified.** The File menu entry is covered by a headless frame test and the
  operation by Editor protocol tests; the file panel was not exercised in a native window.

## Animation is a first, session-only slice (#361)

**Severity:** medium
**Area:** animation
**Issue:** #361, epics #52 and #62

Keys, a Time Slider, playback and MCP tools exist so a cube can be keyed, scrubbed, played and read
back. No parity with Maya is claimed, and nothing was compared with a running Maya (#5).

- **Not saved.** Keys live in the session and in undo history. A scene holding keys cannot be saved:
  Save is unavailable for a new scene, and the authored writers refuse keys rather than drop them.
  Deleting every key leaves the evaluated pose, which is then an ordinary edit.
- **Authored scenes only.** Sourced scenes refuse keys. Their animation curves are neither read nor
  evaluated, so scrubbing one moves the time and changes nothing, even when the file is animated.
  `inspect_keys` says so through `unavailable_reason`. A file's current time and range are not read.
- **Transform channels only.** Whole frames from -1000000 through 1000000, and 65536 keys a scene.
  Fractional times are refused. Rotation keys are in the scene's declared angular unit.
- **Time units.** Only `film` plays. An ASCII or binary scene in another
  admitted unit (ntsc) opens, and every time operation refuses (`unsupported_time_unit`); that refusal is tested in the core only.

PROVISIONAL, because the docs do not state it or the feature behind it is absent:

- Linear interpolation. Maya's default tangents are not implemented, so in-between values differ.
- Playback at 24 fps in real time, skipping frames, looping continuously. The 2027 Playback Controls
  page states no default for either.
- A playback range defaulting to 1 through 48. The 2027 [New Scene Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8F961875-1D41-4C52-9CD5-B90DE75C546C.htm) page gives
  animation start 1 and end 48 and no playback default. There is no separate animation range, and
  the Range Slider is absent.
- A manual edit of a keyed channel holds until the next time change, then snaps back. Undo and Redo
  re-evaluate at the current time only when the restored snapshot was taken at another time, which
  they do not restore; at an unchanged time they give back the recorded state, held edits included.
- Setting the time to the current frame replies unchanged and keeps a held edit.
- Step Key does not wrap at the first or last key, and Step Frame ignores the Loop Play Range
  preferences. A held step repeats, and S does not.
- Play from outside the playback range starts at the range start.
- `set_keyframe` with a time other than the current one keys the current value, per the
  [setKeyframe](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/setKeyframe.html) command, then re-evaluates that channel at the current time, so a held edit on
  it snaps back.
- Undo entry names `Set Key` and `Delete Keys`.
- A native time change with a channel draft is refused as busy. Clicking the slider while a field
  holds a draft reports that on the first frame, before the field's own blur commit.

Unsupported: tangents, Graph Editor, Dope Sheet, Auto Key, animation layers, Time Editor, Range Slider
gestures, Play Backwards (visibly disabled), audio, cached playback, the Key menu, F4, Set Key Options,
Alt+Shift+V and keying non-transform attributes.

Playback advances only while the window renders, and a hidden window stops it. Each frame that moves
a keyed channel changes the revision, so `inspect_scene` and `inspect_keys` cursors go stale during
playback; stop first to page. The timeline row had native checks on Linux with real mouse and key
input in a private virtual session; other platforms rely on CI, and Delete Key on a keyed frame is headless only.

## Manipulator size is PROVISIONAL and session-only

**Severity:** low
**Area:** viewport manipulators
**Issue:** cristim/AztecCraft#165

The + and - keys and `set_manipulator_size` set one global size from 0.1 to 10 for the Move,
Rotate and Scale manipulators (Maya's Global scale). No parity with Maya is claimed. The default,
key step grid, key repeat and drawn length (rows 15 to 19) and their evidence are in the [policy constants table](.project-docs/development.md#application-policy-constants).
The size is session-only (no preferences store; it never dirties the scene or enters history). The MCP tool accepts any value from 0.1 to 10, rounded to three decimals; a later key step moves to the next grid value in its direction that changes the three-decimal value. A key step stores the raw grid value, so an exact tie such as 1.5625 rounds to 1.563 for stepping but is stored as 1.5625.

- The drawn length has a floor before the view foreshortening, so at small sizes the
  manipulator stops shrinking on screen. A foreshortened axis (#155) can still become hard to pick.
- The arrowhead, boxes, centre zones and pick radius (8 px, Maya Pick Range) stay fixed in pixels.
- Since the pick radius became 8 px (#295), a click within 8 px of a manipulator handle grabs it
  instead of selecting the object behind. A disabled handle (the free-rotate circle, the view ring or a
  Scale plane) consumes the click with an unsupported status only when no enabled handle also lies
  within the radius; an enabled handle wins the overlap
  (`scale_planes_are_not_axis_hits_from_the_front_and_the_oblique_result_is_pinned`).
- A Scale axis whose tip lands inside the 7 px square centre box stays drawn but takes no hover
  or press, so the ring just outside the centre box falls through to the other axes, a disabled
  Scale plane (which consumes the click with its unsupported status) or the viewport (PROVISIONAL:
  Maya's docs do not describe end-on axes).
- At very large sizes with a wide field of view, ring segments behind the camera are dropped rather
  than near-clipped, and the hemisphere cull is approximate.
- Unsupported: the Preferences Manipulators page, Handle size and Line size (`manipOptions
  -handleSize` and `-lineSize`).
- Plain-text focus keeps the keys for the field (the shortcut gate returns early while a text field
  wants keyboard input); this is checked by code reading only, with no test. Native QA covered + and -
  resizing the Move manipulator, not text-field focus.

## Material data editing is a bounded candidate

The native Common Material Attributes/Diffuse control and `set_material_diffuse`
tool edit only an existing checked Phong scalar whose complete shared-shader
consumer closure is supported. Missing/default records, connected overrides,
unsupported consumers and graph edits are unavailable. Values use native f64
words and one shared History value; the slider does not clamp untouched source
data. No rendered material, texture lookup, assignment or color processor is
provided. Isolated material widget/SDK and default/headless/client checks pass;
the corrected protocol reopen journey was independently reproduced and bounded
macOS native edit/Undo/Save/reopen passed. Cold verification of the committed
source on main (`7ca0ebf`) and its CI passed (see
`.project-docs/native-verification.md`). The ordinary pre-commit hooks also passed on that tree. Still open:
native visual re-check of the committed build, and a published Save/NCRV/Face/Grid
composition, which no test covers (binary Save refuses curves; ASCII Save is
refused), so the feature stays a candidate.

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
Cmd-versus-Ctrl behavior is unverified. Native visual verification is pending. The body
scrolls with the wheel; Alt+middle-drag and Ctrl+Alt+middle-drag panning and wheel-over-tabs
node switching (Maya 2027 Attribute Editor page) are unsupported.

## Reference baseline unavailable

**Severity:** high
**Area:** parity verification

The exact Maya version/platform and reference installation have not been established. Identical behavior and visual parity cannot be verified yet. Build functionality without claiming parity; acquire a pinned reference before parity acceptance.

## Complete parity inventory not yet enumerated

**Severity:** high
**Area:** release gate

The original first-push restriction required 80–90% of all Maya 2026 functionality, including simulation, rendering, scripting, and plugin compatibility. The user's later explicit push/merge instruction authorizes early prototype publication, superseding that restriction. The full-functionality target remains unmet, and the complete inventory must still be enumerated and verified against a reference. Don't calculate a percentage from an arbitrary feature list.

The [parity inventory framework](.project-docs/parity-inventory/README.md) (status vocabulary, evidence record, PROPOSED documentation-conformance measurement rule and 14 unexpanded family files) is in progress; the slices are tracked in #6 and no family has rows yet. [The initial inventory seed](.project-docs/inventory-seed.md) provides source-backed candidate workflows, but its 100 entries aren't a complete denominator. Component versions, cloud services, collection components and third-party plug-in scope still need to be pinned. Overall coverage remains unmeasured.

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
Unix and Windows binary Save candidate uses the ordinary vendored atomic engine. Its
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

Undo keeps the newest 50 NativeScene snapshots and silently drops older ones. The 50 matches
the Maya 2027 Undo preferences default; the fixed size (no Infinite choice) does not, and
the evidence is in the [policy constants table](.project-docs/development.md#application-policy-constants) (row 21; configurable limit pending #178). Redo is
unchanged. The dirty state compares the live scene with the saved scene by value,
so dropping old entries cannot make an edited scene read clean. Memory still
scales with scene size times 50. Autosave and persisting history remain pending.

Undo and Redo entry names are PROVISIONAL: the Maya 2027 documentation names no undo entries. A
transform edit that changes several value groups reads "Transform"; Channel Box and numeric edits
read "Edit" whatever channel changed.

Selection, camera, tool and panel changes do not create undo entries in
AztecCraft (`.project-docs/mcp.md` already says selection does not). Whether Maya
puts selection changes on its undo queue is not stated by its Undo preferences page; see
row 22 of the [policy constants table](.project-docs/development.md#application-policy-constants).

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
The provisional Unix and Windows binary Save/Save As candidate preserves proved
existing records and reconciles uncertain installations. It does not support Maya
ASCII or structural rewrites, and still needs native Save QA. Windows limits are
listed under "Windows binary Save limits".
A core recovery-file store (`src/recovery.rs`: staged private writes, never replaces an occupied name, per-session retention, lock-based crash markers) exists but is not wired to the editor, preferences, MCP or a launch dialog; autosave, recovery UI and camera/history persistence remain
pending (#21). Legacy JSON v1 and OBJ remain separate library utilities; their geometry
tests don't establish native persistence. Structural
validation doesn't repair geometry or guarantee rendering fidelity.

The camera core is connected to a depth-tested opaque GPU geometry viewport with hardware clipping. Materials, transparency, normal shading, textures, document color management and reference navigation parity remain pending. Backend failures remain explicit with no CPU fallback. These limits are described in [development instructions](.project-docs/development.md).

Checked source appearance summaries now distinguish supported immutable source
associations from unavailable or unbound shapes in the Help Line's "Scene
diagnostics" header, also shown in the Attribute Editor body while a node is
selected, and in MCP inspection. They do not evaluate materials, resolve texture assets, prove
quadratic-filter equivalence or provide a color processor. The geometry preview
continues to report incomplete appearance; correct display and Maya comparison
remain acceptance work.

The extrusion API supports one quad with signed object-space normal displacement,
including zero-distance topology. All-face extrusion, including a one-quad plane,
is explicitly unsupported until a pinned Maya fixture establishes default behavior.
Region/world extrusion, divisions, offset/taper, UVs/normals, construction-history
nodes, the Extrude manipulator and multi-face Extrude remain pending (native single-face Extrude with a Thickness field exists, see the component selection entry).

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
markers and plain primary-click replacement/clearing. The marker, click box, center
definition, center-ray visibility, depth tolerance and work cap are application policies,
not verified Maya defaults (rows 1 to 5, 12 and 13 of the [policy constants table](.project-docs/development.md#application-policy-constants)). Nonplanar faces can
hide their own centers. Occluders within the face-center depth tolerance of the center's
view depth (z-fighting range, e.g. stacked planes 1e-3 apart at distance 50) do not hide it;
unobstructed finite degenerate centers can be selected.
Other component masks, multiple-face gestures, component marquee (the object marquee exists, see
"Object marquee is a provisional subset"), hover and marking menus are unsupported. The same holds in Vertex and Edge modes: a Shift or Ctrl click is refused
with ModifiedClick and leaves the selection unchanged. Maya supports modified clicks to add
to or toggle the component selection; AztecCraft selects one vertex, edge or face only. Native Extrude of the single selected face is live (issue #44, slice 1): the shelf Extrude button opens an Extrude Options window with Thickness only (default 0, applied as the normal displacement, PROVISIONAL). The other option rows, including Taper 1 and Twist 0 (polyExtrudeFacet node defaults, PROVISIONAL), are disabled. There is no manipulator, no multi-face region and no all-face extrusion; a Thickness of 0 gives a coplanar cap with zero-height walls, and its disclosure in the status line is a later step. PROVISIONAL, with no Maya source: the shelf Extrude button opens the Options window instead of extruding at once, and the disabled Divisions 1, Offset 0, Smoothing angle 30 and Curve Selected rows show display defaults, not documented ones.

When one frame carries several primary releases (egui keeps one click/drag state per frame, so
the frame cannot say which release owns it), a last release that is a plain click acts alone with
its own position and modifiers; earlier releases of that frame are superseded (#31). Any other
coalesced frame (last release a drag or modified click, e.g. a Shift click followed by an
unmodified drag) is still refused with a single-click retry rather than guessed coordinates or
modifiers. Single-release event-time modifiers and pure Alt navigation are preserved. The combined
menu activation, key and viewport click journey is covered by a native test across frames; pressing
Enter on a menu item and clicking the viewport inside one frame stays intercepted by the popup.
Real window, other-platform input and Maya comparison remain separate verification requirements.
See [selection policy](.project-docs/development.md#face-center-selection).

## Vertex and Edge picking and drawing constants are provisional

**Severity:** low
**Area:** component input and viewport

F9, F10, the Select > Components entries and a plain click reach `Camera::pick_vertex`
and `pick_edge` (#29; the Status Line Vertex, Edge and Face buttons dispatch the same modes). F8 leaves Vertex and Edge for
Object and enters Face from Object and Hierarchy; Maya's F8 return target is
UNCONFIRMED, so this is provisional. A click on empty space clears the active type's
pick, also UNCONFIRMED for Maya. Switching between Vertex, Edge and Face keeps each type's
pick and restores the type you switch to: the 2027 Selection preferences page documents
Preserve Component Selections, on by default (polygon components only), which backs up the
component selection on a type switch and restores the type switched to (#29). Maya's
off setting, which would clear on a switch, is not implemented.
The Channel Box shows only the parent object's channels in Vertex, Edge and Face modes; there is
no component feedback (coordinates or attributes) for the picked vertex or edge.
The vertex click box, edge click distance, vertex marker, selected-edge strip, candidate
ranking and depth tolerance are original provisional values listed in the [policy constants table](.project-docs/development.md#application-policy-constants)
(rows 6 to 11), not verified Maya defaults. Colours follow only the Maya 2026 colour words (purple-ish
vertex, highlighted when selected, light-blue edge, light-orange selected edge) with
original RGB values. In Vertex and Edge modes component
feedback replaces Object feedback, so the selected object's faces are not tinted and
selected curves are not highlighted.

Test limits for this slice: the `sync_fields` owner-path test fakes the selected path through `set_selection_for_test`
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
draw and pick agree; the face centre keeps its own tolerance (table row 5). Readbacks (Apple M1 Pro, Metal, side-2
cube at the origin, near 0.01, far 10000, yaw 0.6, pitch 0.4, 1024 px square, 45 degree
fov; the committed ignored default-camera readbacks were re-run on the same adapter at the default near 0.1 and the film-fit lens and pass): every corner with a front-facing face showed all 25 marker pixels and no hidden
corner drew at the default distance 8 and at 120 for tolerances 6e-3 through 8e-3; 5e-3
left one silhouette corner at 20/25, and 2e-2 let a hidden corner show at 120, so the
margin is thin. The 5e-3 and 2e-2 bracket comes from manual readbacks with the constant
temporarily edited; no committed test reproduces it (the ignored readbacks pin only the
6e-3 outcome). Those numbers come from the Apple M1 Pro Metal adapter only. CI's
software Vulkan job runs the GPU tests, but it was not used to choose or bracket this
value; other adapters were not run. Limits: the tolerance is relative to distance, so
plates thinner than about 0.05 units at the default distance 8 (6e-3 times 8) can show,
or let a click pick, the vertex behind them; objects only a few px across can have a
hidden corner picked; and at large distances a flat 6e-3 under-covers the Depth32Float
resolution (no readback needed more at d=120 or d=300 with small-object runs, but the
buffer's relative resolution grows with distance). The relative depth step is about
5.96e-8 * z / near, which reaches 6e-3 near z = 1007 at near 0.01 and near z = 10067 at
near 0.1, so the default near of 0.1 moves that limit beyond the default far plane. A pixel-derived bias is
the planned follow-up (#135).
The default near clip is 0.1 (Create Camera Options default; the persp camera's own is
undocumented, PROVISIONAL). Dolly has no minimum distance, so a distance below the near
clip puts the target closer to the eye than the near plane (between the eye and the plane) and clips it; whether Maya has a hard dolly
stop is PROVISIONAL (no Dolly Tool page cited). A biased vertex marker (0.6% toward the
eye) can be near-clipped while its surface vertex at eye depth in [0.1, 0.1006) stays
visible; negligible and pre-existing.
Frame All and Frame Selected floor the distance at twice the near clip plus 5% (0.21 at the
default near), so an object with a radius under 2·near·sin(limiting half angle), about 0.065 at aspect up to 1.5 and less in wider viewports, frames smaller than the screen
fraction other objects get. There is no per-camera near clip control (Maya exposes Near Clip
Plane on the camera); a Camera Attributes near-clip setting is the candidate, not built.
The default lens is film-fit (development.md row 25). Only the Fill film fit is built;
Horizontal, Vertical and Overscan fits, film offset, camera scale and lens squeeze are
unsupported, and the focal length and aperture cannot be changed. The Fill threshold, the Fill
default and treating the viewport aspect as the resolution gate are PROVISIONAL. Pan moves
`pixels * distance * 0.002` world units whatever the field of view, so on screen it runs about
1.21x (aspect <= 1.5) to 1.43x (16:9) faster than under the old 45 degrees; it is not
cursor-locked (pre-existing). MCP-only sessions that never painted frame at viewport aspect 1.0
(a placeholder), so Frame All there is framed for a square viewport; `fov_y_degrees` then
reports the 1.0-aspect value. Readbacks calibrated at 45 degrees (the marker bracket above) were
not repeated at the film-fit lens beyond the committed default-camera ones.

Wireframe hides occluded vertex markers like face centres, but draws the selected edge
and every edge through surfaces. When that check is over budget `vertex_markers_hidden`
(Vertex mode) or `face_centres_hidden` (Face mode) is true. The renderer draws every
`ViewInstance` edge; the bin crate has no loose-edge fixture, so the face-less case is
covered by the carried edge list and the picker tests, not pixels.

## Select menu: All by Type is not implemented; the rules are Object and Hierarchy mode only (#325)

The Select menu follows Maya 2025.1's order (`buildSelectMenu.mel`): All (Ctrl+Shift+A), All by
Type, Deselect All (Alt+D), Hierarchy and Inverse (Ctrl+Shift+I, hint only), with labels from
the `SelectAll`, `SelectNone`, `SelectHierarchy` and `InvertSelection` runtime commands and
hotkeys from `hotkeySetup.mel`.
Deselect All is the typed `DeselectAll` operation shared by the menu, Alt+D and MCP
`deselect_all`; like Maya's `SelectNone` (`changeSelectMode -component; changeSelectMode
-object; select -cl`) it also clears remembered components and returns to Object mode from Face,
Vertex, Edge or Hierarchy. All, Hierarchy and Inverse are the typed `SelectByRule` operation shared by the menu,
Ctrl/Cmd+Shift+A (All) and Ctrl/Cmd+Shift+I (Inverse). They select transform instances, nested ones
included (PROVISIONAL), and in Hierarchy selection mode All and Inverse select roots only and
Hierarchy changes nothing. Maya's Hierarchy adds "parent and child" nodes; ours adds descendants
only (PROVISIONAL). In Face, Vertex or Edge mode Maya selects components, which cannot be
multi-selected yet (#30), so the rows are disabled and the operation is refused, a documented
deviation. All by Type stays a disabled row with a reason (#325); the
[2027 Select menu page](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D1BBC051-0EE7-4A91-B4FF-2A369764EC8B.htm)
confirms these labels, their order, Alt+D, Ctrl+Shift+A and Ctrl+Shift+I and lists All by Type as a
submenu, drawn here as a plain disabled row. Similar, Grow, Shrink and the rest of Maya's Select
menu are absent. Option+D on macOS relies on egui-winit's physical-key fallback (PROVISIONAL).

## Select All, Hierarchy and Inverse: PROVISIONAL scope choices (#325)

The Select menu page does not state these, so each is a pinned choice, not Maya parity:

- P1/P2: Select All selects every transform instance, nested children included, in scene order
  with the last as the lead (`select_all_in_object_mode_selects_nested_instances`). A grouped
  scene therefore selects parent and children together, and Delete of that selection removes each
  subtree once.
- P3: in Hierarchy selection mode (Root sub-mask) All and Inverse select root transforms only and
  Hierarchy changes nothing (`select_all_in_hierarchy_mode_selects_roots_only`,
  `select_inverse_in_hierarchy_mode_swaps_roots_only`,
  `select_hierarchy_in_hierarchy_mode_is_unchanged`).
- P4: Hierarchy adds every descendant of the selected objects and no parents, although the page
  says "parent and child"; the lead stays the lead and added nodes sit before it
  (`select_hierarchy_adds_descendants_keeps_lead`).
- P5/P6: Inverse selects the unselected instances in scene order, last as the lead, and an empty
  selection selects everything (`select_inverse_of_an_empty_selection_selects_everything`).
- P7: Maya selects components in a component mode; here All, Hierarchy and Inverse are refused in
  Face, Vertex and Edge mode (components cannot be multi-selected until #30), natively with
  disabled rows and over MCP with `invalid_input`
  (`select_rules_in_component_modes_are_refused_and_keep_state`).
- P17: after Delete of several objects the selection is empty and Undo restores the nodes but
  not the selection (`undo_of_delete_selection_restores_nodes_but_not_the_selection`).

## Frame Selected with no visible geometry frames the object origin (PROVISIONAL, #335)

F frames parents only and Ctrl+F frames with children, per the [2027 Frame page](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-C420C235-B8DC-4129-8449-8B7B7911F94B.htm). The page
is silent on a selection that owns no visible geometry (a plain group, or a parent whose only own
shape is hidden). F then frames the objects' manipulator origins as a lone point at the near floor;
the origin is the translate plus rotate-pivot translate, so a rotate pivot point does not move it
(Maya's choice between pivot and matrix translation is undocumented). Pinned by
`exclude_frames_a_shapeless_group_at_its_world_origin`, `exclude_origin_ignores_the_rotate_pivot_point` and
`has_direct_geometry_counts_visible_own_meshes_and_curves_only`. The MCP `frame_selected` tool
follows the same rules and takes `with_children` for Ctrl+F. F and Ctrl+F with
nothing selected in Object or Hierarchy mode frame everything (the View page documents this for
F only; Ctrl+F is PROVISIONAL).

The MCP `fit_factor` argument (viewFit `-fitFactor`, #392, policy row 52) is the only Fit Factor
control: the Frame page documents no menu or preference for it, so no native UI exists. Its range
(0.01 to 1), the sphere fit and the unchanged default margin are PROVISIONAL; `-animate`, `-center`,
`-noChildren` and `-fitClipPlanes` are unsupported.

## Open filter, unsaved-changes prompt and read-only marking follow Maya 2025.1 (PROVISIONAL); Recent Files is missing (#189)

**Severity:** low
**Area:** native document lifecycle
**Issue:** cristim/AztecCraft#189

The Open dialog's first (default) filter is "Maya Scenes" for `*.ma` and `*.mb`, as Maya 2025.1's
`fileDialogFilterTypes.mel` puts "Maya Scenes (*.ma *.mb *.mlt)" first; AztecCraft reads no `.mlt`,
and the per-format filters keep their existing labels, which say how much of each format is
read. The unsaved-changes prompt uses Maya 2025.1's `saveChanges.mel` wording: "Save Changes" and
"Save changes to <path>?" for a scene with a path (Maya formats the full scene name), "Warning:
Scene Not Saved" and "Save changes to untitled scene?" otherwise, with Save, Don't Save and
Cancel. The MCP decision for Don't Save is still `discard`. AztecCraft keeps its own extra lines
for uncertain writes and changed Open sources. The default filter applies where the platform dialog has a filter list: Windows and GTK select
the first filter, the Linux XDG portal leaves the default to its backend (PROVISIONAL), and the
macOS panel merges every filter into its allowed types with no filter choice. In the prompt, Save
is disabled with a reason unless the scene is a directly opened (not symlinked) Maya Binary with a
known destination on Unix or Windows, or a Maya ASCII scene this build saved on Unix (or a write is
being reconciled); Maya's Save would run Save As for an untitled scene, which the prompt's Save As
button (#289) does on Unix (#156, #15). The disabled Save is asserted in the app tests. A read-only
(Maya ASCII) scene shows " (read-only)" in the window title and its reason at the top of the File
menu, both AztecCraft additions with no Maya counterpart. File > Recent Files is not implemented
yet: it needs persisted preferences and an MCP read path.

Researched on 2026-11-12 in the Maya 2027 help: the [Open Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-203FC12C-6C80-497C-AC29-548DF6EC91B5.htm)
page says the "File type" option sets the default format for the next Open and, on Windows and
macOS, the browser filter; it names no default value, so "Maya Scenes" first stays from 2025.1
(PROVISIONAL). [Open recently saved files](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ManagingScenes/files/GUID-699D997C-02C5-4F55-9A3D-6CD68DD0B687.htm)
lists three entry points: the App Home icon, File > Recent Files, and a right-click on the Status
Line Open a Scene icon; the list holds the last saved file and the last file open before a crash,
and its length is set in the Files/Projects preferences. Neither page gives wording for the
unsaved-changes prompt, so that wording stays Maya 2025.1 (PROVISIONAL).

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

The supported legacy 2016 static ASCII curve class (open, nonrational, degree 1 to 3) now has inert native Open,
canonical f64 curve state, evaluated GPU line geometry, conservative framing and
bounded MCP metadata. Source/SDK/UI/renderer tests cover this class; they do not
admit general Maya ASCII, arbitrary units (only mm/cm/m/in/ft, deg/rad, film/ntsc; curves need centimeter and meshes need centimeter/degree/film) or history/drivers, rigs, imported
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
Curve segments are f32 offsets from an f64 origin and the renderer subtracts the f64 eye, so
a small curve far from the origin is no longer refused for the f32 grid of its segments.
The camera target and eye and all non-curve overlays are still world-space f32, so a curve
is refused (Open fails) once the f32 grid at its coordinates exceeds 5% of its size
(an ~8.8cm curve: drawn at x=3e6cm, refused at 1e7cm; any curve past 1e18cm), a deliberate loud limit instead of
an off-screen curve (decided by claude-optimizer; the alternative is an f64 camera). Within
it the curve is exact but overlays (meshes, edges, markers; full f32 matrix) can land up to
about 0.3 NDC from it at the limit with the default Film lens (measured over axes, orbit
poses and aspects; a 120-degree lens measured 0.65), and can be depth-clipped when the
far plane margin is only the near distance (framing sets far to distance+radius+near;
observed with fov 10, small near, or curves over ~1e3cm), less nearer the origin; framing and orbit hold,
but pan and dolly of a curve near the limit are coarse (they move in f32 grid steps). Core and CPU submission
tests cover this; the in-app visual check of a far-origin curve is pending.
[Development notes](.project-docs/development.md#read-only-static-cubic-maya-ascii)
record the supported class, limits and explicitly invoked genuine consumer.

## Existing Maya plug-in compatibility unverified

**Severity:** high
**Area:** scripting and extensions

The Maya scripting/API surface and existing plug-ins aren't implemented or tested in the current slice. Establish architecture/API/ABI boundaries and run existing script and plug-in fixtures against both applications. AztecCraft-specific extensions alone can't prove Maya compatibility.

## First-click text input can be lost

**Severity:** low
**Area:** native text fields

Fixed for the common case: `Editor::render` focuses the name, channel or material field egui already hit-tested for a primary press before the field is shown, so Text, Backspace, Delete and Paste in the press frame reach it. Regression tests: `first_click_with_same_frame_text_reaches_unfocused_channel_field`, `first_click_with_same_frame_backspace_edits_the_field_at_the_end_caret`, `first_click_with_same_frame_delete_removes_the_character_after_the_caret`, `first_click_with_same_frame_paste_inserts_at_the_end_caret`, `first_click_on_the_material_field_receives_same_frame_text`, `first_click_press_release_and_text_in_one_frame_reaches_channel_field`, `first_click_on_another_field_commits_the_valid_draft_before_typing_there`.

Residuals, none verified against Maya (PROVISIONAL):

- A first click while a menu or popup is open still drops the same-frame text (`first_click_text_is_ignored_while_a_popup_is_open`).
- IME Commit on the first click is still lost: egui only routes IME events to a field that already enabled IME.
- Same-frame arrow and Tab keys still move focus with the old focus filter, because egui computes the direction before the press.
- Text typed in the press frame lands at the field's end caret; the press then moves the caret to the click point.
- The manipulation numeric field is pre-focused too, but leaving it does not run the A-first commit, so its draft keeps its existing blur path.
- After a move between name, channel or material fields, egui reports the old field's blur for two frames and the blur commit is skipped for both, which defers the new field's own valid blur in the second frame. The draft is kept and commits on the next blur, Enter or `commit_pending`, so nothing is lost (`release_click_and_tab_after_a_first_click_defer_the_blur_commit_once`: a release click plus Tab in the frame after the click commits the new field's draft once, a frame later). The Diffuse field is disabled while a channel draft exists, so a channel to Diffuse move cannot occur; a click on it leaves focus and text in the channel field (`first_click_on_the_disabled_material_field_keeps_the_channel_draft_and_its_text`).

## Windows binary Save limits

**Severity:** low
**Area:** native document lifecycle

Windows Save and Save As use the same transaction and uncertain-outcome receipt as
Unix, through std path calls checked against the volume serial number and file
index of the opened file and its folder. The following differ from Unix and are
PROVISIONAL until native Save QA on Windows:

- The opened scene's handle stays open, so Windows refuses to rename or move its
  folder (or a parent folder) until another scene is opened or the editor closes.
  A Save As destination folder is likewise pinned for the duration of the save.
- A scene held by another process without delete sharing (some sync or backup
  tools) refuses Save with an access-denied error after the engine's short retry;
  a sharing or lock violation reports busy. Neither is reported as a changed file.
  Retry once the file is released.
- Destination names Windows cannot store as ordinary files (a `:` stream
  separator, device names such as `CON.mb`, a trailing dot or space, wildcards)
  are refused as invalid destinations.
- Active Save refuses a file with the Hidden or System attribute, because the
  replacement cannot carry them; Save As to a new name still works. A read-only
  file refuses Save, as on Unix. Explicit ACL entries and alternate data streams of
  the replaced file are not carried over; the new file inherits its folder's ACL.
- Windows cannot flush a folder without administrator rights. After the rename
  the installed file is flushed instead, which relies on the NTFS journal for the
  name change; this durability claim is unverified.
- Save As relies on hard links for its no-clobber install. Volumes without them
  (FAT32, exFAT) refuse Save As rather than risk replacing a file.
- File identity uses the 64-bit file index. It is unique on NTFS; ReFS 128-bit
  identifiers are not compared in full, and FAT/exFAT derive the index from the
  directory slot, so a recreated folder can reuse it. On FAT/exFAT, Active Save
  replacing a file that another handle has open is expected to fail rather than
  replace it; this is unverified.
- Only symbolic links and junctions are refused as links. Other reparse points
  (OneDrive placeholders, deduplicated or compressed files) are treated as
  ordinary files; saving them is untested (PROVISIONAL).
- Symbolic-link tests need Developer Mode or elevation; without it they print
  `SKIPPED` and pass without exercising links. CI's Windows runners can create
  links.
- App-level tests that simulate an uncertain outcome by moving the scene's folder
  stay Unix-only, because Windows refuses that move; the Windows core tests cover
  the same receipt through injected faults.

## Save of a very large new scene is refused

**Severity:** low
**Area:** native document lifecycle

Saving a new scene as Maya Binary decodes the written bytes to check them. Since #380 the
appearance binder indexes each shape's parent and face count once per decode, instead of
scanning every DAG edge, binding and shape per shape. That scan had capped a new scene at
about 335 default cubes. The remaining bound is the decoder's whole-file work admission
(`admit_work` in `src/native_scene/binary/admission.rs`, quadratic in transforms, DAG edges
and leaves): measured with default cubes in a release build, 1700 save and 1792 refuse.
Save then refuses with the too-large message, writes nothing and leaves the scene open and
unsaved. `a_512_cube_scene_reads_back_through_the_self_check` and
`a_scene_past_the_decoder_work_bound_refuses_as_too_large` pin both sides. Raising the work
bound is a separate change to a check that also guards untrusted files.

## Native MCP idle responsiveness remains unverified

**Severity:** medium
**Area:** native protocol lifecycle

Native testing of the initial nine-tool server reproduced repeated reply deadlines while the window was backgrounded, including a deterministic minimized-window inspection failure. Desktop interaction resumed dispatch. Pinned eframe uses `App::logic` without `App::ui` for hidden windows; the original bridge drained only in UI rendering.

The revised hidden logic path and retained-input guard pass actual trait-callback tests, and [scoped macOS native verification](.project-docs/mcp-native-verification.md) confirmed minimized requests at source `36dffdf`. In the combined icon/MCP build `541c72c`, an idle request reached its UI reply deadline. A main-thread sample taken during investigation included Metal drawable acquisition, without establishing overlap with the timed-out request or a five-second acquisition wait; the next request recovered in the same process and scene token. The underlying cause remains unproven. These results do not establish unconditional idle/background responsiveness or other-platform native behavior.

## macOS Dock Quit, logout and shutdown bypass the unsaved-changes guard

**Severity:** medium
**Area:** native document lifecycle

Cmd-Q, the AztecCraft > Quit menu item, Dock Quit, Quit All from the Apple menu, logout and shutdown all send `terminate:`. `crates/macos-quit-menu` adds an `applicationShouldTerminate:` method to winit's application delegate at startup (additive `class_addMethod`; the launch aborts with a printed error if the selector already exists or the delegate shape changed; winit is pinned to 0.30.13 and `tests/winit_pin.rs` fails if the lock moves). The hook replies "terminate later", asks the app to close the front window through the shared close guard (#11), and answers AppKit with the user's choice, so each of those paths is designed to show the Save/Don't Save/Cancel prompt (native coverage is listed below). The earlier `performClose:` retarget of the Quit item is retired. With no key window the hook picks the window itself, which should stop the Cmd-Q beep (native check pending).

Fallbacks, all deliberate:
- If a terminate is owed and `request_close` is refused for good (an invalid text draft or an active manipulation), the app declines the terminate: a logout or shutdown is aborted instead of hanging, and the user fixes the draft and quits again.
- If a terminate arrives after a viewport close was already sent, the app declines it to avoid a double exit: a logout is cancelled, but the app still exits through the close already in flight, with no data lost.
- While a deferred AppKit reply is still queued, a new `terminate:` is declined instead of begun, so the stale answer cannot cancel a new request (#268).
- A `terminate:` that arrives while an AppKit modal is up (an rfd Open or Save panel runs a nested `runModal` inside the frame, so the reply could never be sent) is declined with a stderr line naming #268, so the app does not hang (#268). Native proof covers a Dock quit with the Open panel up; the Save panel uses the same check and is covered only headlessly. PROVISIONAL (the Maya docs do not cover OS quit with a dialog open): the dialog stays up, and an open panel therefore aborts logout and shutdown.
- Limit: the hook checks only `NSApplication.modalWindow`. Every rfd panel in the tree runs `runModal`, which sets it. Any future nested event loop inside a frame that does not set `modalWindow` would bring the hang back.
- A panic on the main thread while a terminate reply is owed declines the terminate through a chained panic hook, so the panic ends the process instead of hanging in AppKit's wait (#270). PROVISIONAL: the declined OS action (logout or shutdown) is aborted. A panic on a background thread does not answer, because the app keeps running and the user can still finish the quit.
- A panic right after `finish_terminate(true)` has queued an approve reply leaves that reply in flight while the panic unwinds; the hook acts only while a reply is owed (#270).
- Save on logout, restart or shutdown is accepted as aborting the OS action when the prompt cannot complete (owner decision on #261).

SIGTERM (macOS and Linux) starts the same close guard. PROVISIONAL policy: the first SIGTERM prompts, a second SIGTERM before the prompt is resolved exits at once with status 143, and an unattended dirty scene waits for an answer (autosave or recovery is a separate issue). SIGINT and SIGHUP still kill the process directly; routing them through the guard is out of scope for #261. Native results and the remaining checklist are in [native-verification.md](.project-docs/native-verification.md); a real logout, Save, the Quit menu click and Linux SIGTERM are not yet verified natively.

## Bounded World-axis Move candidate

Move currently implements one selected object transform, World X/Y/Z arrows and
one guarded preview transaction through native UI and MCP. Plane/center/component
handles, snapping, other orientations and Rotate/Scale tools are unavailable.
Driven data, unsupported/unstable parents and translations with
unproved retained source words reject editing. MA and general source authoring
remain subject to their existing read-only/source guards. The 72-physical-pixel
geometry and near-parallel threshold are application policies, not Maya parity.

Native/SDK tests for this candidate are in the workspace and mcp-test-client
suites; CI for the current head is the record of their status. Native
framebuffer/desktop and genuine source-bound Save consumer checks remain open.
The prior changed-parent full MCP-client run observed an unchanged transport
notification-flood outer-timeout failure; its single isolated reproduction
passed. The full run remains failed and the timing cause unproved.

## Absent default transform leaves on Maya binary scenes

**Area:** Move tool, Channel Box, `edit_node`, Maya binary save

Maya `.mb` files omit default transform leaves (translate 0, rotate 0, scale 1).
Saving an opened `.mb` now inserts an absent translate `t` leaf (`DBL3`) at a
recorded site, copying the file's own header word and padding and refusing
anything else (grammar and sources: `.project-docs/binary-leaf-insertion.md`).
The insertion is reached by the encoder, `edit_node`, the Channel Box, native
Move, MCP and document save. No AztecCraft-written file containing an inserted record has been opened in Maya; acceptance of inserted records is unproven (#5).
Only AztecCraft's own decoder reopening the output is checked.

Still refused: rotate and scale leaves, which are not part of this slice, and a
translate on a node with no recorded site (the file has no `DBL3` at all, its
`DBL3` header words differ, one has non-zero padding, or the transform's
children do not match the grammar). The refusal differs by entry point. Move,
Rotate and Scale (`begin_manipulation`) return the typed `AbsentChannelLeaf`
error (MCP code `absent_channel_leaf`; for a siteless translate the text says
"leaf insertion is unsupported"). Channel Box, `edit_node` and `set_transform`
edits return `UnsupportedEdit` (MCP `scene_error`, "native ownership or edit
effects are unsupported"). The refusal rate on real Maya files outside the
five-file private corpus is unmeasured.

Source-free scenes (new) use the authored writer for new Maya binary output; OBJ scenes remain unwritable. Setting every inserted word to +0.0 inserts nothing, because that is the
default; -0.0 is a real edit.

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

- a menu-set selector where Modeling and Rigging work; Animation, FX and Rendering are disabled;
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

- F2 and F3 switch Modeling and Rigging; F4 to F6 stay unbound because their sets do not exist.
- No Layer Editor exists, so the toggle shows or hides only the Channel Box; its hover and a Layers stub in the panel say so.
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

Create > Polygon Primitives > Sphere and `create_primitive` with `sphere` build a radius-1,
20x20, Y-axis sphere (382 points, 780 edges, 400 faces: 40 pole triangles and
360 quads). The radius and subdivisions come from the polySphere node reference;
the command page lists radius 0.5, and Maya 2026 defaults are unverified (#5).
Vertex and face order and winding are not claimed to match Maya. Edges are hard,
so the viewport looks faceted. There are no UVs or normals. On Unix, new scenes require
an initial Save As to a fresh Maya binary file; subsequent Save operations persist changes
(see "Mesh Save is PROVISIONAL"). Pole triangles cannot be extruded; body quads can.
The new node is named
`pSphere1` with shape `pSphereShape1`, as in Maya (#175).
The legacy quad-only JSON/OBJ scene refuses spheres with a typed error.

## Poly Torus is provisional and MCP only (#43)

`create_primitive` with `torus` builds a radius-1, section-radius-0.5, 20x20, Y-axis torus
(400 points, 800 edges, 400 quads, no poles). Defaults come from the polyTorus node page (2026 and 2027 pages agree; the command page was read for 2026 only, while reference.md pins 2027.2); the
pages state no ranges or relation between the two radii, so a section radius larger than the
radius is accepted (PROVISIONAL, self-intersecting). `twist` and Create UVs are not
implemented and are not offered. Vertex order and winding are not claimed to match Maya.
Edges are hard, there are no UVs or normals, and persistence follows the sphere. The Create >
Polygon Primitives > Torus row is still disabled; enabling it needs an in-app check.

## World-space Rotate

The ASCII importer reads a node's `inheritsTransform` (`.it`) and `offsetParentMatrix` (`.opm`,
row-vector `matrix`, finite and affine, #105). The view, picking and the handle frames compose
`(it ? parentWorld : I) * opm * matrix` (column form of Maya's `matrix * opm * parentWorld`). That
formula is PROVISIONAL: the `it` off case together with `opm` is inferred from the documented
order and is unconfirmed against a Maya reference scene, so no parity is claimed for it. The binary
importer still refuses `it` and `opm` at open because their on-disk encoding is not confirmed by
any fixture; the tests only pin the refusal. ASCII-sourced scenes are read-only, so nothing edits
these attributes.

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

Create > Polygon Primitives > Cylinder and Cone, and `create_primitive` with `cylinder` or `cone`,
build radius-1, height-2 (y from -1 to 1), Y-axis solids with 20 axis divisions,
1 height division and no cap subdivisions. The cylinder is the sc=0 variant (node-page
default) with 40 points, 60 edges, 22 faces and two true 20-gon caps; the command page
lists sz 1 instead, so a Maya-default cylinder may have different counts, and the
cap topology of both solids is derived, not verified against Maya 2026 (#5). The cone
has 21 points, 40 edges and 21 faces (20 side triangles and a 20-gon base) with its apex
at (0, 1, 0) and its base at y=-1. Vertex and face order and winding are not claimed to
match Maya. Edges are hard, so the viewport looks faceted. There are no UVs or normals.
On Unix, new scenes require an initial Save As; subsequent Save operations persist changes
(see "Mesh Save is PROVISIONAL"). Caps and cone side triangles cannot
be extruded; cylinder side quads can. A pick on any cap triangle selects the whole cap,
and its centre marker is the corner average. New nodes are named `pCylinder1` and
`pCone1` with shapes `pCylinderShape1` and `pConeShape1`, as in Maya (#175). The legacy
quad-only JSON/OBJ scene refuses both with a typed error. No shelf icons exist for them.

## Shape names follow Maya on creation and rename (#175)

Observed in Maya 2025.1; Maya 2026 is unverified (#5), so this is PROVISIONAL. Default-named
primitives get `pCube1` with shape `pCubeShape1`, the two numbered separately. An explicit name
(MCP `create_primitive` `name`) gives `<name>Shape`, even for `pCube7`. Renaming a transform
renames each direct shape whose name is still derived from it: `<old>Shape` becomes `<new>Shape`
and `<oldBase>Shape<digits>` becomes `<newBase>Shape<newDigits>` (`pCube1` to `bar1` gives
`barShape1`); other shape names are kept. AztecCraft numbers both with the lowest free suffix,
while Maya 2025.1 chose `pCube8` when only `pCube5` and `pCube7` existed, so default numbers can
differ after gaps. Default and derived names also skip any name a transform or shape already
has, and a rename keeps the old shape name when the derived one is taken: Maya allows that
duplicate under another parent, but authored saves need unique names. The rename rule is by
name, so for shared shapes (not creatable yet) renaming a copy can rename the shape; Maya was
only observed renaming the original. Legacy JSON scenes keep `<name>Shape`.

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
Degenerate. The limit and the other handle geometry values (rows 15, 16 and 20 of the [policy constants table](.project-docs/development.md#application-policy-constants))
are provisional, not taken from Maya, were not changed
by the UI wiring and use physical pixels (they ignore HiDPI scaling). Maya 2026 back-half
ring behaviour is unverified.

Move arrows and Scale shafts and boxes draw at their full length times the sine of the angle between
the axis and the view ray, so an end-on axis is short and picks exactly where it is drawn
(#155). Plain perspective foreshortening was chosen because a search of the Maya 2026
manipulator help found no documented minimum length, fading or hiding of end-on axes; the
rule is unverified against Maya itself. It is a first-order approximation: exact end-on.
Off-centre it draws short. For a pinhole camera, a small displacement perpendicular to a view ray `a` off
the camera axis, at distance D, shows on screen scaled by 1/(D cos a) out of the ray/axis
plane and 1/(D cos^2 a) in it (checked with a finite-difference projection), against 1/D
for the same displacement at the same distance on-axis. The constant full length (table row 15) times the sine
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

Rotate and Scale each keep their own Object/World selector, as Maya does. The selector,
the axis choice and the numeric field are a row above the viewport standing in for the
unsupported Tool Settings panel. Gimbal, Parent, Normal, Custom and Component are listed
disabled. Move's row has only an Axis Orientation selector fixed at World: Object,
Component, Parent, Normal, Along Rotation Axis, Along Live Object Axis and Custom (the
Maya 2026 Move Tool order) are listed disabled, and Move's other Tool Settings, axis
choice and numeric field are absent. The MCP `update_manipulation` takes an absolute
Rotate angle in the scene angular unit and the native numeric field takes degrees; both reach the core as degrees.

## Primitive creation options are partial and PROVISIONAL (#174)

`create_primitive` accepts size, subdivision and axis (X, Y or Z) options for cube, plane,
sphere, cylinder and cone, and each built Create > Polygon Primitives row has an option box that opens a
"Polygon <name> Options" window (native visual QA pending). Its numeric fields are text-only, without the drag-to-scrub
and sliders Maya's option boxes have (PROVISIONAL gap). Keeping the window open after Create
and using the last created options for plain menu and shelf clicks are PROVISIONAL (Maya's
pages do not say), session only and never saved. On Unix, scenes with created primitives
require an initial Save As; subsequent Save operations persist changes (see "Mesh Save is
PROVISIONAL"). The AztecCraft limits (size 1e-6 to 1e6 cm, subdivisions up to 128,
at most 20,000 faces) are safety bounds,
PROVISIONAL because the Maya pages state no ranges, until the pinned reference install (#5).
Not supported: cylinder and cone cap divisions and round cap, UVs and texture mapping,
interactive creation and Exit on Completion (both are disabled rows in Create > Polygon
Primitives and in the options window; their drawn unchecked state is PROVISIONAL, no default is cited), Free and Camera axis, and editing the options after creation (no
construction history). The plane default is 1x1 with 10x10 subdivisions (#176), PROVISIONAL:
the polyPlane node page lists sw/sh 10 (the command page lists sw 10 and sx/sy 5), and
Maya 2025.1 creates 10x10 through both `polyPlane` and Create > Polygon Primitives > Plane.
Only counts were compared; Maya 2026 is unverified.

## The grid uses Maya 2025.1's queried size and spacing (#160)

Maya 2025.1 `grid -q` in `mayapy` standalone reports size 12, major
lines every 5 units and 5 divisions, matching the Grid Options defaults in the 2024 help;
the 2027 grid command page instead lists 10 and 10 for spacing and divisions (row 28 of the
[policy constants table](.project-docs/development.md#application-policy-constants)). The viewport
draws a line every unit out to +/-12 with major lines at multiples of 5. Reading size as the
half-length is PROVISIONAL, as are the greys: `displayRGBColor` returns nothing headless, so
Maya's grid colours are unverified (its axes may be darker, not brighter, than the lines; the
earlier AztecCraft ordering is kept), and Maya draws the axes bold, which one-pixel lines cannot.
Maya 2026 is unverified. Grid Options (size, spacing, divisions, colours) is a disabled row in
the Display menu, where Maya keeps the Grid option box.

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
session-only lambert table. It is not saved (a new scene refuses Save and Save As with `save_unwritable` while any
session material has been created or assigned, because the file cannot carry it; undo the material to save), sourced binary scenes
refuse it with `unsupported_edit` and Maya ASCII scenes with `read_only_format`; it stays separate
from the saved Phong `set_material_diffuse`. There is no native UI until the Attribute Editor
branch lands. Assignment is whole-shape only: per-face assignment and instance-specific groups
are follow-ups. Ids are table indices reused after undo, so clients must not cache them. The
table holds at most 512 materials, far inside the `inspect_scene` budget (tested); creating past it returns `session_material_limit`. Names follow Maya 2025.1 (#177): the
initial shading group's shader is `standardSurface1`, and a new material is `lambert<N>` with
`lambert<N>SG`, N one above the highest `lambert<N>` any transform, shape, material or group holds
and at least 2 (Maya's unconnected `lambert1` always exists), so undo frees the number and a
transform named `lambert9` makes the next `lambert10`, as observed. Counting `lambert<N>SG` names
is PROVISIONAL. Transform names given to create, rename or `edit_node` are not checked against
material or shading-group names, so a transform can share one (Maya would rename one of them).
The initial material is modelled as a lambert colour, not a standardSurface, its colour is the
viewport-fill placeholder rather than Maya's base colour, Maya 2026 is unverified, and linear
colour is shown as sRGB bytes, so no parity is claimed. No MCP test creates two instances of one shape
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
(PROVISIONAL for Export All: the Maya docs do not say whether hidden instances are
included). Export Selection also filters to displayed instances, so selecting a hidden
object can produce nothing to export (PROVISIONAL: Maya's docs do not describe this
case). In Face, Vertex or Edge selection mode, Export Selection writes the whole
selected object's geometry (PROVISIONAL: Maya's docs do not state how component
selection affects OBJ export). Export Selection covers every selected object and its
descendants, each instance once, in scene order rather than selection order (PROVISIONAL:
the docs give no export order). Maya's options page says the parents of selected nodes are
also exported; a parent's own mesh is not written here unless it is selected, so that is
unmet. Normals, materials, smoothing groups, curves and Maya's OBJ Export Options are not
written or offered, and a shape with session UVs is refused rather than exported without them (the option boxes are disabled), and export never overwrites an
existing file. The view of the whole scene is built first, so one mesh the viewport
cannot display (for example a concave n-gon) fails every export, including Export
Selection of another object. The Windows save dialog may first ask whether to
replace an existing file; confirming still fails with "destination already exists"
and the file is left untouched. The output has not been compared with
Maya's objExport plug-in; OBJ import is unsupported. An export with no faces at all is refused
(`nothing_to_export`); in a mixed export an instance without faces is still written as an `o`
group with only its vertices and counted in `objects` (PROVISIONAL: vertex-only objects are
valid OBJ, and the Maya docs do not say what objExport writes for a mesh without faces).

## Authored Maya ASCII: only the AztecCraft-written subset reopens

**Severity:** medium
**Area:** file format
**Issue:** cristim/AztecCraft#156

Save As `.ma` (macOS and Linux only) writes the authored-scene subset: one transform over one mesh per
object, centimeter/degree/film units, no UVs, no session materials, no curves. Reopening admits a mesh
`.ma` only when the file is byte-identical to what the writer would emit for the scene it decodes to
(`src/native_scene/ascii/document.rs`). That makes the reader exactly as wide as the writer: a `.ma`
with meshes written by Maya, hand-edited, reformatted or saved by another tool is refused, not
approximated, and Maya-authored mesh files (UVs, shading connections, smooth edges, history) stay
unsupported. Windows refuses mesh `.ma` files at Open and has no authored ASCII Save. Maya acceptance of
the written files is PROVISIONAL and unproven. A reopened file is an authored document: Save rewrites it,
and any edit the writer cannot carry (session material, UVs, curves) refuses Save and keeps the scene
dirty.

Because the gate compares against the writer's exact output (including the fixed header text), any
change to the ASCII writer invalidates `.ma` files saved by earlier builds: a mesh file written before
the change is refused at Open by the later build (a file with no meshes opens read-only). Writer changes
therefore need either a version marker the reader can branch on or an accepted break; neither exists yet.

Reopen trigger: Maya-written mesh `.ma` support (a general mesh reader), UV or material writing, or a
public Maya acceptance check (issue #5).

## Authored Maya Binary writer: unproven profile

**Severity:** medium
**Area:** file format
**Issue:** cristim/AztecCraft#156

The fresh `.mb` writer (`src/native_scene/binary/write.rs`) takes its profile (`VERS` and the aux word
of each chunk kind) from one public Maya 2025 file and emits groups shorter than any Maya wrote. Files
it produces are Maya 2025-format; that Maya 2026 opens them, and that Maya accepts them at all, is
PROVISIONAL and unproven. AztecCraft's own decoder provides read-back checks; the optional MayaSceneKit cross-check adds an independent reader but does not establish Maya acceptance. Saved meshes
will carry no shading assignment. The invented items are the I rows of
`.project-docs/binary-authored-writer.md`.

Reopen trigger: a public, licensed Maya 2026 file with a mesh becomes available (re-derive the
profile from it), or Maya rejects, repairs or rewrites these files (issue #5 asks for a read-back).

## Hierarchy delete: Maya semantics are provisional

**Severity:** low
**Area:** editor core
**Issue:** cristim/AztecCraft#187

Delete removes a node with all its descendants, root or not, natively and over MCP; only a
descendant transform shared with a node outside the subtree is refused (`scene_error`). Maya's exact
Delete semantics for hierarchies are PROVISIONAL: only the 2024 Delete help page was found, no
2026 page exists, and none states what a parent delete does to its children. A session joint is deleted with everything below it like any other
node (#363).

## Joints are session-only with PROVISIONAL display and defaults

**Severity:** medium
**Area:** rigging
**Issue:** cristim/AztecCraft#363

Joints exist only in authored, unsourced scenes and are never saved: both writers refuse a scene
holding joints rather than writing them as plain transforms, so such a scene cannot be Saved until
they are deleted. A joint's parent must be a joint, so a joint cannot sit under a mesh transform.
Create Joints is a numeric window, not Maya's click-driven Joint Tool; there is no orientation,
jointOrient, rotate axis, rotate order, IK, skinning or constraint.

PROVISIONAL, because the cited 2027 pages do not settle them: the default name `joint<N>`; the
display (circle radius 0.5 times the radius, 16 segments, one line per bone, existing object
colours); default radius 1.0 at creation and the 1e6 cm upper limit on radius; an inverse of 1 for a zero or non-finite parent scale;
Delete removing descendants; a child rotation of 0 under a rotated parent; the Rigging menu order;
the undo label Create; the 8 pixel click tolerance on joint centres, with bones not pickable; the
Radius row in the Channel Box. None of this has been compared with Maya.

## Naming the refusing member of a multi-object Delete costs N to 2N checks

**Severity:** low
**Area:** editor core
**Issue:** cristim/AztecCraft#336

A refused Delete of N selected objects names the one member whose absence clears the refusal. A
scene-wide pending-insertion refusal names nobody and skips the analysis. Otherwise the refusal path
decodes the source once, then checks each member alone and, only for a member that fails alone with
the combined refusal, the set without it. That is N checks plus one per such member: N+1 when one
member explains the refusal, up to 2N when many members fail alone but none clears it. The checks
run one at a time in one reused id buffer (O(N) memory) and stop at a second qualifying member. On
an opened binary scene each check scans the source text, up to 16 MiB, and they run synchronously,
so a refused Delete of many objects in a large file can still briefly stall the UI. A successful
Delete pays nothing extra. Not capped: no member count is documented as the point where naming
stops being useful.

## Object marquee is a provisional subset

**Severity:** medium
**Area:** viewport input
**Issue:** cristim/AztecCraft#30, cristim/AztecCraft#168

A primary drag in Object or Hierarchy mode selects objects by rectangle (`select_in_rect`, policy
rows 34 to 38 in [development.md](.project-docs/development.md#application-policy-constants)). It
is a subset of Maya's marquee and every choice below is PROVISIONAL and unverified against Maya:

- Objects hidden behind another object inside the rectangle are selected in every shading mode,
  which is believed to match Maya's object marquee: camera-based selection is documented for
  components only ([Select objects or components](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-5C5F2B9B-BE16-4C49-B72B-70EA7CB4C688.htm)
  and the Modeling Toolkit selection options), so occlusion filtering belongs to the component
  marquee slice. PROVISIONAL, not checked against Maya. Pinned by
  `marquee_selects_an_occluded_object_in_every_shading_mode`.
- Any overlap of a visible triangle selects the object (the docs are silent on partly covered
  objects); there is no backface culling.
- Shift toggles, Ctrl or Command removes, Shift+Ctrl adds; the Marquee page is silent on Shift.
- Hits are ordered by instance order and the last hit leads, so Move's pivot follows the last hit.
- Hardware Ctrl-drag on macOS is an AppKit secondary-button drag and never selects; Command-drag
  does. Not checked by tool.
- Component modes (Face, Vertex, Edge) have no marquee: the drag shows a status and changes nothing.
  Hidden instances and curves cannot be marquee-selected. Lasso, Paint Selection, the Drag style,
  Tab and Alt-repositioning are absent (#69, #30).
- The five #168 drag cases now read: a stationary press held longer than 0.8 s that egui counts as a
  drag is a zero-area rectangle and picks like a click (nearest hit); releasing Alt before the drag
  threshold shows nothing (no Orbit and no rectangle); a fast flick that coalesces into one frame is
  not a drag and does nothing; Alt held only on the release event is not a marquee; a release over
  another panel selects with the end clamped to the viewport edge. Esc during a drag cancels it.
  #168 stays open until each of these is checked against Maya.

## Disabled Create menu rows expose their reason by hover only

Create > Polygon Primitives shows the reason on its 12 disabled rows (unbuilt primitives and the two
interactive toggles) as an egui hover tooltip. egui does not copy `on_disabled_hover_text` into the
AccessKit node and disabled widgets cannot take keyboard focus, so keyboard and screen-reader users never
get the reason. Edit > Delete by Type and Delete All by Type share this limitation.

## File and Redo hotkeys use Command on macOS without a documented glyph

Ctrl/Cmd N, O, S, Shift+S and Y follow the Maya hotkeys page, which shows the macOS alternate modifier
as an image the documentation fetch could not read, so Command on macOS is PROVISIONAL. Held file
chords no longer repeat; Cmd+S used to. Open Scene and Save Scene As have no trailing ellipsis because the
docs give bare names (PROVISIONAL).

Held Ctrl/Cmd Z and Ctrl/Cmd Y keep repeating Undo and Redo (egui reports OS key repeat), capped at
one step per frame, so the rate is frame-rate bound. Keys that ignore repeat: the file chords N, O, S
and Shift+S (menus.rs:128), Q/W/E/R (shortcuts.rs:26), 4/5/6/7 (shortcuts.rs:44), Alt+Home
(shortcuts.rs:74), A (shortcuts.rs:95) and Ctrl+A (shortcuts.rs:108). Keys that repeat besides Undo
and Redo: the manipulator size keys +, = and - (shortcuts.rs:52), F (shortcuts.rs:83), Escape
(shortcuts.rs:192, idempotent), and F8/F9/F10/F11 and Delete (panels.rs:48-62, no repeat filter). Not
reviewed against Maya: a held Delete or F8-F11 repeats. The Maya 2027 hotkeys page lists Ctrl z and Ctrl y
(https://help.autodesk.com/cloudhelp/2027/ENU/Maya-KeyboardShortcuts/files/GUID-30CACC9D-8FBE-4B85-8A8F-C5ADF32DDD4E.htm)
but does not say whether a held key repeats, so the repeat is PROVISIONAL and no Maya parity is claimed.
It is the behaviour before #283; a test pins it. File chords ignore repeat to avoid a second dialog or
New, which does not apply to history steps.

## Default View pose and view history are unverified

The Default View row and Alt+Home restore AztecCraft's own default camera (row 24 of the [policy constants table](.project-docs/development.md#application-policy-constants)). Maya documents no default perspective pose, so the pose is not
compared with Maya: PROVISIONAL. Default View restores only the pose (target, distance, yaw,
pitch); the far clip plane, which framing can grow, is not reset. Fn+Option+Left as Alt+Home on compact Mac keyboards is unchecked.
Maya's Undo/Redo View Change, the `[` and `]` view history and View > Bookmarks are not
implemented; camera changes create no undo entries (see Core authoring limits).

Set Camera Pose and `set_camera_pose` judge Changed/Unchanged numerically, so -0.0 over 0.0 (target, yaw or
pitch) replies Unchanged while the stored sign bits flip; the view renders identically and inspect may show
-0. Move-handle drag capture compares bits (`same_capture`), so the flip can only cancel a live drag
conservatively, and the human-input guard keeps remote camera operations out of a held drag.

## Manipulator highlight stays painted while the hover status is suppressed

**Severity:** low
**Area:** viewport manipulators, MCP inspection
**Issue:** cristim/AztecCraft#306

The Rotate and Scale hover status text is gated by the hover admission (no modal, not closing, window
focused, pointer inside, no egui popup, renderer healthy), but the handle highlight is painted from the
raw pointer every frame. With the window unfocused, a popup open or a file modal up, the handle under
the cursor stays highlighted while the status reverts to the tool help. `inspect_scene`
`pointer_hit`, `pointer_disabled` and `pointer_refusal` follow the highlight, so they keep reporting
the pick in those states (pinned by `inspection_reports_the_pick_while_native_hover_status_is_suppressed`).
They are null in other states too (for example, `viewport` is null when the window is hidden or
minimized, in component selection modes and after a renderer terminal error; it is also null with no
valid capture, after a scene load and after a camera change, the last two until the next viewport
capture; the pointer fields are null when the pointer has left the window, a manipulation is active
or Select is the tool). `pointer_refusal` includes the refusals raised when a Rotate or Scale drag
begins, for example an edge-on Rotate ring, a ring plane behind the camera along the press ray, or a
Scale grab whose along-axis component is under 1 px of world length, as the press shows them; native
hover shows the same text. Not fixed: gating the highlight
changes native behaviour and needs its own issue; the admission is an inline expression that needs the egui context for the popup check.

## A pointer capture can survive a frame that skips the view build

**Severity:** low
**Area:** viewport manipulators, MCP inspection

A view build error now clears the pointer capture (`missing_move_viewport`, #310). An earlier error
in the same frame returns before the view build, so the paint is skipped and the capture survives
for that one frame: the selection refusal in `src/app/panels.rs` and a `commit_pending` failure.

## Mesh Save is PROVISIONAL: Maya acceptance unproven, no UVs

**Severity:** medium
**Area:** file format, Save As
**Issue:** cristim/AztecCraft#156

On Unix, a new (Untitled) scene requires an initial Save As, which writes a fresh Maya
2025-format `.mb` through the writer above. Subsequent Save operations persist changes
to that file. Saving Untitled scenes is unsupported on Windows.
Meshes are written with an empty UV set (0 points, 0 ids) and no shading assignment. Autodesk
documents "No UVs" as a primitive option and an empty UV set as a mesh state (citations in
`.project-docs/binary-authored-writer.md`) but not what a file stores for either, so Maya
acceptance is unproven. MayaSceneKit (a third-party reader, not Maya; pinned commit 95a11db) decodes the Maya-written
`tests/fixtures/gt-tools/dummy_mesh.mb` (no MESH issue, an exact DMSH/MESH decode) but reports the
writer's cube MESH/DMSH as unsupported ("no decoder registered for chunk tag"). The empty UV set is the
INFERRED cause, from the reader's mesh decoder source (it requires a nonzero UV count) plus that
contrast; no probe that adds UVs has been run. The ignored test `msk_decodes_maya_meshes_but_not_the_writers_mesh`
(`AZTECCRAFT_MSK_BIN`, see `.project-docs/mayascenekit-crosscheck.md`) pins the contrast so a change
becomes visible. The
`save_scene_as` MCP description carries the same PROVISIONAL note.

Reopen triggers: Maya rejects, repairs or rewrites a saved file (issue #5 asks for a read-back);
MayaSceneKit starts decoding these meshes (the cross-check test flips); a public, licensed Maya
file shows a mesh without a UV block or with default primitive UVs.

## Multi-object selection colours and group descendants are PROVISIONAL

With several objects selected, the lead and the other members are drawn in our own colours
(`.project-docs/development.md` row 33); Maya's page says only "a different colour". The
renderer matches by instance-path prefix, so an object below the lead's path is drawn in the lead
colour and an object below another member in the member colour, whichever member is listed
first. Whether Maya draws descendants of a selected group that way is unconfirmed. Pinned by
`descendants_take_the_colour_of_the_selected_ancestor_they_sit_below`.

Slice 1 of #166 gives an ordered selection with a lead (Shift, Ctrl and Shift+Ctrl viewport clicks
in Object and Hierarchy modes, Shift and Ctrl Outliner clicks, MCP `modify_selection`). It acts on
several objects for Move only (native drag, and MCP `begin_manipulation` with `expected_selection_generation`, #325 slice B);
Rotate, Scale and Channel Box edits still refuse with `multiple_selection` until #325 slices C and D (the by-id MCP tools `delete_node`,
`rename_node`, `set_transform` and `edit_node` still act on the node they name), and the marquee
selects objects only (#30, see "Object marquee is a provisional subset"). The native Delete (menu, Channel Box and key) acts on all selected objects, all or nothing, and Frame Selected frames their union (Select All, Hierarchy and Inverse exist as native UI rows and MCP `select_by_rule`). These choices are PROVISIONAL because the Maya pages are ambiguous: a viewport
Shift click toggles the clicked instance (the page says "the last selection"); a Ctrl click on
an unselected object does nothing in the viewport but selects it in the Outliner, where the page
pairs Shift with adding and Ctrl with toggling; Shift wins over Ctrl in the Outliner; Command
counts as Ctrl. On macOS a hardware Ctrl-click reaches the app as an ordinary Ctrl-click
(maintainer check, 2026-11-04: it removes a selected object in the viewport), while a secondary
(right) click with Ctrl held is ignored; a lone
selection keeps the old colour; the Outliner lead row is bold. Native checks on 2026-11-04
confirmed the viewport Shift, Ctrl and Command clicks, the Outliner Ctrl click and the lead
colour; whether they match Maya stays PROVISIONAL. Multi-selection is not an undo entry, and a Create, Select, New or Open replaces the
whole set.

## Ambiguous selection is unreachable today

`OperationError::AmbiguousSelection` (an object with several instance paths selected without a
path) cannot occur in a running editor: no decoder or runtime operation gives a transform two
parents, so every node has one path. Only the unit test
`an_object_with_two_instance_paths_needs_an_explicit_path`, which fabricates the targets,
covers `only_instance_path`; there is no end-to-end test through a loaded scene.

## A very large selection lowers the usable inspect_scene node_limit

Every `inspect_scene` page repeats the full `selection` list, about 40 bytes per member. With 4096
selected default-named cubes a page of 1 node is about 170 KB and 50 nodes about 212 KB, but 250 nodes
exceed the reply cap, so such a selection needs a small `node_limit` (about 50). Select All (native, and
MCP `select_by_rule`, #325) builds a whole-scene selection in one call. A client that connects fresh
inspects with a small `node_limit` first to obtain the token, then clears the selection with
`deselect_all` if it wants larger pages. Only a selection whose serialized members approach the reply
cap on their own (for example very deep instance paths) blocks every page; that case has no
tokenless recovery except clearing the selection in the native UI (a plain click on empty space, or
Alt+D). Pinned by `a_full_selection_inspects_with_a_small_node_limit` and
`a_full_selection_is_inspectable_with_a_small_node_limit`.

## Creating N objects costs O(N^2)

Each `NativeScene::create` runs a full `validate()` through `structural_edit`, so building N cubes one
by one is O(N^2). `validate_dag` itself is linear in the edges since #345: it looks up transforms and
each parent's children through the `NativeScene::dag_index` that `transform_paths` also uses (#347),
and builds one identity-has-an-edge set, instead of scanning `self.dag` for every identity and every
pending parent. In a release build one `validate()` measured about 9x for 8x the scene (512 to 4096
cubes) against 45x before. `validate_indexes_the_dag_once_however_many_identities` pins one index
build per validation; it does not catch a new per-identity scan added beside the index. The remaining
quadratic factor is the per-create full `validate()`, not the DAG walk. Tests that need a large scene
still build it through `azteccraft::scene::Scene::create` and `NativeScene::from_legacy`.

## Paged inspect cost and the decision not to cache transform paths

After #340 `NativeScene::transform_paths` indexes nodes and children once per call, so one
`inspect_scene` page costs about 2.6 ms at the 4096-node cap in a release build (0.6 ms of it
`transform_paths`) and a 21-page walk about 75 ms; it was 40 ms per page and 859 ms per walk. Nothing is
cached per `History::revision()`: the cache would need invalidation on every mutation, load and undo for
about 1 ms. Revisit if a measurement shows `transform_paths` above about 5 ms at the 4096 cap.
The regression guard of record is `page_cost_timing_guard`, a wall-clock check on the median of 5
interleaved ratios between 512 and 4096 nodes (limits 30x for `transform_paths`, 25x for one page; the
per-edge scans measured 57x to 72x). It is `#[ignore]`d and runs alone as a single-threaded step of the
`check` job, in a release build (about 80 s more compile than debug locally, and more stable timings),
because timing ratios flake under parallel load.

## A view build still looks up each transform by linear scan

Since #341 a view build indexes instance admission once (`ViewAdmissionIndex`), and
`a_view_build_indexes_admission_once_for_every_instance` pins one index build per view. The build
still calls `NativeScene::internal_transform` for every transform path record
(`src/view_scene.rs`), and that resolves the node through `NativeScene::node`, a linear
`nodes.iter().find`. So the view build remains O(T^2) in transforms. Measured release ratios for
16x the scene (256 to 4096 cubes) were about 26x after #341 against 65x before; the remaining
superlinear part is this lookup. Not fixed here; the build-count pin does not cover it.

## Frame origin for a mixed selection is PROVISIONAL

F (frame without descendants, #338) frames the objects' origins when no selected object owns visible
geometry. The Maya documentation does not state this, so the choice is provisional and pinned only by
tests: the origin is the node's translate plus its rotate-pivot translate and ignores the rotate pivot
point (`exclude_origin_includes_the_rotate_pivot_translate`, `exclude_origin_ignores_the_rotate_pivot_point`),
and a selection where any one member owns geometry frames the geometry only
(`exclude_with_one_member_owning_geometry_ignores_origins`). Revisit when the behaviour can be compared
with Maya 2027.

## A failed Move finish or cancel leaves the preview in place

`History::finish_move` and `cancel_active_move` take the active transaction before computing
`changes()?` and restoring the baselines. If that step fails (a baseline whose node or source no longer
matches), the error is returned, the transaction is already cleared and the preview values stay in the
scene; nothing reverses them. Pre-existing for single-object Move, Rotate and Scale. For a multi-object
Move the preview stays whole and no member is restored, because `changes()?` fails before any restore
and `restore_channels_many` checks every owner before its first write. The Channel Box is re-synced so
it shows the actual values; the scene is reconciled by inspection. Pinned for the multi case by
`a_failed_restore_is_typed_clears_the_transaction_and_writes_nothing_more`.

`NativeScene::edit_nodes` and `check_effects` scan every member against every node at commit, which is
O(members x nodes). It runs once per drag, and is recorded next to the begin cost of a 4096-member Move
(about 0.7 s in a debug build).

## Move of several selected objects: PROVISIONAL choices (#325 slice B)

Researched on 2026-11-03 in the Maya 2027 help: [Move objects and components](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-ABF1A141-7860-4234-889B-00278F20B9A3.htm),
[Move Tool settings](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9BCE8B23-6DA6-4C7F-B897-7E25D6E8A865.htm),
[Transform objects and components](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9622730D-3D21-451C-8BEE-E01BCC979F91.htm) and
[Move, rotate and scale using precise values](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-EB585085-9C14-444F-8ACD-ECB087286FDF.htm).
The pages say little about several objects, so these choices are pinned by tests, not by Maya:

- P18: the manipulator is drawn and hit-tested at the lead (last selected) object's pivot only (`the_press_frame_is_the_leads_even_when_the_first_member_is_elsewhere`).
- P19: a World-axis translation is pivot independent, so every moved member shifts by the same world displacement (`several_movers_preview_commit_one_labelled_undo_entry_and_undo_redo_bit_identically`).
- P20: a selected parent and descendant move the descendant once: it follows its ancestor and gets no translation of its own. Ancestry is decided by node identity, not path prefix (`a_parent_and_child_both_selected_move_the_child_once`, `move_instanced_ancestor_covers_descendant_by_identity`).
- P21: one node selected through two instances is refused whole, naming the node, because its instances share channels; the same target twice is `InvalidIds` (`move_refuses_two_instances_of_one_node`, `the_same_target_twice_is_invalid_ids`).
- P22: one undo entry, labelled Move, for the whole set (`several_movers_preview_commit_one_labelled_undo_entry_and_undo_redo_bit_identically`).
- P23: MCP `update_manipulation distance` applies the same World displacement to every member (`sdk_move_with_the_current_generation_moves_every_selected_object_in_one_undo_step`).
- P24: not done: Object and Parent axis orientations (each object moving along its own axes) need a Move orientation setting, which does not exist; only World is available.
- P25: when the lead is covered by a selected ancestor, the pivot is still the lead's own origin and the inspection reports the lead's own channels (`the_lead_may_be_the_covered_member`).
- P27 (#39): Move and Rotate handles are drawn and dragged at the rotate pivot (T + RT + RP in the parent's space), and the Scale handle at the scale fixed point (the full matrix applied to the scale pivot). Maya's pages do not place the handles relative to the pivots (`handle_origin_is_the_rotate_pivot_for_move_and_rotate_and_the_scale_fixed_point_for_scale`, `the_rotate_ring_is_picked_at_the_rotate_pivot_and_not_at_the_translate`).
- P28 (#39): Frame Selected keeps using T + RT and ignores the rotate pivot (see the frame-selected entry above); Move, Rotate and Scale channel edits never write the pivots (`move_rotate_and_scale_patch_only_their_words_through_save_save_as_and_reopen`).
- P29 (#39): pivots with rotatePivotTranslate, scalePivotTranslate or shear are covered by in-memory test fixtures only; a `.mb` carrying those keys does not import until #105, and nothing edits, creates or saves pivots until #39 slice B (an authored scene with a pivot is refused with NonDefaultPivot).
- P26: an uncovered selected object that a selected ancestor also carries through another instance path is refused whole with `AlternatePathAncestor` (see "Move: a selected ancestor on another instance path of an uncovered object is refused" below; `an_uncovered_mover_with_a_selected_ancestor_on_another_path_is_refused`).

Costs: beginning a Move of 4096 objects takes about 0.7 s in a debug build and one update about 45 ms
(`four_thousand_members_begin_within_the_budget_and_not_one_byte_less`); the inspection checks Move availability over every member once per request.

## UVs are session-only and planar (#360)

UVs exist only on shapes authored in the session, as one `map1` holding one UV per outer face corner,
so no two faces share a UV. They are not saved: authored writers and the legacy view refuse a scene
holding them, rather than dropping them, and OBJ export refuses a shape that has them
(`export_invalid`, remedy Delete UVs) instead of writing it without `vt` records. A shape with UVs refuses Extrude until Delete UVs or Undo,
because UVs do not follow topology edits. Planar projection covers the whole shape in object space with
bounds of the face corners (Bounding box fit, PROVISIONAL; points no face uses do not affect the fit) only; Best plane, Camera, Keep image ratio, Insert before deformers, Create new UV
Set, the projection manipulator and construction history are missing. The projections are oriented
as Maya's Y-up side, top and front views (X: u=-z, v=y; Y: u=x, v=-z; Z: u=x, v=y), view-matching and
right-handed, so a face looking along +axis has positive UV area. Maya's pages state no per-axis
orientation, so this and the Y default are PROVISIONAL. The earlier X and Y mappings were mirrored
(fixed, #394). Scenes opened from a file do not expose their UVs, which does not
mean they have none. The UV Editor is a read-only outline, not drawn with a texture, and the viewport
does not show UVs. The native menu and both windows have automated UI tests but no reviewed native
screenshot yet, and nothing has been compared with Maya (#5).

## Delete on an opened Maya Binary

**Severity:** medium
**Area:** native scene, Maya Binary save
**Issue:** cristim/AztecCraft#272

Delete removes a transform, all its descendant transforms and their shapes (root or not) from an
opened `.mb`, and Save writes the file without their records. A shape that hangs under a surviving
transform stays. Maya's own handling of children on delete is not stated in the documentation
(PROVISIONAL). Maya's acceptance of the pruned file is unproven (#5), so the
behaviour is PROVISIONAL. The refusals and limits that remain, each a typed `unsupported_delete`
reason except where noted:

- Not an `unsupported_delete` reason: a descendant transform that also has a parent outside the
  subtree returns `scene_error` (an opened `.mb` cannot produce one); so does any node of an
  opened ASCII scene.
- Fail closed on records. Every surviving leaf must be a kind the analysis lists: modeled by the
  decoder (`CREA`, `SLCT`, `CWFL`, `RELA`), a typed attribute payload (`DBLE`, `DBL2`, `DBL3`,
  `FLT2`, `FLT3`, `FLGS`, `I32#`, `MESH`, `CMP#`, `CMPD`, `ATTR`), or text that is scanned for the
  removed names (`STR `, `STR#`, the header tags `AUNI`, `LUNI`, `TUNI`, `BCSZ`, `CHNG`, `DATS`,
  `FINF`, `ICON`, `INCL`, `INFO`, `MADE`, `OBJN`, `PLUG` (the plug-in requirements, present in
  nearly every real file), `SMTD`, `TDUR`, `UVER`, `VERS`, and every leaf
  of a `SCRP` group). A typed attribute payload is skipped only inside a bound transform or shape
  group, whose layout the opener validated; in any other group it is scanned like text, so a few
  false refusals are possible there. Any other tag, anywhere in the file, refuses the delete as "a
  '<tag>' record that cannot be checked", even when it names nothing. A real file with other tags
  (for example further typed-attribute tags) is not deletable until each tag is added with a reason.
  The vendored `file_owner_delete.mb` uses only listed tags; its four default cameras are
  refused because each has a camera shape (`DCAM`) parented under it (the names `persp`, `top`,
  `front` and `side` also occur in its text, so the scan would refuse them too). `dummy_mesh.mb`
  does not open.
- Instancing. The opener builds one parent edge per binding, so a shape with several parents
  cannot be read from the decoder. An extra instance parent would have to be a record naming
  the shape: an unknown tag is refused by the rule above (tested for a record naming the shape
  of the deleted parent), and a selection, relation, connection or scanned text that names it is
  refused as a reference. Nothing more is claimed.
- A record that names a removed node as parent, such as a camera shape under a removed group; a selection, relation or connection that names a
  removed node (by `|` or `:` path component); per-face material links (group id and group colour),
  which every recognised shaded shape carries, so only unshaded shapes and whole-object next-available
  set members can be deleted today.
- A mode-1 set link with an index, a surviving explicit-index link beside a dropped mode-1 link, a
  surviving mode-1 link beside a dropped mode-0 link, any `dagSetMembers` alias into the set, and a
  set that is neither created in the file nor `:`-rooted.
- A shape that is the first or only user of a material or the only user of a diffuse-bound shader
  (reached only after the per-face rule is lifted; pinned by direct tests).
- A removed range that would change an enclosing group's padding, and an edit that needs a leaf
  inserted, before or after a Delete (the edit fails with `scene_error`).
- Text scanning is bounded to 16 Mi (bytes times removed names), about 0.25 s in a debug build;
  a larger scan is refused as "too large to check" rather than skipped, so a file with 200 KB of
  script text refuses once more than about 80 names are removed. Free text in a listed kind that
  holds a removed name (for example a `TEXT` leaf in a `SCRP` or an `STR` in any group) is refused.
- A multi-delete is analysed once and is all or nothing. Undo restores the nodes; Save keeps the
  scene bound to the original bytes, so repeat saves are identical.

## Move: a selected ancestor on another instance path of an uncovered object is refused (PROVISIONAL, #325 slice B)

A node's translation is shared by all its instances. When an uncovered selected object (no selected
ancestor on its own selected path) is also reachable under a selected node through another path, a
Move would apply its own translation and its ancestor's, so that instance would move twice.
`begin` refuses the whole Move with `AlternatePathAncestor` (MCP code `alternate_path_ancestor`),
naming the object. A member that is covered through its own path is not refused, because it gets no
translation of its own (`move_instanced_ancestor_covers_descendant_by_identity`; refusal:
`an_uncovered_mover_with_a_selected_ancestor_on_another_path_is_refused`). Maya's behaviour for
instanced selections is not documented, so this is a conservative choice.

Cost: a multi-object begin enumerates every transform path of the scene once (`transform_paths()`, linear since #347 and bounded by the path limit, so an oversized scene is refused with `ResourceLimit` rather than scanned). Begin does not hold that list otherwise, so it is not reused; the scan is a few milliseconds at 4096 paths in a debug build and runs once per begin, not per update.

## Joint creation: untyped name and parent errors, and edit_node positions (#363)

- `create_joint` through MCP reports an invalid name and an unknown parent id as the generic
  `scene_error`, as the other node tools do on `main`; only a joint-specific refusal (parent not a
  joint, singular parent, radius, position range) is `unsupported_joint`. Typing those two is
  deferred with the same gap in the other tools.
- `create_joint` refuses a position that is not representable as f32 (`unsupported_joint`), because
  one such joint would fail every view build. `edit_node` still accepts translations beyond f32
  range on `main` (code outside the rigging slice), with the same effect on the view build; it is
  not changed here.
- Undo and redo do not restore the object selection: undoing a joint creation leaves nothing
  selected and redo does not reselect the joints, and undoing a delete does not reselect the
  deleted chain. Maya's selection undo behaviour is not documented for these operations
  (PROVISIONAL); the redo tests pin the current behaviour.

## Pivot editing choices and gaps (#39 slice B)

Researched on 2026-11-09 in the Maya 2027 help ([xform](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/xform.html), [Center the pivot point](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-7F4D1D6F-C2CE-4E31-B82E-FDFA9D492219.htm), [Modify menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-378D7FF6-492F-4DA4-A468-507E6ABF8B58.htm)). The pages do not state these choices, so tests pin them:

- P-B1: Center Pivot uses the bounding box of the node's whole hierarchy in the node's own Object space, not a world-aligned box.
- P-B2: a preserving pivot edit compensates through the rotate and scale pivot translates, as `xform -preserve` documents; the exact placement is not stated.
- P-B3: Zero Pivot follows `xform -zeroTransformPivots`: pivots and pivot translates go to zero and the matrix is kept by moving the compensation into the translate.
- P-B4: what an Attribute Editor field edit does (preserve or raw) is undocumented; `set_pivots` defaults to preserve like `xform`.
- P-B5: Object (local) space only; World space has no tool.
- P-B6: the undo entry is labelled Pivot.
- P-B7 (PLANNED, T10; not implemented yet): the leaf order of authored `.mb` rotate and scale pivots will be invented from the corpus.
- P-B8: pivot edits on a node with any session key are refused (`pivot_on_keyed_node`), because the next time change would overwrite the compensation.
- P-B9: curve control points are in the Center Pivot box, proved-invisible geometry is not, and unproved visibility refuses Center Pivot (`scene_error`); a hierarchy without geometry is unchanged.

Gaps: `edit_node` and `set_transform` report the same `ReadOnlyFormat`, `UnsupportedEdit` and `InsertionAfterDelete` refusals as the generic `scene_error`, while the pivot tools give them their own codes; the older tools are unchanged. Center Pivot on a selected shape node is unreachable (the editor selects transforms only).

## Save with a typed pivot draft commits the draft, then refuses (until T9/T10, #39)

Save, Save As and Resolve(save) first commit a pending Attribute Editor pivot draft, as for any draft. Until the writers support nondefault pivots (T9 `.ma`, T10 `.mb`) the save then fails with "nondefault pivots are unsupported" or "a transform has a non-zero pivot". The scene keeps the committed pivot and its undo entry and stays unsaved. Pinned by the `pivot_draft` cells of `tests/snapshots/guard_matrix_file_writing.txt`; T9/T10 flip them.
