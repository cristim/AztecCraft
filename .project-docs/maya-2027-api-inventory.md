# Maya API catalog sources and 2026 to 2027 delta

Name-level source registry for Autodesk's public Maya command, node, C++ and
Python catalogs, with the release delta between the 2026 and 2027
documentation. Catalog membership is not runtime compatibility: nothing here
says AztecCraft loads, implements or is compatible with any Maya API.

- Source: Autodesk Maya Help and Developer Help pages linked below (Autodesk
  documentation; cited and summarised, not copied).
- Pinned version: Maya 2027.2 documentation (see [reference.md](reference.md)).
  Access date: 2026-11-20 (first drafted by the contributor on 2026-11-14).
- Author: Elvis33LE (original draft, PR #457), trimmed and re-verified by the
  AztecCraft maintainers.
- Acceptance stays the pinned Maya 2026 target. A 2027 catalog cannot satisfy or
  widen it. No Maya executable, devkit or plug-in manifest was used.
- Families are the project's 14 (see the
  [family inventory](parity-inventory/README.md)). A catalog symbol is not a
  workflow and not a parity denominator row.

## Source registry

| Surface | Autodesk source | Established | Status |
| --- | --- | --- | --- |
| MEL and `maya.cmds` commands | [2027 index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index.html), [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/index.html); the full alphabetical catalog is the child page `Commands/index_all.html` ("All commands") | Entry point for command names; the catalog mixes core commands and MEL scripts and carries "New" and "Updated" markers | Names only; flags, modes and results on leaf pages are not reviewed |
| Dependency nodes | [2027 index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index.html), [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/index.html); catalog `Nodes/index_all.html` ("All nodes") | Entry point for node names | Names only; attributes and defaults not reviewed |
| C++ API classes | [2027 class list](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html), [2026](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/annotated.html) | Doxygen class catalog including `MHWRender`, function sets, iterators and plug-in base classes | Names only; members and signatures not reviewed |
| C++ namespaces, modules, members | [namespaces](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/namespaces.html), [modules](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/modules.html), [members](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/functions.html) | Separate indexes for free functions and grouped API | Not reviewed on 2026-11-20 (PROVISIONAL) |
| Python API 2.0 | [2027 packages](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html), [2026](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/py_ref/namespaces.html); classes in `py_ref/annotated.html` | Four packages, identical in both years: `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI` | Names only; members not reviewed |
| Python API 1.0 | [Maya Python libraries](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Scripting/files/GUID-49701DA7-9224-424D-93F7-EC1DA779C99C.htm), [Developer Help](https://help.autodesk.com/view/MAYADEV/2027/ENU/) | `maya.OpenMaya` is "Python API 1.0", `maya.api.OpenMaya` is "Python API 2.0" (more pythonic, less comprehensive), `maya.cmds` wraps MEL; PyMEL is third-party and not supported by Autodesk | No 1.0 symbol list enumerated; do not infer it from 2.0. The page's 2.0 reference link points at a 2025 path |

## Name counts

Counts of catalog names (2027 / 2026) as reported by the contributor from
their 2026-11-14 crawl. They were not recounted on 2026-11-20 because the
fetch tool truncates the long index pages; the differences below were
checked (see the delta table), the absolute numbers were not.

| Surface | 2027 | 2026 | Difference check |
| --- | --- | --- | --- |
| Commands | 1528 | 1524 | 4 names, all confirmed below |
| Nodes | 944 | 943 | 1 name (`shotLabel`), confirmed by diffing both full pages |
| C++ classes (qualified) | 640 | 634 | 6 names, all confirmed below |
| Python API 2.0 classes | 313 | 310 | 3 names, all confirmed below |

The raw name lists and the crawl script from the PR are intentionally not part
of this file; regenerate them from the linked pages if a list is needed.

## Release delta, 2026 catalog to 2027 catalog

Names present in the 2027 catalog and absent from the 2026 catalog (the
contributor's crawl found no 2026-only names; that check was not repeated).
Entries were re-checked against the live pages on 2026-11-20 except
`polySmartBevel` and `traxAudioTrack` (both commands), which come from the
contributor's crawl: the commands index page is truncated in the fetch and
those two names were not independently re-read.

| Surface | 2027-only names |
| --- | --- |
| Commands | `cameraSequencer` (marked "Updated" in the 2027 index), `cameraSequencerCurrentTimeCtx`, `polySmartBevel`, `traxAudioTrack` (marked "New") |
| Nodes | `shotLabel` |
| C++ classes | `MHWRender::MPreviewRenderEnvironment`, `MPxFileDialog`, `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`, `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct` |
| Python API 2.0 | `MPreviewRenderEnvironment`, `MSequencerCustomDrawManager`, `MSequencerDrawPrimitive` |

Note: a node named `polySmartBevel` is in both years' node lists, so only the
command is new. The `cameraSequencer` pair needs per-command review before it
is used as a version claim. Neither catalog is a runtime inventory; the served
2026 pages may already contain 2026.1 and 2026.2 content.

## C++ changelogs

Official change pages: [2027.0](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_0.html),
[2027.1](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_1.html),
[2027.2](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_2.html),
[2026.0](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_0.html),
[2026.1](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_1.html),
[2026.2](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_2.html).
These are C++ deltas only; Python, command and node deltas are not diffed
beyond names.

- 2027.0 (introduced in that release): `MAnimUtil` animation-layer helpers
  (`AnimLayerFilter`, `createAnimCurve`, `findActiveAnimationLayer`,
  `findAnimationLayers`, `findAnimCurve`, `findBlendNodeInputPlug`);
  `MFnDistanceManip::isSnapModeOn` and `setSnapMode`; `MFnPlugin::registerFileDialog`
  and `deregisterFileDialog`; class `MPreviewRenderEnvironment`;
  `MPxShaderOverride::supportsNonTextured`; `MRenderItem::ufePathString`;
  `MRenderOverride::colorManagedOperation`;
  `MRenderUtilities::createPreviewRenderEnvironment` and `getIBLPresetPath`;
  `MSceneRender::pluginDisplayFilterExclusions`.
- 2027.1: `MFnFreePointTriadManip::setExtraMatrix` and the four sequencer
  classes `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`,
  `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct`.
- 2027.2: `MItMeshEdge::getConnectedFaces` and `getIncidentFaces`.
- 2026.0: the page exists (it lists, among others, `M3dView::viewSelected`,
  `MDagPath::matchLocalMatrix`, `MDGModifier::setPlugLockState`,
  `MRenderItem::cullMode`); not reviewed further.
- 2026.1: `MDGContext::isManaged` and `isTimed`, `MFnMesh::hasBlindDataComponentIds`,
  `MGlobal::executeCommandOnIdleWithPriority` and
  `executePythonCommandOnIdleWithPriority`.
- 2026.2: `MExternalDropData::formatData`.

These are documentation observations, not proof of runtime availability, ABI
compatibility or equivalent behaviour in Maya 2026.

## Open work

Not covered: leaf detail for every symbol (command flags and modes, node
attributes and defaults, C++ and Python members and signatures), C++
namespaces and free functions, a Python API 1.0 catalog, behavioural diffs and
all runtime or ABI compatibility. Keep symbol enumeration, documented
semantics, runtime tests, component availability and ABI compatibility as
independent evidence fields; do not substitute a guessed count for uncrawled
work.
