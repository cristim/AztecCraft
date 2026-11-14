# Maya API catalog inventory (2027.2 lens; 2026 acceptance)

**Status: name-level catalog snapshot with open leaf work — not an exhaustive
API inventory.** This file records what can be established from Autodesk's
public catalogs (now including crawled 2027/2026 name lists), explicitly tracks
the per-symbol leaf work still required, and does not assert that AztecCraft
implements, loads, or is compatible with any Maya API. Catalog membership is
not runtime compatibility.

## Scope and method

- Documentation lens: Maya 2027.2, with the 2027.0 and 2027.1 API catalogs
  where versioned changelogs are available. Retrieval date: 2026-11-14.
- Acceptance remains the pinned Maya 2026 target; a 2027 catalog or runtime
  cannot silently expand or satisfy that target.
- Sources below are Autodesk Help / Developer Help only. The name-level
  snapshots in `maya-2027-api/` record their exact source URL and retrieval
  timestamp in each file header; leaf-page details have not been crawled or
  archived.
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
| MEL / `maya.cmds` commands | [2027 Commands index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index.html); [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/index.html) | Official command reference entry point. The wrapper page is a frameset whose child `Commands/index_all.html` carries the alphabetized command catalog; a direct fetch of that child exposes every linked command anchor. | **Name list crawled:** 2027: 1528, 2026: 1524 (`commands.txt`, `commands-2026.txt`). The list mixes core commands and MEL scripts, as the catalog does. Flags, modes, results and notes on the leaf pages remain uncrawled. |
| Dependency nodes | [2027 Node reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index.html); [2026 index](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/index.html) | Official node reference entry point. Its frameset child `Nodes/index_all.html` carries the alphabetized node catalog, including node names and "Updated" markers. | **Name list crawled:** 2027: 944, 2026: 943 (`nodes.txt`, `nodes-2026.txt`). Attributes, defaults and classifications on leaf pages remain uncrawled. |
| C++ API classes | [2027 class list](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html); [2026 class list](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/annotated.html) | Official Doxygen class catalog. The raw `annotated.html` HTML exposes every class/namespace row (hundreds of anchors, including nested namespaces); text-only extraction collapses it and must use HTML parsing. 2027 class page visibly spans Maya API, `MHWRender`, function sets, iterators, callbacks, plug-in base classes and other domains. | **Name list crawled:** 2027: 639, 2026: 633 (`cpp-classes.txt`, `cpp-classes-2026.txt`). Rows mix namespace entries with class entries, mirroring the Doxygen page. Per-class members/signatures remain uncrawled. |
| C++ namespaces/modules/members | [2027 namespaces](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/namespaces.html); [modules](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/modules.html); [member index](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/functions.html) | Separate indexes needed in addition to class names to cover free functions, namespace members and grouped API. `functions.html` is a valid alphabetical class-members index with letter pages (`functions_a.html` … `functions_~.html`). | **Letter-page structure confirmed; members not enumerated in this artifact.** |
| Python API 2.0 | [2027 package index](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html); [2026 package index](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/py_ref/namespaces.html) | Autodesk package index exposes `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI` (rendered by page extraction as `NOpenMaya...`). Each package's `namespace_open_maya*.html` page lists its classes and functions, and `py_ref/annotated.html` exposes every class row in HTML. | **Name list crawled:** 2027: 313, 2026: 310 (`py2-classes.txt`, `py2-classes-2026.txt`). Rows mix the package names with class entries. Per-class members/functions remain uncrawled. |
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

## Machine-readable catalog snapshots

`maya-2027-api/` holds first-pass name inventories plus the crawl script:

| File | Source page | Symbols |
|---|---|---|
| `commands.txt` / `commands-2026.txt` | `Commands/index_all.html` of 2027 / 2026 | 1528 / 1524 |
| `nodes.txt` / `nodes-2026.txt` | `Nodes/index_all.html` of 2027 / 2026 | 944 / 943 |
| `cpp-classes.txt` / `cpp-classes-2026.txt` | Doxygen `cpp_ref/annotated.html` of 2027 / 2026 | 639 / 633 |
| `py2-classes.txt` / `py2-classes-2026.txt` | Doxygen `py_ref/annotated.html` of 2027 / 2026 | 313 / 310 |
| `crawl-catalogs.ps1` | regenerates all eight lists with retrieval timestamps | — |

Method and limitations:

- Each file is one symbol per line with a `#` header recording the exact source
  URL, retrieval timestamp and count. `crawl-catalogs.ps1` reproduces the lists;
  rerunning it updates the headers, so treat the checked-in snapshot plus its
  header as the evidence pair. Lists preserve the source page order: commands
  and nodes are alphabetical, while the Doxygen lists intentionally contain
  order breaks where namespace rows group their child classes with the parent.
- Commands and nodes lists contain every catalog anchor, mixing core commands
  with MEL-only script commands; C++ and Python lists mix namespace rows with
  class rows, as the Doxygen pages do.
- These are **documentation-catalog snapshots of the live online help**, not
  the complete leaf inventory: flags, attributes, member signatures, version
  annotations and option semantics still require the per-symbol leaf pages.
- Name-list membership is not runtime existence, and a symbol absent from one
  release's online catalog is not proof it was absent in that shipped runtime.

First-pass cross-version name diff (2026 catalog → 2027 catalog, same anchors,
retrieved 2026-11-14):

| Surface | 2027-only names in catalog | 2026-only names |
|---|---|---|
| Commands | `cameraSequencer`, `cameraSequencerCurrentTimeCtx`, `polySmartBevel`, `traxAudioTrack` | none |
| Nodes | `shotLabel` | none |
| C++ classes | `MPreviewRenderEnvironment`, `MPxFileDialog`, `MSequencerDrawPrimitive`, `MSequencerDrawPrimitives`, `MSequencerEditPrimitiveFct`, `MSequencerPrimitiveBuildMenuFct` | none |
| Python API 2.0 classes | `MPreviewRenderEnvironment`, `MSequencerCustomDrawManager`, `MSequencerDrawPrimitive` | none |

Notes: `MPxFileDialog` (C++) and the sequencer/preview additions are consistent
with the published 2027.0/2027.1 changelog deltas; `polySmartBevel` matches the
Smart Bevel workflow documented for 2027. Commands such as `cameraSequencer` /
`traxAudioTrack` appearing only in the 2027 catalog while 2027 marks other
sequencer commands "Updated" suggests catalog-page reorganization rather than
new functionality, and needs per-command verification before use as a version
claim. This diff compares the live online catalogs; the served 2026 pages may
already include 2026.1/2026.2-era content, and neither list is a runtime
inventory of any installed Maya.

## Coverage by API surface and family

“Retrievable, not transcribed” means the official catalog page exposes the names
but this artifact does not yet contain a counted name list; it does not mean the
catalog is empty or fully audited. Counts remain **unknown** until a crawl is
produced and hashed.

| Surface | Families implicated (candidate mapping) | Names/counts in this artifact | 2027.0/1/2 vs 2026 status |
|---|---|---|---|
| MEL / `maya.cmds` | All 14; especially SCN, MOD, CRV, UVE, ANM, RIG, MAT, RND, INT, DYN, PIP | 1528 (2027) / 1524 (2026) names crawled. Flags, query/edit/create modes and result contracts uncrawled. | Name-level diff done (see snapshots); flag/behavior diff not performed. |
| Nodes | All 14; cross-cutting DG nodes; especially SCN, MOD, ANM, RIG, MAT, RND, DYN | 944 (2027) / 943 (2026) names crawled. No attributes, defaults, classifications, affects/connections or version annotations crawled. | Name-level diff done (see snapshots); attribute/behavior diff not performed. |
| C++ API classes and members | SHL, SCN, VPT, MOD, CRV, UVE, ANM, RIG, MAT, RND, INT, SCR, DYN, PIP | 639 (2027) / 633 (2026) class/namespace names crawled. Per-class members/signatures uncrawled. `MHWRender`, UI, DG/DAG, function sets, iterators and plug-in extension APIs visibly occur in catalog. | Name-level diff done (see snapshots); partial official C++ changelog observations above; member-level diff not done. |
| C++ namespaces/free functions/modules | Potentially all families | Index pages confirmed; members not enumerated; count unknown. | Not compared. |
| Python API 2.0 | All 14 | Four package names: `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, `OpenMayaUI`. 313 (2027) / 310 (2026) class/namespace names crawled; leaf members/counts unknown. | Name-level diff done (see snapshots); member-level diff not performed. |
| Python API 1.0 (`maya.OpenMaya`) | All 14 | Autodesk overview confirms surface exists; no leaf symbol index or enumeration here. | Not compared; API 2.0 must not be used as proxy. |
| Runtime compatibility / ABI / plug-in loading | All 14, especially SCR and PIP | No claims established. No Maya 2026 executable/devkit or component manifest tested. | Unverified independently of symbol enumeration. |

### 14-family disposition for the catalog crawl

Every family has API exposure to inspect; name-level lists exist for all
families via the four snapshots, while no family has leaf-level detail or a
complete semantic inventory in this artifact.

| Family | Relevant likely catalog domains (candidate, not an exhaustive mapping) | Enumeration |
|---|---|---|
| SHL | C++ UI, panels, controls, contexts, editor APIs; commands/nodes | Names crawled; leaf details open |
| SCN | Commands, DG/DAG/data/function sets, file/reference APIs, nodes | Names crawled; leaf details open |
| VPT | `MHWRender`, view, selection, manipulator and tool APIs | Names crawled; leaf details open |
| MOD | polygon commands/nodes, mesh function sets/iterators | Names crawled; leaf details open |
| CRV | curve/surface commands/nodes/function sets/iterators | Names crawled; leaf details open |
| UVE | UV commands/nodes, mesh UV members, texture editor extensions | Names crawled; leaf details open |
| ANM | animation commands/nodes, animation API, sequencer/editor APIs | Names crawled; leaf details open |
| RIG | joint/IK/skin/deformer commands/nodes/function sets | Names crawled; leaf details open |
| MAT | shader/light/texture commands/nodes and rendering APIs | Names crawled; leaf details open |
| RND | render commands/nodes, `MHWRender`, renderer/override APIs | Names crawled; leaf details open |
| INT | file commands, translators, file I/O and external-content APIs | Names crawled; leaf details open |
| SCR | all command and API catalogs; plug-in lifecycle and `MPx*` surfaces | Names crawled; leaf details open |
| DYN | simulation commands/nodes and component/plug-in APIs | Names crawled; leaf details open |
| PIP | file/resolver/evaluation/profiling/batch-related commands and APIs | Names crawled; leaf details open |

## Remaining work and auditable completion criteria

There is a defensible name-level baseline now, but not a leaf-symbol inventory:
`maya-2027-api/` holds crawled 2027 and 2026 name lists for commands, nodes,
C++ and Python API 2.0 classes with counts and source headers. The concrete
remaining limit is **every leaf detail for every listed symbol** — command
flags/modes/results, node attributes/defaults/classifications, C++/Python
members/signatures, plus C++ namespaces/free functions, Python package
functions and a verified Python API 1.0 catalog. Do not substitute a guessed
count for the uncrawled leaf work.

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
catalogs including frameset-child catalogs and Doxygen letter/members pages;
crawled 2027 and 2026 name snapshots for commands, nodes, C++ classes and
Python API 2.0 classes with counts, headers and a name-level cross-version diff;
observed Python API 2.0 package names; a small explicit C++ delta sample;
family-by-family statement that leaf enumeration remains open; and the boundary
that catalog presence is not compatibility. **Not complete:** per-symbol leaf
detail (flags/attributes/members), C++ namespace/member and Python
package-function inventories, a verified Python API 1.0 catalog, semantic or
behavioral diffs, or any runtime compatibility audit.
