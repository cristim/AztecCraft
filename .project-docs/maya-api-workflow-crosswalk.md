# Maya API-to-workflow crosswalk (non-scoring)

This companion is a non-exhaustive traceability aid, not a workflow inventory
or semantic audit. It does not add rows to or alter the parity inventory's
14-family denominator. API membership/runtime exports do not establish workflow
semantics, UI placement, implementation, compatibility, or parity.

## Fields and mapping status

| Field | Definition |
| --- | --- |
| API observation | Symbol or grouped change described in the API inventory. |
| Source version / class | Release and evidence type: update-specific C++ changelog, mutable year-page difference, or bounded runtime observation. |
| Workflow / family | Candidate workflow and family only where justified; otherwise `TBD` or `unmapped`. |
| Mapping status | `mapped` is tentative source-backed traceability, not semantic confirmation; `TBD` needs owner review; `unmapped` asserts no workflow. |
| Autodesk source | Exact leaf or source page supporting the observation. |

Command, node and Python year-page differences have unknown update provenance;
they must not be assigned to a particular update. Runtime modules and exports
are not workflows and remain unmapped. No semantic audit is complete.

## Seed rows

| API observation | Source version / class | Workflow / family | Mapping status | Autodesk source |
| --- | --- | --- | --- | --- |
| `MFnAttribute::isNodeEditorPrimary`, `setNodeEditorPrimary` | 2027.3 / C++ update changelog | Node Editor presentation / SHL candidate | TBD | [2027.3 C++ changelog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_3.html) |
| `MPxTexture2dEvaluator` | 2027.3 / C++ update changelog | Texture evaluation / MAT candidate | TBD | [2027.3 C++ changelog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_3.html) |
| `cameraSequencer` (marked Updated), `cameraSequencerCurrentTimeCtx`, `polySmartBevel`, `traxAudioTrack` (marked New) | 2027 vs 2026 / mutable command year-page difference; update provenance unknown | Sequencer, bevel or Trax candidates pending leaf review | TBD | [2027 command catalog](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index_all.html); [cameraSequencer](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/cameraSequencer.html); [cameraSequencerCurrentTimeCtx](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/cameraSequencerCurrentTimeCtx.html); [polySmartBevel](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/polySmartBevel.html); [traxAudioTrack](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/traxAudioTrack.html) |
| `shotLabel` | 2027 vs 2026 / mutable node year-page difference; update provenance unknown | Sequencer labeling candidate | TBD | [2027 node catalog](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index_all.html); [shotLabel leaf](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/shotLabel.html) |
| `MHWRender::MPreviewRenderEnvironment` | 2027 vs 2026 / mutable C++ year-page difference; introduction is update-specific to 2027.0 per changelog | Preview rendering / RND candidate | TBD | [2027.0 C++ changelog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_0.html); [class leaf](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_h_w_render_1_1_m_preview_render_environment.html) |
| `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`, `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct` | 2027 vs 2026 / mutable C++ year-page difference; introductions are update-specific to 2027.1 per changelog | Sequencer drawing candidate | TBD | [2027.1 C++ changelog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_1.html) |
| Python API 2.0 `MPreviewRenderEnvironment`, `MSequencerCustomDrawManager`, `MSequencerDrawPrimitive` | 2027 vs 2026 / mutable Python year-page difference; update provenance unknown | Catalog classes alone do not establish workflow | unmapped | [2027 Python API 2.0 catalog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/annotated.html) |
| `maya.OpenMaya` (API 1.0) | 2027.3 / bounded runtime top-level export observation; no complete API 1.0 catalog established | Runtime module exports are not a workflow | unmapped | [Maya Python Libraries](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Scripting/files/GUID-49701DA7-9224-424D-93F7-EC1DA779C99C.htm); [Python API reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/index.html) |
| `maya.api.OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI` | 2027.3 / bounded runtime top-level export observation, Python API 2.0 | Runtime module exports are not workflows | unmapped | [Python API 2.0 overview](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/index.html); [Python API 2.0 catalog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/annotated.html) |

Rows are seeds only. Owners must verify leaf semantics, workflow/UI placement,
and version evidence before changing any family inventory row.
