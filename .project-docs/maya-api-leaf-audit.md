# Maya API leaf audit — first bounded slice

Status: partial source audit, not a complete Maya API audit or a parity claim.
This slice reviews selected 2027 command/node leaves and the year-page API
differences plus the documented 2027.0/.1/.3 C++ additions. It is not a
representative sample of every catalog surface. Sources were checked on
2026-11-21.

Maya 2027.3 is the installed documentation/runtime reference. Maya 2026 remains
the acceptance target. The English Maya 2026 leaf URLs for the selected
commands and `shotLabel` node all returned 404 on the review date; that is a
documentation retrieval gap, not evidence of absence from Maya 2026. The
2027.3 runtime observations below establish names in one process only.

## Command and node leaves

| Symbol / family candidate | 2027 leaf semantics observed | UI/workflow evidence and open details | Version and runtime evidence |
| --- | --- | --- | --- |
| [`cameraSequencer`](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/cameraSequencer.html) / ANM | Undoable, queryable, editable editor command; create returns an editor name, while query return types depend on the queried flag. Leaf documents editor connections, framing/ranges, clip style (default `2`, valid `1–3`), key/tangent display and snapping, hit-testing via `menuContext`, custom draw-menu handling, and shot color/thumbnail controls. The 2027-marked `shotTimeScale` defaults to trim mode (preserves animation speed) versus scale mode (changes timing); `clearCustomColor`, `clipBounds`, `showColorDialog`, and `showThumbnail` are also marked 2027 on the leaf. | Names the Camera Sequencer editor, clips/shots, tracks and context hit-testing. The command leaf does not place controls in a menu, shelf, Tool Box, Status Line, marking menu, Channel Box or Attribute Editor. It does not identify what the 2027 index's “Updated” marker changed. | 2027.3 probe: command name present. 2026 English leaf URL returned 404; update-specific delta not established. |
| [`cameraSequencerCurrentTimeCtx`](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/cameraSequencerCurrentTimeCtx.html) / ANM | Undoable, queryable, editable command creating a context to change current time in a clip editor's track area; returns a context name. Flags cover existence, construction history, three tool-icon strings, and context name. | Leaf names the clip-editor track area only; no default gesture, menu, hotkey, shelf or other placement stated. | 2027.3 probe: command name present. 2026 English leaf URL returned 404; update provenance unknown. |
| [`polySmartBevel`](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/polySmartBevel.html) / MOD | Undoable, queryable, editable “Bevel edges” command taking polygon edge components and returning a node name. Leaf exposes common modifier flags: caching, construction history, output-node name, node state and world-space reference. Construction history is ignored if history already exists and the node is inserted; node state defaults to normal; world-space defaults off. The leaf does not specify bevel width/segments or the algorithm. | Categorized under Modeling > Polygons; the leaf does not establish a particular menu/shelf/tool placement. The separate 2027.3 What's New summary documents removal of extremely short edges, improved cutback, processing all specified edges, and preserving straight input edges. Do not infer more option semantics from that summary. | Name observed in the catalog probe; the bounded 2027.3 [behavior pass](maya-api-behavior-evidence.md) records normal-edge versus one measured short-edge outcome on synthetic cubes. No general algorithm/threshold claim. 2026 English leaf URL returned 404. The year-catalog “New” marker alone does not date introduction. |
| [`traxAudioTrack`](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/traxAudioTrack.html) / ANM | Undoable, queryable, editable; manages audio-clip tracks in Trax, including insertion/removal and lock/mute/solo state. Track indices for insertion and swapping are 1-based; removing a particular track requires it to be empty. `removeEmptyTracks` removes every empty track; `numTracks` queries count; `track` selects the track containing a named audio node. No defaults are stated for lock/mute/solo. | The leaf names the Trax editor and audio tracks. It does not give menu, shelf, hotkey or other UI placement. | 2027.3 probe: command name present. The 2027 leaf is marked 2027.2; this is update-specific page evidence for the documented leaf, not proof of Maya 2026 absence. 2026 English leaf URL returned 404. |
| [`shotLabel`](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/shotLabel.html) / ANM | Auxiliary/camera dependency node related to `sequencer`, MFn type `kShotLabel`. Attributes: `color` float3 default `(0,0,0)`; `colorR`, `colorG`, `colorB` floats default `0`; `name` string default `nullptr`. Each is input/output, connectable and storable. | The node leaf says it represents a sequencer shot label with associated label/color, but does not document creation/edit UI or menu/shelf/marking-menu/Attribute Editor placement. | 2027.3 probe: registered node type present; the bounded [behavior pass](maya-api-behavior-evidence.md) also queried defaults/current values and flags on one created node. 2026 English leaf URL returned 404; year-page difference is not an introduction/removal claim. |

The 2027 year-page snapshot separately recorded `cameraSequencer` as
“Updated”, `polySmartBevel` and `traxAudioTrack` as “New”, and `shotLabel` as a
2027-page-only node. Those are mutable catalog annotations/differences except
where an individual leaf provides an update marker. Do not turn them into
release-history claims.

## API class and member leaves

| API surface / symbol | Documented detail | Version and evidence boundary |
| --- | --- | --- |
| C++ [`MHWRender::MPreviewRenderEnvironment`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_h_w_render_1_1_m_preview_render_environment.html) | Viewport 2.0 preview utility used by LookdevX. `renderImage(MObject shaderNode, MImage& image)` renders offscreen and reports success, failure, invalid parameter or insufficient memory. The image determines output size/pixel type. Other accessors expose shape, IBL path/rotation, camera mode, display-color-space use and clear color; accepted shape strings include sphere, plane, shaderball, teapot, cloth and hair; IBL path must be EXR. | C++ changelog and class leaf date introduction to 2027.0. Do not treat the Python 2.0 catalog's appearance as an update date. No C++ runtime or behavior probe was performed. |
| Python API 2.0 [`OpenMayaRender.MPreviewRenderEnvironment`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/class_open_maya_render_1_1_m_preview_render_environment.html) | `renderImage(shaderNode, image) -> MStatus`; image size and pixel type determine output. `setClearColor(clearColor)` sets RGBA preview background. | Class is present on the 2027 Python catalog and was observed in `maya.api.OpenMayaRender` exports in the 2027.3 process. No Python per-update changelog; no behavior/ABI evidence. |
| Python API 2.0 [`OpenMayaUI.MSequencerCustomDrawManager`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/class_open_maya_u_i_1_1_m_sequencer_custom_draw_manager.html) | Described as a manager for custom sequencer drawing. Documented methods include timeline/track registration and deregistration, primitive lists, draw layer/visibility, redraw requests, tooltip/background/audio/track settings, and primitive-edit callbacks. | Observed as an `maya.api.OpenMayaUI` module export in the 2027.3 process. Mutable catalog difference only; no per-update Python source or user-facing artist workflow established here. |
| Python API 2.0 [`OpenMayaUI.MSequencerDrawPrimitive`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/class_open_maya_u_i_1_1_m_sequencer_draw_primitive.html) | Properties include draw type, start/end time, color, label, tooltip, height, bottom offset and priority; shape types include filled rectangle, outlines, vertical line, bracket and text. The Python page describes it as used with `MTimeSiderCustomDrawManager` (page spelling). | Observed as an `maya.api.OpenMayaUI` module export in the 2027.3 process. Year-page appearance does not establish introduction date. |
| C++ sequencer classes [`MSequencerDrawPrimitive`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_sequencer_draw_primitive.html), [`MSequencerDrawPrimitives`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_sequencer_draw_primitives.html), [`MSequencerEditPrimitiveFct`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_sequencer_edit_primitive_fct.html), [`MSequencerPrimitiveBuildMenuFct`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_sequencer_primitive_build_menu_fct.html) | Primitive enum covers filled rectangle, upper/full outline, vertical line, bracket and text. The array supports clear/empty/length/append/index/set. The edit callback receives time offset and current time during mouse drag; the build-menu callback receives menu ID and time. | Each class leaf/changelog marks introduction in 2027.1. This does not establish a new artist workflow or Python API release date. |
| C++ [`MFnAttribute::isNodeEditorPrimary`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_attribute.html#aaf5debf3dedaef38bf778a3853fd9969) and [`setNodeEditorPrimary`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_attribute.html#a726dda0ea425a50c02c1ed6c29573ee8) | Getter returns a boolean and accepts optional `MStatus*`; setter accepts a boolean and returns `MStatus`. Autodesk says attributes are not Node Editor primary by default. | Both members are marked 2027.3 in the leaf/changelog. The bounded [behavior pass](maya-api-behavior-evidence.md) found neither name on the Python API 2.0 `MFnAttribute` wrapper for `shotLabel.color`; C++ behavior and Node Editor end-user behavior remain untested. |
| C++ [`MPxTexture2dEvaluator`](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_px_texture2d_evaluator.html) | A plug-in node base extending `MPxNode` for batched 2D texture evaluation. Pure virtual `eval2dTextureBatch(const MDoubleArray& uCoords, const MDoubleArray& vCoords, MVectorArray* resultColors, MDoubleArray* resultAlphas)` evaluates color/alpha at multiple UV samples. | Introduced in the 2027.3 changelog. No plugin was loaded to test presence or behavior; no ABI claim. |

The complete 2027.3 process probe observed the Python API 1.0 module's public
top-level names, but this leaf slice does not enumerate API 1.0 class members.
Autodesk's year-level Python reference is API 2.0; API 1.0 remains a separate
documentation/source gap. See the [API source inventory](maya-2027-api-inventory.md)
and [non-scoring API/workflow crosswalk](maya-api-workflow-crosswalk.md).

## Verification boundary and next audit work

- The original 2027.3 standalone catalog probe reported all four commands and
  `shotLabel` as present by name in one Windows process. The separate bounded
  [behavior pass](maya-api-behavior-evidence.md) exercises only the named
  synthetic cases; it does not establish complete command/node behavior or UI
  flows.
- API 2.0 `dir(module)` confirms module-level exported names only. A separate
  behavior check inspects two Python wrapper names on one `MFnAttribute`
  instance; it does not establish C++ member behavior or ABI compatibility.
- A 404 for a 2026 English leaf route is not evidence that the API was absent
  from Maya 2026. The requested 2026 acceptance build remains unverified.
- Next: obtain usable 2026 documentation/runtime evidence, expand command/node
  and API leaf coverage beyond these name-level deltas, and test user-facing
  workflows against the pinned Maya versions. Keep family links provisional
  until UI placement and relevant behavior have been reviewed.
