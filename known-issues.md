# Known limitations

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
- An end-on Scale axis keeps its 5 px tip box next to the origin (about 1.4 px away in the test
  camera), so it wins pixels just outside the 7 px centre box, out to about 16 px from the origin at
  the 8 px radius. This pre-existing defect is tracked in #297.
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

Undo keeps the newest 50 NativeScene snapshots and silently drops older ones. The 50 matches
the Maya 2027 Undo preferences default; the fixed size (no Infinite choice) does not, and
the evidence is in the [policy constants table](.project-docs/development.md#application-policy-constants) (row 21; configurable limit pending #178). Redo is
unchanged. The dirty state compares the live scene with the saved scene by value,
so dropping old entries cannot make an edited scene read clean. Memory still
scales with scene size times 50. Autosave and persisting history remain pending.

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
The provisional Unix binary Save/Save As candidate preserves proved existing
records and reconciles uncertain installations. It does not support Maya ASCII,
Windows writing or structural rewrites, and still needs native Save QA.
Recovery, autosave and camera/history persistence remain
pending. Legacy JSON v1 and OBJ remain separate library utilities; their geometry
tests don't establish native persistence. GUI Export remains disabled. Structural
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
markers and plain primary-click replacement/clearing. The marker, click box, center
definition, center-ray visibility, depth tolerance and work cap are application policies,
not verified Maya defaults (rows 1 to 5, 12 and 13 of the [policy constants table](.project-docs/development.md#application-policy-constants)). Nonplanar faces can
hide their own centers. Occluders within the face-center depth tolerance of the center's
view depth (z-fighting range, e.g. stacked planes 1e-3 apart at distance 50) do not hide it;
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
The vertex click box, edge click distance, vertex marker, selected-edge strip, candidate
ranking and depth tolerance are original provisional values listed in the [policy constants table](.project-docs/development.md#application-policy-constants)
(rows 6 to 11), not verified Maya defaults. Colours follow only the Maya 2026 colour words (purple-ish
vertex, highlighted when selected, light-blue edge, light-orange selected edge) with
original RGB values. In Vertex and Edge modes component
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

## Native MCP idle responsiveness remains unverified

**Severity:** medium
**Area:** native protocol lifecycle

Native testing of the initial nine-tool server reproduced repeated reply deadlines while the window was backgrounded, including a deterministic minimized-window inspection failure. Desktop interaction resumed dispatch. Pinned eframe uses `App::logic` without `App::ui` for hidden windows; the original bridge drained only in UI rendering.

The revised hidden logic path and retained-input guard pass actual trait-callback tests, and [scoped macOS native verification](.project-docs/mcp-native-verification.md) confirmed minimized requests at source `36dffdf`. In the combined icon/MCP build `541c72c`, an idle request reached its UI reply deadline. A main-thread sample taken during investigation included Metal drawable acquisition, without establishing overlap with the timed-out request or a five-second acquisition wait; the next request recovered in the same process and scene token. The underlying cause remains unproven. These results do not establish unconditional idle/background responsiveness or other-platform native behavior.

## macOS Dock Quit, logout and shutdown bypass the unsaved-changes guard

**Severity:** medium
**Area:** native document lifecycle

Cmd-Q, the AztecCraft > Quit menu item, Dock Quit, Quit All from the Apple menu, logout and shutdown all send `terminate:`. `crates/macos-quit-menu` adds an `applicationShouldTerminate:` method to winit's application delegate at startup (additive `class_addMethod`; the launch aborts with a printed error if the selector already exists or the delegate shape changed; winit is pinned to 0.30.13 and `tests/winit_pin.rs` fails if the lock moves). The hook replies "terminate later", asks the app to close the front window through the shared close guard (#11), and answers AppKit with the user's choice, so each of those paths is designed to show the Save/Discard/Cancel prompt (native coverage is listed below). The earlier `performClose:` retarget of the Quit item is retired. With no key window the hook picks the window itself, which should stop the Cmd-Q beep (native check pending).

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

Source-free scenes (new, OBJ) are not refused but cannot be saved as Maya
binary. Setting every inserted word to +0.0 inserts nothing, because that is the
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
so the viewport looks faceted. There are no UVs or normals, and
new nodes are not persisted (Save is unavailable for created scenes, as for the
cube). Pole triangles cannot be extruded; body quads can. The new node is named
`pSphere1` with shape `pSphereShape1`, as in Maya (#175).
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

Create > Polygon Primitives > Cylinder and Cone, and `create_primitive` with `cylinder` or `cone`,
build radius-1, height-2 (y from -1 to 1), Y-axis solids with 20 axis divisions,
1 height division and no cap subdivisions. The cylinder is the sc=0 variant (node-page
default) with 40 points, 60 edges, 22 faces and two true 20-gon caps; the command page
lists sz 1 instead, so a Maya-default cylinder may have different counts, and the
cap topology of both solids is derived, not verified against Maya 2026 (#5). The cone
has 21 points, 40 edges and 21 faces (20 side triangles and a 20-gon base) with its apex
at (0, 1, 0) and its base at y=-1. Vertex and face order and winding are not claimed to
match Maya. Edges are hard, so the viewport looks faceted. There are no UVs or normals,
and new nodes are not persisted. Caps and cone side triangles cannot
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
pages do not say), session only and never saved. Created
primitives are still not persisted: Save is unavailable for created scenes. The AztecCraft
limits (size 1e-6 to 1e6 cm, subdivisions up to 128, at most 20,000 faces) are safety bounds,
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
session-only lambert table. It is not saved (Untitled scenes have no save), sourced binary scenes
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

## Authored Maya Binary writer: unproven profile

**Severity:** medium
**Area:** file format
**Issue:** cristim/AztecCraft#156

The fresh `.mb` writer (`src/native_scene/binary/write.rs`) takes its profile (`VERS` and the aux word
of each chunk kind) from one public Maya 2025 file and emits groups shorter than any Maya wrote. Files
it produces are Maya 2025-format; that Maya 2026 opens them, and that Maya accepts them at all, is
PROVISIONAL and unproven. AztecCraft's own decoder reopening a file is the only check. Saved meshes
will carry no shading assignment. The invented items are the I rows of
`.project-docs/binary-authored-writer.md`.

Reopen trigger: a public, licensed Maya 2026 file with a mesh becomes available (re-derive the
profile from it), or Maya rejects, repairs or rewrites these files (issue #5 asks for a read-back).

## Hierarchy delete is unsupported and its Maya semantics are provisional

**Severity:** low
**Area:** editor core
**Issue:** cristim/AztecCraft#187

Delete refuses any node with children or a parent (`unsupported_delete`), natively and over MCP.
Maya's exact Delete semantics for hierarchies are PROVISIONAL: only the 2024 Delete help page was
found and no 2026 page exists. Real hierarchy delete is blocked on a structural source rewriter.

## Marquee drag feedback has PROVISIONAL edge cases

**Severity:** low
**Area:** viewport input
**Issue:** cristim/AztecCraft#168

A plain primary drag in the viewport ends with the status "Marquee (drag) selection is not
implemented yet; click to select", because the real marquee (#30) is missing. The message is decided
per drag from egui's drag state and the release event's modifiers. These cases are PROVISIONAL and
unverified against Maya:

- A stationary press held longer than 0.8 s can count as a drag in egui 0.36 (only if some frame
  lands during the hold) and then shows the message; egui does not treat such a press as a click
  either way.
- Releasing Alt before the drag threshold is crossed shows the message, because no Orbit was emitted.
- A fast flick whose press, move and release coalesce into one frame is not seen as a drag and shows
  nothing.
- Alt held only on the release event, with no Orbit during the drag, is silent.
- A drag that starts in the viewport and ends over another panel shows the message.

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
begins, for example an edge-on ring, a ring plane behind the camera along the press ray, or a Scale
grab under 1 px along the axis, as the press shows them; native hover shows the same text. Not fixed: gating the highlight
changes native behaviour and needs its own issue; the admission is an inline expression that needs the egui context for the popup check.

## Inspection keeps reporting a pick after a view build error

**Severity:** low
**Area:** viewport manipulators, MCP inspection
**Issue:** tracked in #310

After a view build error (for example ancestors whose scales overflow), `move_viewport` survives, so
`inspect_scene` pointer fields still report a pick from the surviving capture although the viewport
paints nothing. This contradicts the "mirrors the painted highlight" wording in
`.project-docs/mcp.md` (the pointer fields paragraph). The tests
`a_scene_frame_refusal_is_reported_for_the_whole_tool_at_any_pixel` and
`sdk_pointer_refusal_reports_a_scene_frame_refusal` rely on the surviving capture.
