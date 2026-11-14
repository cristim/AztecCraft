# Maya API catalog inventory (2027.2 lens; 2026 acceptance)

**Status: partial source audit, not an exhaustive API inventory.** This file
records what can be established from Autodesk's public catalogs and explicitly
tracks the enumeration work still required. It does not assert that AztecCraft
implements, loads, or is compatible with any Maya API. Catalog membership is
not runtime compatibility.

## Scope and method

- Documentation lens: Maya 2027.2, with the 2027.0 and 2027.1 API catalogs
  where versioned changelogs are available. Retrieval date: 2026-11-14.
- Acceptance remains the pinned Maya 2026 target; a 2027 catalog or runtime
  cannot silently expand or satisfy that target.
- Sources below are Autodesk Help / Developer Help only. Pages are live
  catalogs, not locally archived snapshots; no source SHA or full crawl was
  produced for this artifact.
- No Maya executable, matching devkit, plug-in installation manifest, or
  runtime comparison was established here.
- Family assignment uses the project's 14 families: SHL application shell,
  SCN scene, VPT viewport, MOD polygon modeling, CRV curves/surfaces, UVE UV,
  ANM animation, RIG rigging, MAT materials/lighting, RND rendering, INT
  interchange, SCR scripting/extensions, DYN dynamics, PIP pipeline.
  Cross-cutting symbols may map to multiple families. A catalog symbol is not
  itself a user workflow or parity denominator row.

## Official source registry

| Surface | Autodesk source | What this establishes | Enumeration status here |
|---|---|---|---|
| MEL / `maya.cmds` commands | [2027 Commands index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index.html); [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/index.html) | Official command reference entry point. The wrapper page is a frameset whose child `Commands/index_all.html` carries the alphabetized command catalog; a direct fetch of that child exposes every linked command anchor. | **Catalog names available via `index_all.html` but not yet transcribed or counted in this artifact.** Flags, modes, results and notes on the leaf pages remain uncrawled. |
| Dependency nodes | [2027 Node reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index.html); [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/index.html) | Official node reference entry point. Its frameset child `Nodes/index_all.html` carries the alphabetized node catalog, including node names and "Updated" markers. | **Catalog names available via `index_all.html` but not yet transcribed or counted in this artifact.** Attributes, defaults and classifications on leaf pages remain uncrawled. |
| C++ API classes | [2027 class list](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html); [2026 class list](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/annotated.html) | Official Doxygen class catalog. The raw `annotated.html` HTML exposes every class/namespace row (hundreds of anchors, including nested namespaces); text-only extraction collapses it and must use HTML parsing. 2027 class page visibly spans Maya API, `MHWRender`, function sets, iterators, callbacks, plug-in base classes and other domains. | **Catalog rows retrievable in HTML; no complete transcription/count in this artifact yet.** |
| C++ namespaces/modules/members | [2027 namespaces](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/namespaces.html); [modules](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/modules.html); [member index](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/functions.html) | Separate indexes needed in addition to class names to cover free functions, namespace members and grouped API. `functions.html` is a valid alphabetical class-members index with letter pages (`functions_a.html` … `functions_~.html`). | **Letter-page structure confirmed; members not enumerated in this artifact.** |
| Python API 2.0 | [2027 package index](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html); [2026 package index](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/py_ref/namespaces.html) | Autodesk package index exposes `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI` (rendered by page extraction as `NOpenMaya...`). Each package's `namespace_open_maya*.html` page lists its classes and functions, and `py_ref/annotated.html` exposes every class row in HTML. | **Package names and retrievable class/members indexes confirmed; class/member symbols not transcribed in this artifact.** |
| Python API 1.0 | [Maya scripting/API overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Scripting/files/GUID-49701DA7-9224-424D-93F7-EC1DA779C99C.htm); [Developer Help entry](https://help.autodesk.com/view/MAYADEV/2027/ENU/) | Autodesk distinguishes `maya.OpenMaya` (API 1.0) from `maya.api.OpenMaya` (API 2.0), and `maya.cmds`; PyMEL is third-party, not Autodesk-supported. | **No standalone API 1.0 symbol index enumerated or verified.** Do not infer its inventory from API 2.0. |
| Release deltas | [2027.0](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_0.html), [2027.1](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_1.html), [2027.2](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_2.html); [2026.1](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_1.html), [2026.2](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_2.html) | Published C++ API change pages for listed updates. | **Partial delta review only; not a full command/node/Python diff.** No 2026.0 changelog verified at expected URL. |

## Observed symbol data

This section deliberately reports a narrow observed set, not a claim to have
transcribed the catalog. The 2027 class-list extraction showed, among many
others, the following exact class identifiers:

`MAnimUtil`, `MFnDistanceManip`, `MFnPlugin`, `MPreviewRenderEnvironment`,
`MPxShaderOverride`, `MRenderItem`, `MRenderOverride`, `MRenderUtilities`,
`MSceneRender`, `MItMeshEdge`, `MExternalDropData`,
`MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`,
`MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct`.

The 2027.0 change page explicitly lists:

- `MAnimUtil::AnimLayerFilter`,
  `createAnimCurve`, `findActiveAnimationLayer`, `findAnimationLayers`,
  `findAnimCurve`, `findBlendNodeInputPlug`.
- `MFnDistanceManip::isSnapModeOn`, `setSnapMode`.
- `MFnPlugin::deregisterFileDialog`, `registerFileDialog`.
- Class `MPreviewRenderEnvironment`.
- `MPxShaderOverride::supportsNonTextured`, `MRenderItem::ufePathString`,
  `MRenderOverride::colorManagedOperation`.
- `MRenderUtilities::createPreviewRenderEnvironment`,
  `getIBLPresetPath`; `MSceneRender::pluginDisplayFilterExclusions`.

Previously recorded 2027 delta observations (see roadmap; not a complete
symbol-by-symbol review):

- 2027.1 adds sequencer drawing/editing/build-menu APIs and manipulator matrix
  APIs. The class-list extraction visibly identifies the four sequencer names
  above; the full signatures/members are not transcribed here.
- 2027.2 adds `MItMeshEdge::getConnectedFaces` and
  `MItMeshEdge::getIncidentFaces`.
- 2026.1 adds `MDGContext` managed/timed queries, mesh blind-data
  component-ID support, and prioritized idle command execution.
- 2026.2 adds `MExternalDropData::formatData`.

These are release-note/catalog observations, not proof of runtime availability,
source/ABI compatibility, or equivalent behavior in Maya 2026.

## Coverage by API surface and family

“Retrievable, not transcribed” means the official catalog page exposes the names
but this artifact does not yet contain a counted name list; it does not mean the
catalog is empty or fully audited. Counts remain **unknown** until a crawl is
produced and hashed.

| Surface | Families implicated (candidate mapping) | Names/counts in this artifact | 2027.0/1/2 vs 2026 status |
|---|---|---|---|
| MEL / `maya.cmds` | All 14; especially SCN, MOD, CRV, UVE, ANM, RIG, MAT, RND, INT, DYN, PIP | Names retrievable from `index_all.html` (hundreds of anchors observed); not transcribed here. Flags, query/edit/create modes and result contracts also uncrawled. | No command-level release diff performed. |
| Nodes | All 14; cross-cutting DG nodes; especially SCN, MOD, ANM, RIG, MAT, RND, DYN | Names retrievable from `Nodes/index_all.html` (hundreds of anchors observed); not transcribed here. No attributes, defaults, classifications, affects/connections or version annotations crawled. | No node-level release diff performed. |
| C++ API classes and members | SHL, SCN, VPT, MOD, CRV, UVE, ANM, RIG, MAT, RND, INT, SCR, DYN, PIP | `annotated.html` exposes class rows in HTML; `functions*.html` letter pages expose members. Partial observed names above; total class/member count not transcribed. `MHWRender`, UI, DG/DAG, function sets, iterators and plug-in extension APIs visibly occur in catalog. | Partial official C++ changelog observations above; exhaustive 2027/2026 diff not done. |
| C++ namespaces/free functions/modules | Potentially all families | Index pages confirmed; not transcribed; count unknown. | Not compared. |
| Python API 2.0 | All 14 | Four package names: `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI`. `py_ref/annotated.html` exposes per-package class rows in HTML; not transcribed here; leaf members/counts unknown. | Not compared. |
| Python API 1.0 (`maya.OpenMaya`) | All 14 | Autodesk overview confirms surface exists; no leaf symbol index or enumeration here. | Not compared; API 2.0 must not be used as proxy. |
| Runtime compatibility / ABI / plug-in loading | All 14, especially SCR and PIP | No claims established. No Maya 2026 executable/devkit or component manifest tested. | Unverified independently of symbol enumeration. |

### 14-family disposition for the catalog crawl

Every family has API exposure to inspect; no family has a complete symbol
inventory in this artifact.

| Family | Relevant likely catalog domains (candidate, not an exhaustive mapping) | Enumeration |
|---|---|---|
| SHL | C++ UI, panels, controls, contexts, editor APIs; commands/nodes | Not done |
| SCN | Commands, DG/DAG/data/function sets, file/reference APIs, nodes | Not done |
| VPT | `MHWRender`, view, selection, manipulator and tool APIs | Not done |
| MOD | polygon commands/nodes, mesh function sets/iterators | Not done |
| CRV | curve/surface commands/nodes/function sets/iterators | Not done |
| UVE | UV commands/nodes, mesh UV members, texture editor extensions | Not done |
| ANM | animation commands/nodes, animation API, sequencer/editor APIs | Not done |
| RIG | joint/IK/skin/deformer commands/nodes/function sets | Not done |
| MAT | shader/light/texture commands/nodes and rendering APIs | Not done |
| RND | render commands/nodes, `MHWRender`, renderer/override APIs | Not done |
| INT | file commands, translators, file I/O and external-content APIs | Not done |
| SCR | all command and API catalogs; plug-in lifecycle and `MPx*` surfaces | Not done |
| DYN | simulation commands/nodes and component/plug-in APIs | Not done |
| PIP | file/resolver/evaluation/profiling/batch-related commands and APIs | Not done |

## Remaining work and auditable completion criteria

There is no defensible numeric “remaining symbol count” in this artifact yet:
the catalog names are retrievable from `Commands/index_all.html`,
`Nodes/index_all.html`, Doxygen `annotated.html`/`functions.html` letter pages
and `py_ref/annotated.html`/`namespacemembers.html`, but none have been
transcribed into a counted list here. Therefore the concrete remaining limit is
**all leaf symbols and total counts for every surface**, except the four Python
API 2.0 package names and the explicitly observed C++ delta names. Do not
substitute a guessed count.

To turn this into an exhaustive inventory:

1. Capture timestamped Autodesk index and leaf-page snapshots for both 2027.2
   and the exact 2026 documentation update selected for acceptance. Record
   retrieval status, canonical URL, page title, release, and a content hash.
   Fetch frameset children such as `Commands/index_all.html` and
   `Nodes/index_all.html` directly; the wrapper frameset pages are not the
   catalogs.
2. Enumerate every command name from `Commands/index_all.html` for both
   releases, then each command's flags, modes, return
   values, version notes and referenced UI workflow. Keep MEL and
   `maya.cmds` exposure distinct where behavior differs.
3. Enumerate every node type from `Nodes/index_all.html` for both releases and
   its attributes, defaults, data types,
   classifications, affects/connections, availability/version notes.
4. Enumerate C++ classes, namespaces, modules, free functions and each
   documented member/signature from `annotated.html`, `namespaces.html`,
   `modules.html` and the `functions*.html` letter pages, including inherited
   members and deprecation / introduction notes. Retain the source page for
   each row.
5. Crawl all Python API 2.0 package, class and member pages (`annotated.html`,
   `namespacemembers.html`, per-package `namespace_*.html`). Separately
   locate/crawl the official API 1.0 reference; if Autodesk publishes no
   browsable index, record that limitation and use only an explicitly
   authorized, version-matched official installed-doc source—never infer from
   API 2.0.
6. Assign each leaf to one primary family and any cross-family references;
   label unclassified entries explicitly. Compare release snapshots by stable
   symbol identifier and track additions/removals/signature/documentation
   changes separately.
7. Keep symbol enumeration, documented semantics, Maya runtime tests, component
   availability and ABI/plugin compatibility as independent evidence fields.

**Genuinely complete here:** source registry for the named official entry-point
catalogs, including the frameset-child catalogs and Doxygen letter/members pages
that make full name enumeration mechanically possible; observed Python API 2.0
package names; a small explicit C++ class and 2027.0 delta sample; family-by-family
statement that enumeration remains open; and the boundary that catalog presence
is not compatibility. **Not complete:** any transcribed command, node, C++ or
Python symbol list in this artifact, exact catalog totals, leaf-page
flags/attributes/members, full cross-version diff, or runtime compatibility
audit.
