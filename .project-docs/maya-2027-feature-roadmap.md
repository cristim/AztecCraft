# Maya 2027 feature and API roadmap for AztecCraft

Research baseline: Autodesk Maya 2027.2 online Help and Developer Help,
consulted 2026-11-14. This is a product-scope and implementation roadmap, not a
claim that every command and plug-in API has been individually audited. The
candidate denominator remains all Maya 2026 functionality, including simulation,
rendering, scripting and existing plug-in compatibility. The 2027.2 pin is a
current documentation lens only; it does not replace that acceptance target.
See [reference.md](reference.md), [parity.md](parity.md), and the
[14-family inventory framework](parity-inventory/README.md).
The artist-facing menu hierarchy and non-menu interaction surfaces are mapped
in [maya-2027-menu-map.md](maya-2027-menu-map.md).

## Research findings and evidence boundaries

- Autodesk's [Maya 2027 Help](https://help.autodesk.com/view/MAYAUL/2027/ENU/)
  documents artist workflows and UI. The [main menus and menu sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-3DB1C040-051F-4C2A-83F8-D806455FCEFF.htm)
  establish seven always-available menus and five workflow menu sets:
  Modeling, Rigging, Animation, Dynamics (also referred to as FX in workflow
  material), and Rendering. Family-specific placement,
  option boxes and documented hotkeys must be recorded in the corresponding
  inventory rows, not inferred from the current AztecCraft UI.
- The [Maya 2027 Developer Help](https://help.autodesk.com/view/MAYADEV/2027/ENU/)
  is the developer entry point. Its major compatibility surfaces are the
  [MEL/Python-wrapped command reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index.html),
  [node reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index.html),
  [C++ API class list](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html),
  [Python API 2.0 package reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html),
  and Maya's [Python library/API overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Scripting/files/GUID-49701DA7-9224-424D-93F7-EC1DA779C99C.htm).
  The Python overview distinguishes `maya.cmds`, Python API 1.0
  (`maya.OpenMaya`) and Python API 2.0 (`maya.api.OpenMaya`), and identifies
  PyMEL as third-party and unsupported by Autodesk. The C++ reference exposes
  scene/DAG/DG, mesh, animation, rendering, UI and plug-in extension APIs; this
   breadth is not evidence that AztecCraft can load Maya binaries or extensions.
- **Catalog audit status:** the official indexes are enumerable, but this
  roadmap is not yet a symbol-by-symbol compatibility inventory. Crawl the
  [command index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/index.html)
  and record command modes, flags, results and version notes; crawl the
  [node index](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/index.html)
  and record node types, attributes, defaults, connections and classifications.
  For C++, enumerate the [class list](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/annotated.html),
  [namespaces](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/namespaces.html),
  [modules](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/modules.html),
  examples and member pages. For Python API 2.0, enumerate each package in the
  [package index](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/namespaces.html):
  `OpenMaya`, `OpenMayaAnim`, `OpenMayaRender`, and `OpenMayaUI`. Build a
  **separate** Python API 1.0 inventory (`maya.OpenMaya`); do not infer it from
  API 2.0. Track source URL/snapshot, symbol/signature, family, component and
  version, documentation evidence, and runtime-test status, then diff catalog
  snapshots. Enumeration alone establishes neither behavior nor compatibility.
- Explicit API domains to disposition include plug-in registration/lifecycle
  (`MFnPlugin`, `MPx*`), file I/O and translators (`MFileIO`,
  `MFileObject`, `MPxFileTranslator`), external content/drop, assemblies,
  callbacks, evaluation, device/input, UI, drawing/rendering (`MHWRender`),
  animation, data/function sets, and standalone/library loading. These are
  candidate domains, not a claim that every symbol is in AztecCraft's support
  scope. Separate Maya-core, bundled/separately versioned component,
  third-party, and AztecCraft-only extension support.
- Official C++ update deltas found so far:
  [2027.0](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_0.html),
  [2027.1](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_1.html),
  [2027.2](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_2.html),
  [2026.1](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_1.html),
  and [2026.2](https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/changelog_2026_2.html).
  2027.0 adds animation-layer utilities, distance-manipulator snap methods,
  plug-in file-dialog registration, and preview/render APIs; 2027.1 adds
  manipulator matrix and sequencer drawing/editing/build-menu APIs; 2027.2 adds
  `MItMeshEdge::getConnectedFaces` and `getIncidentFaces`. 2026.1 adds
  `MDGContext` managed/timed queries, mesh blind-data component-ID support, and
  prioritized idle command execution; 2026.2 adds `MExternalDropData::formatData`.
  No 2026.0 changelog was found at the expected URL; this is not evidence that
  the release had no API changes. These are C++ deltas only: separately diff
  Python 1.0/2.0, commands and nodes.
- The official [2027.2 What's New](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-49B18BA5-F029-4395-83B5-8B216DBE5914.htm)
  confirms update-specific items including Graph Editor lock/mute-button
  visibility, MotionMaker model frame-rate handling, and OpenColorIO 2.5.2.
  The [2027.2 release notes](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2027_2_HTML.html)
  include fixes and known issues, not a complete feature contract. These
  2027-specific observations are candidates for the documentation inventory;
  they do not silently change the 2026 acceptance set.
- Autodesk's [included plug-in list](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Customizing/files/GUID-DF9366D9-E24B-4A08-88F1-8EE1B78A52F9.htm)
  identifies distinct translators, renderers, simulation systems, modeling
  tools and APIs (including FBX, Alembic, USD, Bifrost, Bullet, Arnold-related
  integrations, XGen, HumanIK, MASH and others). Presence in the list does not
  establish installed version, loaded state, entitlement, or compatibility.
- Pages retrieved here establish the source catalog and representative
  behaviors. A full page-by-page audit of all command flags, node attributes,
  API methods, bundled components and UI option defaults remains a required
  denominator-freeze task. Any detail absent from an individually reviewed
  source must be labelled `PROVISIONAL`, not filled from memory or a different
  release.
- Online references cannot prove runtime availability, undocumented defaults,
  evaluation/numerical behavior, plug-in loadability or ABI compatibility.
  Those require the exact Maya 2026 build and relevant plug-ins; C++ compile/link
  claims additionally require its matching devkit and platform toolchain.

## Product and compatibility rules

1. Keep AztecCraft a native Rust desktop editor. Model scene operations in a
   widget-independent core; native UI and opt-in MCP dispatch the same typed
   operations against one authoritative `NativeScene`.
2. Preserve Maya data rather than approximating it: hierarchy, DAG paths and
   instancing, DG connections, animation, unknown nodes/data, native numeric
   precision, and untouched source values. Refuse an edit or save when
   preservation cannot be demonstrated. Maya ASCII and Binary Open/Save/Save
   As remain primary; JSON is compatibility/migration input, not Maya format
   support.
3. Treat Maya commands, nodes, APIs, scripts and plug-ins as separate
   compatibility goals. A similar UI action, a new AztecCraft-only plug-in
   API, or parsing a file does not establish compatibility with existing Maya
   scripts or plug-ins.
4. Keep imported scripts, expressions and plug-in payload inert by default.
   No scene-driven code execution, network service, paid service request or
   automatic MCP listener. Any execution boundary needs a separately reviewed
   threat model, explicit user opt-in and compatibility tests.
5. A user-facing capability is incomplete until its actual implemented
   operation/state is exposed by synchronized MCP schema, real Editor protocol
   tests, and coverage documentation. No dummy or aspirational tools.
6. Feature completion means documented behavior is implemented and tested.
   Maya parity additionally requires pinned-reference comparisons. Do not
   report a coverage percentage until the complete Maya 2026 denominator and
   measurement rule are agreed and frozen.

## Complete candidate feature scope

The family files are the row-level source of truth and eventual audited
denominator. Entries below are planning domains, not scored rows; the current
inventory explicitly says several families are empty or partial. Each feature
slice must research Maya 2027.2 behavior, UI placement, defaults/options,
hotkeys and edge cases, and retain 2026-only/newer-version distinctions.

## Required feature categorization

Classify every candidate workflow along the axes below before sequencing or
implementation. A feature can belong to more than one technical/API category,
but it has one primary workflow family and one owner row. This prevents, for
example, a menu item from being mistaken for an implemented operation, or a
plug-in-dependent capability from being treated as Maya-core behavior.

| Axis | Values to record |
| --- | --- |
| Product family | One primary inventory family: SHL, SCN, VPT, MOD, CRV, UVE, ANM, RIG, MAT, RND, INT, SCR, DYN or PIP. Cross-family dependencies are references, not duplicate owners. |
| Feature category | User workflow/tool; editor/panel; scene data/model; operation/command; format/translator; evaluator/solver; renderer; scripting/extension; pipeline/service. Add a more specific subcategory such as mesh topology, UV mapping, key editing, skin influences, or cache management. |
| User task and UI | Testable imperative task, main menu/menu set and exact submenu, option box, shelf/Tool Box/Status Line, panel/editor, context/marking menu or hotkey. If official Help does not specify a location or default, mark `PROVISIONAL`. |
| Technical/API surface | MEL command, `maya.cmds`, node type/attributes/connections, Python API 1.0, Python API 2.0, C++ API, file translator, UI/editor extension, or external CLI/service. Record exact command flags, node attributes or API symbols in the audited family row. |
| Runtime/component scope | Maya core; bundled plug-in (name/version); separately versioned Autodesk component; third-party/vendor extension; cloud/service/entitlement; or AztecCraft-only implementation. Record the pinned version and load/license assumptions. |
| State and persistence | Authored scene state; source-preserved/unknown state; derived/evaluated state; session-only interaction state; user preference/workspace state; or external/cache/sidecar data. State what Save/Open, undo and interchange are required to preserve. |
| Version and scope | Maya 2026 acceptance candidate; `2026-only`; `2027-new`; update-specific; version fallback; or unresolved scope. A 2027-only feature does not silently enter the 2026 acceptance denominator. |
| Delivery and evidence | One of the nine playable milestones, dependencies, inventory status, cited Autodesk page/symbol, AztecCraft implementation/test evidence, Maya runtime comparison status, and any `PROVISIONAL` questions. |

Keep the existing row-status vocabulary and evidence rules in
[parity-inventory/README.md](parity-inventory/README.md). This categorization is
an organization and planning aid, not an alternative scoring scheme; it does
not authorize percentages or exclusions.

| Order / family | Planned user workflows and API surface | Primary UI/source anchors |
| --- | --- | --- |
| 1. Application shell (`SHL`) | Menus/menu sets, shelves, hotbox and marking menus, hotkeys, Tool Box, Status Line, panel layouts and docking, workspaces/preferences, Outliner, Channel Box, Attribute Editor, Script Editor/Command Line, editors and help. | [Menus and sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D90A2BDB-FD05-4528-8A95-C33A02D15129.htm); [UI inventory](ui-parity-inventory.md); [01 shell](parity-inventory/01-application-shell.md). |
| 2. Scene management (`SCN`) | New/open/save/save-as, dirty-state prompts, undo/redo/repeat, hierarchy, groups, parenting, namespaces, sets, layers, attributes, connections, references/proxies, instancing, scene units, projects/paths, selection rules, node evaluation and scene assembly. Preserve `.ma` and `.mb` sources losslessly where possible. | [02 scene management](parity-inventory/02-scene-management.md); Maya-Tech-Docs `file`, `parent`, `group`, `sets`, `select`, `undo`, `currentUnit`, reference and node commands. |
| 3. Viewport and interaction (`VPT`) | Perspective/orthographic cameras, navigation, panel configuration, object/component selection and masks, marquee/soft selection, manipulators, pivots, snapping, grids/guides, display layers, HUD, image planes, shading modes, viewport 2.0 and custom draw/selection extensions. | [Viewport menu map](parity-inventory/README.md#top-level-menu-to-family-map); [03 viewport](parity-inventory/03-viewport.md); C++ `MHWRender`, `M3dView`, selection and context APIs. |
| 4. Polygon modeling (`MOD`) | Primitive creation/options; component selection; transforms; extrude, bevel/Smart Bevel, bridge, append, insert edge loop, Multi-Cut, merge/weld, combine/separate/extract, booleans, mirror/symmetry, smooth/subdivision/creases, normals, cleanup, sculpt/soft selection, retopology/Quad Draw, construction history and mesh conversion. | Modeling menu set, Modeling Toolkit, polygon options and [04 polygon modeling](parity-inventory/04-polygon-modeling.md); command/node references (`poly*`, mesh/poly nodes, `MFnMesh`). |
| 5. Curves and surfaces (`CRV`) | NURBS/Bezier curve creation and editing, CVs/knots/degree, rebuild/attach/detach/trim, loft/revolve/extrude/planar, NURBS surfaces, tessellation and polygon conversion; preserve parameterization/history. | Modeling > Curves/Surfaces; [05 curves and surfaces](parity-inventory/05-curves-and-surfaces.md); curve/surface commands, nodes, `MFnNurbsCurve`, `MFnNurbsSurface`. |
| 6. UV editing (`UVE`) | Projection, automatic mapping, cut/sew, unfold/relax, pinning, layout/packing, UV sets/UDIM, UV Editor interactions, texture painting and texture persistence. | Modeling > UV and UV Editor; [06 UV editing](parity-inventory/06-uv-editing.md); UV commands/nodes, mesh UV APIs and UV editor extensions. |
| 7. Animation (`ANM`) | Time/range/playback, keys and Auto Key, tangent/infinity editing, Graph Editor, Dope Sheet, Time Editor clips/layers, animation layers, motion paths, constraints/baking, camera sequencing, audio/playblast and MotionMaker when in the pinned update. | Animation menu set; [07 animation](parity-inventory/07-animation.md); commands/nodes and `MFnAnimCurve`, time, sequencer/graph-editor APIs. |
| 8. Rigging/deformation (`RIG`) | Joints/skeletons, orientation, IK/FK, constraints, HumanIK and retargeting, skin binding/weight paint and transfer, blend shapes, deformers (lattice, nonlinear, wrap, wire), driven keys, muscles and rig evaluation. | Rigging menu set; [08 rigging](parity-inventory/08-rigging.md); joint/IK/skin/deformer commands and nodes, `MFnIk*`, `MFnSkinCluster`, deformer plug-in APIs. |
| 9. Materials and lighting (`MAT`) | Shading networks and assignments, OpenPBR/standard materials, textures and file nodes, color spaces/OCIO, UV/UDIM textures, lights, shadows, light linking, LookdevX/MaterialX, utility nodes and material editing UI. | Rendering > Lighting/Shading and Texturing; [09 materials and lighting](parity-inventory/09-materials-and-lighting.md); shader/light/texture nodes and API. |
| 10. Rendering (`RND`) | Viewport appearance, render setup/layers and overrides, cameras/stereo, Arnold interactive and batch workflows, Render View/sequence, AOVs, volumes, displacement, depth of field, motion blur, playblast and renderer plug-in contracts. Distinguish core Maya from versioned renderer components and entitlement. | Rendering menu set; [10 rendering](parity-inventory/10-rendering.md); `MHWRender`, renderer/override/render-pass APIs and Arnold 2027 docs. |
| 11. Interchange (`INT`) | Native Maya formats; import/export options and translators; OBJ, FBX, Alembic, USD/MayaUSD, caches, animation interchange, images/audio and supported legacy formats. Preserve unsupported data or refuse; test round trips and external application compatibility. | File menu and Cache menu; [11 interchange](parity-inventory/11-interchange.md); `MPxFileTranslator`, MayaUSD/FBX/Alembic translator docs. |
| 12. Scripting and extensions (`SCR`) | MEL execution/editor and command semantics; `maya.cmds` scripts and flags/errors; Python API 1.0/2.0; `mayapy`/standalone; callbacks, expressions, script nodes, Python/MEL plug-ins, C++ binary plug-ins, custom commands/nodes/tools/translators/render hooks and version/ABI behavior. | Script Editor, Command Line and Plug-in Manager; [12 scripting](parity-inventory/12-scripting-and-extensions.md); Developer Help command, node and C++ API references. PyMEL is third-party and not an Autodesk-supported compatibility target unless separately decided. |
| 13. Dynamics and extended systems (`DYN`) | nParticles/fields/solvers, nCloth/nConstraints, nHair, fluids, ocean/BOSS, Bullet, Bifrost graphs/compounds/simulation/cache/CLI, XGen legacy and interactive grooming, Paint Effects, MASH, caches and associated rendering. Pin independently versioned components, plug-in state and service/entitlement boundaries. | FX and Modeling > Generate menu sets; [13 dynamics](parity-inventory/13-dynamics-and-extended-systems.md); official plug-in list and component developer references. |
| 14. Pipeline and robustness (`PIP`) | Project/workspace and path resolution, references/assets, recovery/autosave, large-scene performance/evaluation/profiling, batch/headless operations, command-line tools, translators, packaging/platform support, plug-in discovery/version checks, error recovery and data integrity. | Windows/Preferences, file/reference workflows; [14 pipeline](parity-inventory/14-pipeline-and-robustness.md); `MFileIO`, resolver/translator APIs, evaluation manager and profiling APIs. |

## Feature-level work packages and dependencies

These packages make the family map actionable: each package is a set of
independently testable Maya workflows, not one broad “family feature”. Specific
command flags, node attribute schemas, API signatures and documented defaults
belong in the family rows during the denominator audit; this section does not
pretend those thousands of reference entries have already been transcribed.

| Package | Feature work packages | Depends on |
| --- | --- | --- |
| SHL — workflow shell | Application/menu-set switching and menu contexts; hotbox/marking menus, shelves, hotkeys and custom shortcuts; Status Line, Tool Box and Modeling Toolkit; quick layouts, multi-panel/workspace docking and preferences; Outliner search, hierarchy and contextual actions; Channel Box, Layer Editor and Attribute Editor; Script Editor/Command Line, Help Line and editor launch/close state. Autodesk [Interface overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm) gives the named shell surfaces and placements. | Scene/selection protocol and stable panel/editor state; not the scene model itself for layout-only settings. |
| SCN — scene lifecycle and graph | New/open/import/reference/save/export and prompts; `.ma`/`.mb` preservation; DAG parenting/groups/instances/namespaces; DG nodes, plugs and connections; sets, display/render layers, attributes and units; references/proxies/offline edits; selection rules and node evaluation; project paths, recovery, history/undo and scene metadata. Autodesk [File menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm), [Open Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-203FC12C-6C80-497C-AC29-548DF6EC91B5.htm) and [Import Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-37B31221-7BBC-40DA-AB33-47C333EF4E14.htm) expose lifecycle settings including inert-by-default handling requirements for script nodes. | Gate 1 authoritative graph, source binding and preservation. The graph/persistence layer is shared by every later family. |
| VPT — view and interaction | Perspective and orthographic panels; orbit/pan/dolly/roll, frame and camera switching; object, DAG hierarchy and vertex/edge/face/UV selection; marquee and drag selection, masks, soft selection and symmetry; move/rotate/scale/universal manipulators and pivot editing; snapping/alignment/construction planes; grid/HUD/display layers, isolate/template/visibility; viewport rendering, panel toolbar and context menus. Autodesk [Selection styles](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-1FD959BC-DF23-49F2-B17A-35C402D18402.htm) documents marquee versus drag workflows; [Panel menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-BD5D066A-4876-4B56-8A9F-96E2243E22A3.htm) documents per-panel controls. | SCN graph and selection; SHL panel/layout; renderer and camera model. |
| MOD — polygon modeling | Primitive creation and option boxes; component modes and loop/ring selection; extrusion/inset/bridge/append; bevel and Smart Bevel; cut/insert edge loops/Multi-Cut; merge/target weld/combine/separate/extract; booleans/mirror/symmetry; smooth/subdivision and creases; normals/hard edges/cleanup; sculpt/soft-selection/Quad Draw; construction history, conversion and topology validation. Autodesk [polygon component loops and rings](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-2ADFD0C1-05C5-42EC-90F8-3035A6B21D77.htm) and [subdivision history/options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-954CB56E-A40D-415B-A1D4-13EDFAED01D3.htm) provide concrete behavior anchors. | SCN graph/history, VPT component picking/manipulation, geometry kernel and ViewScene. |
| CRV — curves and surfaces | Polygon/NURBS/Bezier curve creation; CV/edit-point placement and selection; degree, knots, tangents and rebuild; attach/detach/trim/extend; loft/revolve/extrude/sweep/planar surfaces; surface edit, stitch/boolean/tessellate; Bezier↔NURBS and NURBS↔polygon conversions; curve/surface history. Autodesk [NURBS Modeling](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-735A0B9A-2180-4FB8-9A7B-68F21F306E97.htm) locates creation in Create and editing in Modeling; [Bezier/NURBS conversion](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-63EB1720-AACF-440D-8BBD-04240B2C0974.htm) names a conversion workflow. | SCN graph/history and VPT component selection; MOD conversion/mesh consumers. |
| UVE — UV and texture authoring | UV sets and current set; planar/cylindrical/spherical/automatic mapping; cut/sew and pinning; unfold/relax; shell transform, stacking and packing; texel density/UDIM; UV Editor selection, view and toolbar; 3D texture paint and save/reload. The candidate flow is anchored in the [UV menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-3100AC8D-C82C-4F17-A556-39760FEE91AC.htm) and [Unfold workflow](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-039FFDA5-3AC5-4CCF-A53E-546CCA6439FC.htm); verify every options page and default at freeze. | MOD mesh topology, VPT component/editor framework, MAT texture graph and SCN persistence. |
| ANM — time and animation | Time units/range/current time; set/move/copy/delete keys and Auto Key; tangent types/weights/infinity; Graph Editor and Dope Sheet selection/editing; animation layers and blend evaluation; Time Editor clips, clip layers and sequencing; motion paths, camera sequencing, audio, playback and playblast; cache/evaluation controls; MotionMaker update-specific workflows. Autodesk [Keys](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-68F66F69-1F76-4F58-A88A-0A58FBDE5350.htm), [animation layers](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-89535329-63E3-4CCA-AD1D-7AB27B2B59B9.htm), [Time Editor clip layers](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-B3FFE1AA-F44D-4EC3-A570-316A3BF3DF88.htm) and [Maya 2027.2 What's New](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-49B18BA5-F029-4395-83B5-8B216DBE5914.htm) anchor separate systems and the update's MotionMaker model-frame-rate workflow; do not collapse the animation systems into one layer model. | SCN time/graph persistence; SHL editors; RIG attributes/constraints; RND playblast/render consumers. |
| RIG — character and deformation | Joint placement, hierarchy, orientation, mirroring and display; IK handles/solvers, pole vectors and FK/IK; parent/point/orient/aim/other constraints; smooth/rigid bind, weight editing/normalization/transfer; blend shapes/in-betweens; lattice/wrap/wire/nonlinear/skin deformers; driven keys and node-based rig graphs; HumanIK/Quick Rig/retargeting; muscle and 2027 procedural-rig updates. Autodesk [smooth skinning](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-CharacterAnimation/files/GUID-11007900-329F-40ED-9C38-BE4DB5C39832.htm), [Bind Skin Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-CharacterAnimation/files/GUID-CF2C698A-44BB-4CA0-BCB9-DB36500DA812.htm) and [Maya 2027 What's New](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-EA529440-E7A6-48E6-B215-691CDD34320A.htm) anchor binding options and the update-specific eye module/rod solver; treat these new workflows as version-tagged candidates. | SCN DG evaluation, ANM time, MOD/CRV geometry and VPT tools. |
| MAT — materials and lighting | Base material/shader-node creation and assignment; graph editing and connections; non-textured color/parameter editing; OpenPBR/standard/renderer-specific shaders; utility and conversion nodes; lights, shadows, light-linking and light editor; MaterialX/LookdevX interchange; material Attribute Editor and previews. Textured materials, UV/UDIM coordinates and texture painting are UVE integration scope, not a prerequisite for base materials. | SCN DG/attributes and RND renderer/color management; UVE coordinates only for textured-material integration; INT for image/material interchange. |
| RND — rendering | Renderer selection and plug-in state; scene-wide settings/presets; Render Setup layers/collections/absolute-relative-connection overrides versus legacy layers; cameras/stereo; interactive Render View and sequences; Arnold/MtoA interactive, batch and license boundaries; AOVs, sampling, volumes, displacement, depth of field, motion blur; viewport renderer and playblast. Autodesk [Render Setup](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-646154F9-E4D2-4A3E-A0D9-B2F062960A4E.htm) states layers/collections/overrides, JSON setup reuse and the incompatibility of simultaneous legacy/new layer systems; [Arnold for Maya](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-AD1E74F3-471A-4B5D-8FB0-9E4EC6EC5252.htm) separates interactive and batch/license behavior. | MAT graph, SCN graph/evaluation, ANM time, VPT renderer, PIP batch/resource policy. |
| INT — file and pipeline interchange | Native Open/Save/Save As and format options; translators and plug-in-specific import/export settings; OBJ, FBX, Alembic, USD, STL and supported legacy translators; geometry, hierarchy, materials, cameras, lights, animation and cache data; reference versus import semantics; path/namespace/remap options; round-trip and unsupported-data reports. MayaUSD specifically documents [round-trip import/export](https://help.autodesk.com/cloudhelp/ENU/Maya-USD/files/USD-for-Maya/GUID-2755A1ED-4857-4111-8571-5D658EE90FA0.html) and Maya 2027 [Component Creator/Variant Manager](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-93ED1C8E-AF46-4F5A-BC07-8B66C7C3D32E.htm); the latter is version-tagged, not presumed part of 2026. | SCN preservation and translators; MAT/ANM/RIG data models; PIP path resolution. |
| SCR — command and extension compatibility | MEL interpreter/commands/procedures; Python `maya.cmds` syntax, flags, errors and selection/results; Python API 1.0/2.0 object/function-set operations; standalone/batch `mayapy`; callbacks and expressions; scene script nodes; Python/MEL/C++ plug-in lifecycle; custom commands/nodes/data/tools/deformers/translators/renderers; versioned API/ABI and serialized-node compatibility. Autodesk command, node and API catalogs are the enumeration sources; each symbol/flag still needs individually audited rows before claiming compatibility. | Threat-model approval plus stable SCN, undo, evaluator, translator and extension boundaries. Never execute untrusted scene content automatically. |
| DYN — simulation and procedural systems | Nucleus solver/time/cache; particles/emitters/fields/constraints; cloth/hair and collisions; legacy fluids and ocean/BOSS; Bullet rigid/soft bodies/constraints; Bifrost graph authoring, compounds, liquid/aero/RBD/particles, caches and `bifcmd`; XGen legacy/interactive grooms and caches; Paint Effects; MASH distribution/instancing/effects/dynamics; renderer and interchange handoff. Maya 2027 What's New documents [Bifrost 3 and RBD updates](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-EA529440-E7A6-48E6-B215-691CDD34320A.htm); component/plugin versions and licensing stay separate. | SCN DG/evaluation, ANM time, MOD/CRV geometry, MAT/RND output, INT caches and PIP resource scheduling. |
| PIP — production reliability | Projects/workspaces and asset search paths; references/proxies and nested load states; autosave/recovery and crash-safe staged persistence; dependency reporting/relocation; evaluation/profiling and large-scene responsiveness; command-line/batch jobs; plug-in inventory, load state and version/ABI diagnostics; platform packaging; actionable failures, cancellation and restart/recovery. Autodesk Open Options explicitly documents reference load-state modes and namespaces; each recovery/performance policy needs direct 2027 and 2026 evidence. | SCN file identity and preservation, all family resource bounds, host platform and build/package system. |

`PROVISIONAL`: exact UI location, menu/set membership, defaults, hotkeys,
option-box values, behavior not stated on the cited page, third-party/service
scope, and undocumented Maya Binary structure. Confirm through the pinned 2026
reference or retain as unresolved; a 2027 page alone is not proof of 2026
behavior.

## Playable milestone order

Delivery order follows the requested artist-facing sequence. Gates 0 and 1
below are prerequisites, not extra user-facing feature batches: establish the
safe authoritative scene and transaction/source-preservation groundwork while
retaining the existing playable baseline. Full Maya-native Open/Save/Import/
Export user workflows belong to Milestone 2, not to the pre-modeling
foundation. After that foundation, complete and verify each milestone as a
playable vertical slice before opening the next one.

| Milestone | Complete this slice before moving on |
| --- | --- |
| 1. Modeling and interaction | Make the Modeling menu set usable end-to-end: core primitives and mesh edits, component/object selection, undo, and the relevant tool/options panels. Add documented commands/options and the requested dial/marking/context menus only where Maya documents them. Keep unfinished tools visibly unsupported. |
| 2. Open/import and save/export | Complete Maya-native Open/Save/Save As first, then prioritized standard formats such as OBJ, FBX, Alembic and USD with format-specific options. Validate hierarchy, geometry, animation/material payloads as claimed, staged writes, round trips, and preservation/refusal of unknown data. |
| 3. Materials | Build and assign supported material/shading graphs, edit their attributes, persist/reopen them, and expose them consistently in UI, viewport/rendering and MCP. |
| 4. Texture and UV Editor | Implement UV sets/projection/editing, shell operations and editor interactions; then connect texture files, color spaces and material inputs. Save/reopen a representative textured scene. |
| 5. Lighting, cameras and rendering | Create/edit lights and cameras; add viewport/render appearance, render settings and a first real render workflow, including the relevant render-layer behavior. Keep component/version and license boundaries explicit. |
| 6. Key-based animation | Create and edit keys, evaluate transforms over time, scrub/play, and save/reopen animation. Then complete tangent/Graph Editor and timeline workflows before advertising broader animation support. |
| 7. Rigging with animation | Build joints/bones, orientation and hierarchy; bind geometry and edit/normalize influences; animate the rig through supported key workflows and preserve the result on save/reopen. The first completed scenario includes basic joint-driven skin influences. Add IK, constraints, other deformers, HumanIK and retargeting as individually completed follow-up slices. |
| 8. Physics and FX | Add dynamics and FX systems one component at a time (solver, simulation, cache, playback and renderer/interchange handoff); pin versions and keep unavailable plug-ins/services visibly unsupported. |
| 9. Scripting | Only after the execution threat model and compatibility boundary are approved, add MEL/Python command behavior, APIs and existing plug-in compatibility in bounded, opt-in slices. Inert inspection is not execution compatibility. |

**Keep the build playable throughout:** at each milestone, retain a small,
verified authoring journey that can be launched and used; finish the milestone's
workflow through save/reopen (where applicable), undo, error handling, native
visual checks and actual MCP protocol coverage before activating the next
feature group. A Maya-like control may be present for layout research, but if
its operation is not implemented it must be visibly disabled/unsupported—do
not enable every option at once or let a menu imply capability that is not
there. The exact “dial menu” meaning and placement are `PROVISIONAL` until
matched to the Maya 2026 reference; Autodesk documentation calls out marking
menus and context menus and their specific interaction behavior.

## Sequenced delivery plan

The nine playable milestones above define artist-facing delivery order. The
technical gates below establish prerequisites and verification; they do not
authorize activating later UI/features ahead of their milestone. Each numbered
slice is independently scoped, reviewed and tested. Do not use sequencing to
claim that downstream families are less important to the acceptance target.

### Gate 0 — Finish the evidence denominator

1. Audit each family against the complete 2027.2 Help and Developer Help
   navigation: artist topics, menu sets and editors; every relevant command
   family and flags; node types/attributes; C++ and both Python APIs; bundled
   and separately versioned components; supported files, tools and scripts.
2. Reconcile every candidate against the Maya 2026 acceptance boundary. Tag
   rows `2026-only`, `2027-new`, `2026 FALLBACK`, `runtime-only`, component
   version, or `PROVISIONAL` as appropriate. Diff the Developer Help C++ API
   change lists and release/component notes. Record actual source URL, retrieval
   date, page scope, expected behavior/UI, and unresolved defaults.
3. Expand all 14 family inventories using the stable IDs and row/status/evidence
   rules in `parity-inventory/README.md`. Reconcile the UI inventory and MCP
   coverage inventory. Record the source snapshot/repository SHA.
4. Get user approval on unresolved scope (third-party components, service
   features, cloud workflows), any exclusions, the denominator freeze and
   measurement method. No percentage before that gate.

### Gate 1 — Preservation and scene foundation

5. Establish a reference manifest and synthetic/private fixture protocol. Build
   lossless scene representation for DAG hierarchy/instancing, DG node/plug
   identity and connections, typed attributes, units, unknown data and source
   bindings. Add bounded traversal and typed refusal paths.
   This is the prerequisite for animation, rigging, materials, references and
   history graphs; those must not be implemented as sidecar mutable models.
6. Implement only the persistence substrate needed by later slices: staged
   writes, exact source identity/change detection, precision-preserving edit
   intent, and safe retention/refusal rules for unknown data. Do not expose this
   as completed Maya ASCII/Binary Open/Save; those user workflows are Milestone
   2 and require pinned-Maya round-trip evidence.
7. Add undoable typed scene operations and native/MCP parity for hierarchy,
   transforms, attributes, selection and guards. Preserve the current playable
   baseline and test draft/modal/close/dirty/token barriers; complete Maya
   native file actions in Milestone 2.

### Gate 2 — Core interactive authoring

8. Complete shell, menus/menu sets, docking/workspaces, Outliner, Channel Box,
   Attribute Editor, panels, camera navigation, viewport picking, selection
   masks, manipulators and snapping, then complete playable Milestone 1:
   Modeling and interaction.
9. Deliver polygon primitives and component editing; then core topology
   operations, construction history, normals, validation, curves/NURBS.
   Complete only the model-authoring scope of Milestone 1 here; UV work is
   reserved for Milestone 4. Each command needs documented options/defaults,
   undo, persistence, malformed/boundary input tests, native visual checks and
   MCP protocol tests.

### Gate 3 — Native and standard interchange

10. Complete Maya-native Open/Save/Save As and the first standard import/export
    formats in playable Milestone 2. Keep native-file preservation ahead of
    optional translators.
11. Expand interchange formats by data class and translator, with independent
    readers, format options and refusal paths. Preserve unknown information and
    do not advertise formats whose fidelity is not tested.

### Gate 4 — Materials, UV and look development

12. Complete Milestone 3 with persistent base material/shading graphs that do
    not require UVs.
13. Complete Milestone 4 with UV editing and texture workflows; connect
    textured materials only after UV state is available and persisted.

### Gate 5 — Lighting, rendering and animation

14. Complete Milestone 5: cameras, lighting, renderer integration and a
    testable render workflow.
15. Complete Milestone 6: persistent key-based animation and timeline/curve
    editing; then expand to layers, Time Editor, motion paths, camera
    sequencing, audio and playblast.

### Gate 6 — Rigging, extended systems and scripting

16. Complete Milestone 7 with a saved/reopened joint hierarchy, basic skin
    binding/influence editing and animation using Milestone 6 key workflows.
    Add IK, constraints, additional deformers, HumanIK and retargeting in
    separately testable follow-up slices; do not re-list basic skin influences
    as unfinished.
17. Complete Milestone 8 as component-pinned simulation/FX slices: nDynamics,
    Bullet, Bifrost, fluids, BOSS, XGen, Paint Effects and MASH. Include caches,
    time stepping, renderer handoff and license/service boundaries. Never show
    an inert control as implemented.
18. Complete Milestone 9 only after the threat model and execution boundary are
    approved. Separate Maya-compatible existing plug-ins from an AztecCraft
    extension SDK; test real scripts/plug-ins and version/ABI combinations.

### Gate 7 — Pipeline, verification and acceptance

19. Add project/reference management, recovery, large-scene responsiveness,
    batch workflows, platform packaging and robust error recovery.
20. For every family row, track documentation status separately from Maya
    comparison. Compare UI interaction, semantic results, numeric outputs,
    persistence/reopen, undo, invalid inputs and interoperability as applicable.
    Record exact Maya build, platform, component versions, fixture hashes and
    evidence. Runtime-only rows remain at zero until tested.
21. Independently review the plan and each implementation diff twice; run the
    project-prescribed core, native interface and real Editor/MCP protocol
    checks. Report family-level evidence and unresolved cases. Exact parity
    remains unclaimed until comparisons against the pinned Maya 2026 reference
    pass.

## Required completion packet for every feature slice

- Researched Maya 2027.2 pages and API symbols, with an explicit 2026 comparison
  or a marked unresolved version delta.
- Documented workflow rows with UI entry points, defaults/options/hotkeys,
  edge cases, component/version, source and `PROVISIONAL` gaps.
- Core operation and authoritative state, plus native UI and MCP adapter using
  the same typed operation; synchronized schemas and actual Editor protocol
  tests.
- Tests for valid, malformed, empty, boundary and failure cases; baseline
  preservation; undo/persistence/interop where relevant; native visual evidence
  for visible UI changes.
- Updated family inventory, MCP coverage inventory, known issues and plan; no
  unsupported behavior advertised as working.
- Independent plan/diff review, verification evidence and a scoped atomic
  commit/branch update under repository rules.

## Current truth

This roadmap does not fill the currently empty family inventories and is not a
frozen exhaustive denominator. The detailed family-row expansion and full
command/node/API/component audit are still open. Implemented status remains
what the family inventories and `parity.md` actually evidence. The Maya 2026
reference installation, complete runtime comparisons, and 80–90% acceptance
target remain unmet; Maya 2027.2 documentation research does not satisfy them.
