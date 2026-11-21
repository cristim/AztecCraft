# Maya API catalog sources and version evidence

Source registry and name-level evidence for Autodesk's public Maya command,
node, C++ and Python catalogs. This is not a complete semantic audit. Catalog
membership is not runtime compatibility: nothing here says AztecCraft loads,
implements or is compatible with any Maya API.

- Source: Autodesk Maya Help and Developer Help pages linked below (Autodesk
  documentation; cited and summarised, not copied).
- Current documentation reference: Maya 2027.3 (see [reference.md](reference.md)).
  The existing 2026-11-14 name comparison and entries checked on 2026-11-20
  are historical retrieval-time observations, not a fresh 2027.3 crawl.
- Author: Elvis33LE (original draft, PR #457), trimmed and re-verified by the
  AztecCraft maintainers.
- Maya 2026 remains the acceptance target. The local Maya 2027.3 executable's
  file version is recorded in [reference.md](reference.md); a bounded Maya
  2027.3 runtime observation is recorded below. No devkit inventory or Maya
  2026 runtime evidence is available.
- Families are the project's 14 (see the
  [family inventory](parity-inventory/README.md)). A catalog symbol is not a
   workflow and not a parity denominator row. See the partial, non-scoring
   [API-to-workflow crosswalk](maya-api-workflow-crosswalk.md); it does not
   alter the 14-family denominator.

## Source registry

| Surface | Autodesk source | Established | Status |
| --- | --- | --- | --- |
| MEL and `maya.cmds` commands | [2027 year catalog](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index_all.html), [2026 year catalog](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/index_all.html) | Mutable year-level alphabetical catalog; mixes core commands and MEL scripts, with "New" and "Updated" markers | Name extraction only; not update snapshots; flags, modes and results on leaf pages are not reviewed |
| Dependency nodes | [2027 year catalog](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index_all.html), [2026 year catalog](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/index_all.html) | Mutable year-level node catalog | Name extraction only; not update snapshots; attributes and defaults not reviewed |
| C++ API classes | [2027 year catalog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html), [2026 year catalog](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/annotated.html) | Mutable year-level Doxygen catalog including `MHWRender`, function sets, iterators and plug-in base classes | Name extraction only; not update snapshots; members and signatures not reviewed |
| C++ namespaces, modules, members | [namespaces](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/namespaces.html), [modules](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/modules.html), [members](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/functions.html) | Separate indexes for free functions and grouped API | Indexes identified, but not systematically enumerated or reviewed (PROVISIONAL) |
| Python API 2.0 | [2027 year catalog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/annotated.html), [2026 year catalog](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/py_ref/annotated.html); [packages](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html) | Mutable year-level Doxygen catalog; four documented packages: `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI` | Names only; not update snapshots; members not reviewed |
| Python API 1.0 | [Maya Python libraries](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Scripting/files/GUID-49701DA7-9224-424D-93F7-EC1DA779C99C.htm), [Developer Help](https://help.autodesk.com/view/MAYADEV/2027/ENU/) | `maya.OpenMaya` is "Python API 1.0", `maya.api.OpenMaya` is "Python API 2.0" (more pythonic, less comprehensive), `maya.cmds` wraps MEL; PyMEL is third-party and not supported by Autodesk | No 1.0 symbol list enumerated; do not infer it from 2.0. The page's 2.0 reference link points at a 2025 path |

## Name counts

The crawler was rerun on 2026-11-20; all eight pages passed structural
validation. Counts below match local snapshot
`target/maya-catalog/snapshot-20261120T211346Z-714` (retrieval time
`2026-11-20T21:13:29Z`; its manifest holds the exact raw-page and extracted-list
SHA-256 values). The snapshot is git-ignored and must not be committed. These
remain mutable year-level page counts, not Maya 2027.3 update counts, complete
inventories, or runtime registrations. The count bounds are structural
tripwires only.

| Surface | 2027 | 2026 | Difference check |
| --- | --- | --- | --- |
| Commands | 1528 | 1524 | 4 names, all confirmed below |
| Nodes | 944 | 943 | 1 name (`shotLabel`), confirmed by diffing both full pages |
| C++ classes (qualified) | 641 (contributor: 640) | 634 | 7 names, see below |
| Python API 2.0 classes | 313 | 310 | 3 names, all confirmed below |

The delta below was recomputed from that dated local snapshot. It compares
year-level pages and cannot establish when a name was introduced within either
year.
The current crawler preserves each run as an immutable local snapshot with a
manifest of source URLs, retrieval time, hashes, counts and validation status.
Failed pages are not published as a snapshot. Only official per-update sources
can support update-specific attribution; for this audit that source exists for
C++ changelogs, not the command, node or Python year catalogs.

## Year-page difference, 2026 vs 2027 snapshots

Names present in the 2027 snapshot and absent from the 2026 snapshot. These are
retrieval-time year-page differences, not release deltas. Their source lists
are retained only in ignored local output; update provenance is unknown unless
an update-specific source establishes it.

| Surface | Observed 2027-page-only names (date/provenance limitations below) |
| --- | --- |
| Commands | `cameraSequencer` (marked "Updated" in the 2027 index), `cameraSequencerCurrentTimeCtx`, `polySmartBevel`, `traxAudioTrack` (marked "New") |
| Nodes | `shotLabel` |
| C++ classes | `MHWRender::MPreviewRenderEnvironment`, `MPxFileDialog`, `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`, `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct`, `MPxTexture2dEvaluator` (the 2027.3 C++ changelog below confirms this class was introduced in 2027.3; the earlier snapshot absence alone was not proof) |
| Python API 2.0 | `MPreviewRenderEnvironment`, `MSequencerCustomDrawManager`, `MSequencerDrawPrimitive` |

Note: a node named `polySmartBevel` is in both years' node lists, so only the
command is new. The `cameraSequencer` pair needs per-command review before it
is used as a version claim. Neither catalog is a runtime inventory; the served
2026 pages may already contain 2026.1, 2026.2 or 2026.3 content; neither the
presence nor absence of a name dates it to a particular update.

## Maya 2027.3 runtime observation

On 2026-11-20, `scripts/maya-api-runtime-probe.py` ran with the installed
Windows `mayapy.exe`, initialized in standalone batch mode. It reported Maya
`version=2027`, `installedVersion=Autodesk MAYA 2027.3`,
`apiVersion=20270300`, `buildVariant=Release`, and
`cutIdentifier=202611131425-e0ccee1081`; the `about(date=True)` value was
captured verbatim as `2026 14 25`. The executable and `mayapy.exe` file version
is `27.3.0.2355` (see [reference.md](reference.md)).

The probe enumerated 4,813 command names, 902 registered node type names and
two currently loaded plug-ins (`cacheEvaluator` and `renderSetup`). Both
reported plug-in API version `20270300`; their reported versions were `1.0`.
The command count comes from the installed runtime's `help("*", list=True)`
enumeration and is not directly comparable to the 1,528 linked command entries
in the Autodesk year-page catalog.
The same probe imported all five requested API modules successfully and
enumerated public module-level names: `maya.OpenMaya` (API 1.0) 282;
`maya.api.OpenMaya` 169; `OpenMayaAnim` 17; `OpenMayaRender` 93; and
`OpenMayaUI` 52. These are observed exports for this process, not complete
member inventories or evidence of API 1.0/2.0 behavioral parity.
The complete local JSON result and stderr capture are under ignored
`target/maya-catalog/runtime-20261120-pythonapis.json` and
`runtime-20261120-pythonapis.stderr.log`; neither is committed.

This is one standalone process on one Windows build. The two plug-ins are only
those loaded in that process; this does not enumerate installed-but-unloaded
plug-ins or all possible initialization states. API 1.0 evidence is limited to
top-level module names: it is not a class/member inventory or behavioral audit,
and establishes neither ABI compatibility nor Maya 2026 compatibility. The script omits
plug-in paths, scene data and exception messages; it does not load/unload
plug-ins or open user scenes.

## C++ changelogs

Official change pages: [2027.0](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_0.html),
[2027.1](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_1.html),
[2027.2](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_2.html),
[2027.3](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_3.html),
[2026.0](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_0.html),
[2026.1](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_1.html),
[2026.2](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_2.html).
No 2026.3 page was found at the expected
`changelog_2026_3.html` endpoint (404 on 2026-11-20), despite official
[Maya 2026.3 release notes](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2026_3_RELEASE_NOTES_HTML.html).
These pages establish documented C++ introductions only. They do not enumerate
all API surfaces or establish runtime presence. Python, command and node
differences above are not update-dated.

The entries below were checked against each linked page on 2026-11-20. Names
are summarized rather than copying signatures; consult the source page for the
full declaration. Groups are grouped by declaring class/type:

- **2027.0:** `MAnimUtil::AnimLayerFilter`, `createAnimCurve`,
  `findActiveAnimationLayer`, `findAnimationLayers`, `findAnimCurve`,
  `findBlendNodeInputPlug`; `MFnDistanceManip::isSnapModeOn`, `setSnapMode`;
  `MFnPlugin::registerFileDialog`, `deregisterFileDialog`;
  class `MHWRender::MPreviewRenderEnvironment`;
  `MHWRender::MPxShaderOverride::supportsNonTextured`;
  `MHWRender::MRenderItem::ufePathString`;
  `MHWRender::MRenderOverride::colorManagedOperation`;
  `MHWRender::MRenderUtilities::createPreviewRenderEnvironment`,
  `getIBLPresetPath`;
  `MHWRender::MSceneRender::pluginDisplayFilterExclusions`.
- **2027.1:** `MFnFreePointTriadManip::setExtraMatrix`; classes
  `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`,
  `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct`.
- **2027.2:** `MItMeshEdge::getConnectedFaces`, `getIncidentFaces`.
- **2027.3:** `MFnAttribute::isNodeEditorPrimary`,
  `MFnAttribute::setNodeEditorPrimary`; class `MPxTexture2dEvaluator`.
- **2026.0:** `M3dView::numViewSelectedObjects`, `viewSelected`,
  `viewSelectedObject`; `MDagPath::matchLocalMatrix`;
  `MDGModifier::setPlugLockState`; `MFnVolumeLight::Axis`, `setFaceAxis`;
  `MHWRender::MGeometryExtractor::MGeometryExtractor`;
  `MMessage::MStringBoolFunction`;
  `MHWRender::MPxShadingNodeOverride::fragmentName`,
  `handlesConsolidatedGeometry`, `updateShader`;
  `MRampAttribute::getColorsAtPositions`, `getValuesAtPositions`;
  `MHWRender::MRenderItem::cullMode`, `CullMode`, `setCullMode`;
  `MHWRender::MShaderManager::k3dIsotropicOpenPBRSurfaceShader`,
  `k3dOpenPBRSurfaceShader`;
  `MUiMessage::add3dViewSelectedChangedCallback`.
- **2026.1:** `MDGContext::isManaged`, `isTimed`;
  `MFnMesh::hasBlindDataComponentIds`;
  `MGlobal::executeCommandOnIdleWithPriority`,
  `executePythonCommandOnIdleWithPriority`.
- **2026.2:** `MExternalDropData::formatData`.
- **2026.3:** no C++ changelog page was found at the expected URL; do not infer
  that the release had no C++ API changes.

These are documentation observations, not proof of runtime availability, ABI
compatibility or equivalent behaviour in Maya 2026.

## Open work

Not covered: leaf detail for every symbol (command flags and modes, node
attributes and defaults, C++ and Python members and signatures), C++
namespaces and free functions, a Python API 1.0 catalog, behavioral diffs and
all runtime or ABI compatibility. The next stages and evidence gates are in
[maya-api-audit-plan.md](maya-api-audit-plan.md). Keep documentation,
catalog extraction, runtime enumeration, behavioral testing, component
availability and ABI compatibility as independent evidence; do not substitute
a guessed count for uncrawled work.
