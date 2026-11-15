# Camera optics and film back (#458)

Status: research and implementation plan only; no new camera capability implemented.
Owner: `codex-458-film-back`. Independent reviewer: `codex-458-film-back-reviewer`.
Baseline: `308e2939`; #452 / PR #468 is a separate, unmerged camera-navigation change.

## Reference and expected behaviour

Read on 2026-11-15 against the existing Maya 2027.2 documentation pin in
[reference.md](reference.md). The 2027.2 release-notes page remains available.
The release index returned HTTP 403, so latest-release status was not independently
reverified today; retain the dated pin, do not infer a newer or installed build.
The Maya 2026 acceptance objective remains unmet. Sources below are paraphrased;
no Autodesk assets or documentation text are redistributed.

| Official source | Expected behaviour and placement |
| --- | --- |
| [Camera node][node] | Camera is a shape under a transform. Apertures, offsets, focal length, squeeze, overscan and pre/post scale are doubles; film roll is an angle. `filmFit`: 0 Fill, 1 Horizontal, 2 Vertical, 3 Overscan. Defaults: focal length 35 mm; offsets/roll 0; squeeze, overscan and scales 1. The node table gives fit default 1 and aperture children 0, whereas its compound aperture describes a 36 by 24 mm gate. Do not use child defaults independently as a valid zero aperture. |
| [Camera command][command] | Query/edit flags expose optical attributes. Focal length stays in mm; aperture/offset stay in inches regardless of scene linear units. Horizontal angle of view derives from aperture and focal length. Film roll is in degrees in this command. Pre scale precedes roll, post scale follows it. The command describes squeeze as informational, which conflicts with a blanket claim that Maya always applies it to the viewport. |
| [Create Camera Options][create-options] | Create > Cameras > camera type option box. New-camera defaults: 35 mm focal length, apertures 1.4173/0.9449 inches, squeeze 1, fit Fill, offset 0, overscan 1. Focal-length input range 2.5 through 100000 mm; above 3500 requires numeric entry. Fill fits the resolution gate within the film gate; Overscan fit does the reverse. Film Fit Offset acts only for Horizontal/Vertical fit. Overscan expands the working view without changing rendered output. Fit is also exposed in the camera panel View > Camera Settings submenu. |
| [Camera Attribute Editor][attribute-editor] | Panel View > Camera Attribute Editor opens the camera shape. Focal Length belongs to Camera Attributes. Film Back contains Film Gate (User default), Camera Aperture, Film Aspect Ratio, Lens Squeeze Ratio, Fit Resolution Gate, Film Fit Offset, Film Offset, pre/post scale and film-roll controls. Aperture entry supports both inches and mm; saved values remain inches. Display Options contains Overscan, also available through View > Camera Settings. The issue's single mm display description is incomplete. |
| [Camera Setup][setup] and [Create a camera][create] | Four default cameras: perspective, top, front, side. Create > Cameras creates additional cameras; panel Panels > Perspective/Orthographic chooses the viewing camera. Scene camera identity must remain distinct from a session viewport pose. |
| [MFnCamera][api] | `getViewParameters` describes near-plane frustum bounds from aperture, offset, focal length and camera scale. `getViewingFrustum` has separate applyOverscan/applySqueeze/applyPanZoom switches, each default false. Film-roll API values are radians; the post-projection matrix is separate. These specify useful math boundaries, not which switches an actual Maya panel enables. |
| [setKeyframe][keys] | Keys target objects/attributes at current or explicit time; explicit values may override the current value. Camera focal length needs a shape attribute target and animation evaluation, rather than another transform component. Existing linear session interpolation is not Maya tangent parity. |

No dedicated optics hotkey, shelf, Tool Box, Status Line or marking-menu entry
is stated by these sources. Channel Box visibility/keyability and exact widget
order/formatting beyond the documented sections require reference verification;
mark those choices PROVISIONAL rather than inventing Maya behaviour.

## Conflicts and provisional decisions

- Preserve imported explicit values. Keep node defaults distinct from Create
  Camera option defaults; current viewport Fill is not proof of a scene-camera default.
- Use full f64 values in authoritative storage, drafts, history and inspected
  state. Convert only at rendering boundaries; unit toggles must not rewrite
  untouched inches. Parse/write round trips must preserve f64 bits, signed zero
  where supported, unknown data and untouched source bytes.
- Squeeze is an optical lens attribute, not interchangeable with output pixel
  aspect. The eventual viewport policy and order of squeeze/fit/offset must be
  labelled PROVISIONAL until compared with a pinned Maya panel. Do not silently
  choose an interpretation of contradictory documentation.
- Numeric UI may use the documented focal-length entry range. Other bounds,
  precision formatting, matrix composition signs/order not specified above,
  fit aspect derived from viewport versus output resolution, roll pivot/order
  assumptions and orthographic optical effects need explicit decisions and
  tests before implementation. Reject invalid/non-finite or unrepresentable
  edits before mutation; preserve otherwise uneditable imported source values.
- Preserve dependent camera attributes such as film-fit offset, camera scale,
  roll pivot/order, film translation, shake and pan/zoom. Until evaluated, a
  nondefault or connected unsupported contributor must prevent an assertion
  of faithful projection and refuse affected edits/writes as appropriate.
- Synthetic projection checks establish internal correctness only. Native
  screenshot review and a pinned Maya comparison are separate gates.

## Audited starting boundary

`NativeScene` stores transforms, meshes and curves, with no camera shape table.
The ASCII reader recognizes only shared default camera shapes, consumes their
limited `.fl`/`.coi`/`.ow` fields without retaining optical state, and rejects
custom camera shapes and the broader film-back attributes. The binary binding
allocates native identities for transforms/meshes, not `DCAM` camera shapes.
Retained unknown bytes are not decoded/editable camera support.

The viewport `Lens::Film` holds fixed f32 constants, not scene optics. Session
bookmarks retain poses, not camera-shape attributes. #468 adds navigation history
and read-only focal length for this existing lens; it does not supply native
camera storage. Key masks and curves currently support nine transform channels.
Existing scalar source edits cover transforms and material diffuse, but there
is no camera optical binding or write intent. Extending the viewport alone
would lose optical data at Save and fail the matchmove objective.

## Independently reviewable slices

1. **This change: researched boundary and plan.** No source changes, tools,
   schema promises or controls. Keep #458 open. Two independent plan/diff passes
   precede the docs commit and locked feature-branch push; open a draft PR early.
2. **Camera identity and lossless read foundation.** Add camera-shape identity,
   f64 optics and source bindings to `NativeScene`, starting with a bounded
   synthetic static ASCII class. Preserve existing default-camera admission;
   reject ambiguity, malformed declarations and unsupported dependencies.
   Inspection, any newly selectable camera shape and its state must enter the
   real Editor/MCP path in the same slice, with schema and coverage updates.
   No editable optical controls until dirty state and a safe persistence policy
   exist. Define binary-read scope separately from ASCII evidence.
3. **Guarded static editing and first persistence path.** Add a shared typed
   camera-targeted operation and History command; native Camera Attributes /
   Film Back fields and MCP use identical authoritative state. A minimal authored
   ASCII Save/Open path must retain camera identity and exact optical values;
   source-bound writes require validated spans/intent and guarded originals.
   Unsupported binary or sourced edits/writes fail explicitly. Until slice 4,
   this is static data editing: native UI and MCP must visibly report that
   projection does not consume those edits, with no matchmove alignment claim.
   Do not imply complete .ma/.mb support from an authored ASCII path.
4. **Projection consumers.** Bind the viewing camera to scene identity; derive
   projection from authoritative optics. One matrix path must drive CPU/GPU
   projection, picking/unprojection, framing, overlays and capture equality.
   Cover four fits, offset, overscan, squeeze policy and post-projection roll /
   scales. Test portrait/landscape/equal-aspect gates, off-axis ray alignment,
   invalid and extreme values, ortho policy and failed-edit preservation.
   Camera navigation and bookmarks must not overwrite optical attributes.
5. **Binary and sourced persistence.** Decode camera records with proven scalar
   bindings, preserve untouched precision/unknown data, then support only
   reviewed edits and authored writes. Test Save, Save As, repeated Save, Open,
   changed-source and occupied-destination guards; unsupported layouts remain
   refused. Maya acceptance requires independent reference evidence.
6. **Focal-length animation.** Extend existing key operations with an explicit
   camera-shape attribute target after reviewing the transform channel schema,
   timeline evaluation, history and writer boundaries. Preserve old clients;
   do not relabel a tenth transform channel as focal length. Exact keys,
   interpolation, time changes, undo/redo, readback and save/load each need proof.

Each source slice gets a refined plan and two independent plan/diff passes before
its atomic commit; this research commit does not approve future implementations.
Feature completion requires all issue checkboxes, not only the first slice.

## Ownership, checks and completion gates

Implementation: `codex-458-film-back`; review: `codex-458-film-back-reviewer`.
This initial assignment edits only this note and its link in `plan.md`. Leave
shared Editor/NativeScene/adapters unchanged until their owners are coordinated
on the issue/PR. #468 overlap includes `src/viewport/lens.rs`, camera dispatch,
inspection, schemas and capture. Re-read merged/rebased hunks before extending.

Use the repository build lock and a dedicated target for later source checks;
mutation probes get a separate scratch target. The docs-only slice requires
`git diff --check`, link/source verification and independent review; no Rust or
native behaviour changed. Source slices use [development.md](development.md):
`cargo fmt --all -- --check`, strict Clippy, headless tests, relevant viewport /
persistence suites, real Editor/SDK tests with `--features mcp-test-client`,
production MCP build and a reviewed real native-window journey. Test token,
human draft, focus, modal, close and manipulation guards, exact baseline
preservation for remote refusals and native `commit_pending` behaviour.

Update `mcp.md`, the tool catalog and capability inventory in every exposed
feature slice. Keep automation opt-in, source scripts inert, private assets and
machine-specific evidence outside commits. Push reviewed atomic commits under
the repository push lock, then arm `ci-watch`. Keep the PR draft until ready;
merge still needs green CI, two independent current-head approvals and human
maintainer sign-off. No parity claim follows from this plan.

[node]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/camera.html
[command]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/camera.html
[create-options]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-35B872B1-840E-4DEE-B80F-F2715B1E8BF0.htm
[attribute-editor]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-C3EBB008-7DBE-4B9D-B9AC-1DA974CEFE15.htm
[setup]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-4410F526-56AC-4788-A7AF-2EAA970BB89D.htm
[create]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-C668AE39-9D09-4EFA-932E-E3D4AFF8C099.htm
[api]: https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_camera.html
[keys]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/setKeyframe.html
