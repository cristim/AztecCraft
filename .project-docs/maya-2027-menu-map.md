# Maya 2027 menu and interaction map

Navigation map for AztecCraft planning, not a claim that AztecCraft implements
these controls. Feature-to-family assignment stays in the
[top-level menu map](parity-inventory/README.md#top-level-menu-to-family-map);
this file adds the artist-facing menu hierarchy and the non-menu entry surfaces.

- Source: official Autodesk Maya 2027 Help pages linked per row (Autodesk
  documentation, (c) Autodesk; cited and summarised here, not copied).
- Pinned version: Maya 2027.2 (see [reference.md](reference.md)). Access date:
  2026-11-20 (first drafted by the contributor on 2026-11-14; every kept claim
  re-checked against the pages on 2026-11-20).
- Author: Elvis33LE (original draft, PR #457), trimmed and re-verified by the
  AztecCraft maintainers.
- No Maya runtime was used. The pinned Maya 2026 comparison remains an
  unmet, independent acceptance requirement; 2027.2 documentation is not
  evidence of Maya 2026 behaviour.

## Notation

IDs are stable path keys of this document, not Autodesk identifiers: global
menus use the lowercase menu name, menu sets `set.<name>`, surfaces dotted
paths (`panel.*`, `context.*`, `shell.*`). **Documented** means the cited page
names the entry; it does not imply order, defaults, availability rules or
completeness. **PROVISIONAL** means not established by a cited page.
**Runtime-only** means it needs observation in the exact installed build.
Hotkeys appear only where the page states them. Names follow the page wording,
including its inconsistencies. A trailing "..." marks an explicit gap.

## Main menu bar

[Main menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-3DB1C040-051F-4C2A-83F8-D806455FCEFF.htm)
states that seven menus are always available (File, Edit, Create, Select,
Modify, Display, Windows) and the others change with the menu set (Modeling,
Rigging, Animation, FX, Rendering). The page's topic list also has Cache and
Help menu topics without saying whether they depend on the set.
[Menus and menu sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D90A2BDB-FD05-4528-8A95-C33A02D15129.htm)
documents switching by the Status Line drop-down or hotkeys F2 to F6 (Modeling,
Rigging, Animation, FX, Rendering), Ctrl+M to toggle the menu bar, and
tear-off menus (click the tear-off line; they float on top). Custom menu sets
are only linked to another page. The page names the fifth set "Dynamics" in
its body and "FX" in the hotkey list.

| Menu | Owner family | Source |
| --- | --- | --- |
| File | INT, SCN, RND, SHL | [File](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm) |
| Edit | SCN, ANM | [Edit](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8AB8C493-9FC7-437C-82F5-323904ECA9DF.htm) |
| Create | MOD, CRV, MAT, VPT, SCN, DYN, INT | [Create](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-AF592D59-02CB-4B97-9C57-37795C8B1D8A.htm) |
| Select | SCN, VPT, CRV | [Select](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D1BBC051-0EE7-4A91-B4FF-2A369764EC8B.htm) |
| Modify | SCN, VPT, ANM, PIP, MOD, CRV, DYN, MAT, SCR, RIG | [Modify](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-378D7FF6-492F-4DA4-A468-507E6ABF8B58.htm) |
| Display | VPT | [Display](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-0657DA41-332E-4F3C-BDAE-4A9AD8CA2553.htm) |
| Windows | SHL and the family owning each editor | [Windows](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-67B0B363-94D9-4BF9-BDB6-75B966F5B144.htm) |
| Help | SHL | No Help-menu page audited: PROVISIONAL. |

Family assignments are the contributor's planning mapping, not Autodesk text.

## Global-menu entries (documented pages)

Entries are the names on each page; option boxes exist for most items and are
not expanded here (follow the linked options pages).

- `file`: New Scene, Open Scene, Save Scene, Save Scene As, Increment & Save,
  Archive Scene, Save Preferences, Optimize Scene Size; Import, Export All,
  Export Selection; Send to Flame, 3ds Max, MotionBuilder, Mudbox, Unity,
  Unreal (the first four are available only with matching Maya and app
  versions; Mudbox update/add items need Mudbox running; Unity and Unreal have
  All, Selection and Set Project items; no availability condition is stated for
  them); Game Exporter; Adobe After Effects Live Link; Export to Offline File;
  the Assign Offline File entry (see below); ATOM; Create Reference,
  Reference Editor; View Image, View Sequence; Project Window, Set Project;
  Recent Files, Recent Projects; Exit. The Open Scene icon on the Status Line
  also lists recently opened files on right-click.
  **Page inconsistency:** the section heading reads "Assign Online File" while
  the body and the Export to Offline File entry say "File > Assign Offline
  File" (options topic "Assign Offline File Options"). The heading is the
  outlier; record the item as Assign Offline File, PROVISIONAL until observed
  in a runtime.
- `edit`: Undo (Ctrl+Z), Redo (Ctrl+Y), Repeat (G), Recent Commands List, Keys
  submenu, Delete, Delete By Type, Delete All by Type, Duplicate (Ctrl+D),
  Duplicate Special (Ctrl+Shift+D), Duplicate with Transform (Shift+D),
  Transfer Attribute Values, Group (Ctrl+G), Ungroup, Create LOD Group,
  Generate LOD Meshes, Ungroup LOD Group, Parent (P), Unparent (Shift+P). The
  page ends at Unparent; whether more Edit entries exist is a gap.
- `create`, by page section. Applications: Open Flow Studio. Objects: NURBS,
  Polygon and Volume Primitives; Lights (Ambient, Directional, Point, Spot,
  Area, Volume); Cameras (Camera, Camera and Aim, Camera Aim and Up, Stereo
  Camera, Multi Stereo Rig); Curve Tools (CV, EP, Bezier and Pencil Curve
  Tools, Three Point and Two Point Circular Arc); Type; Sweep Mesh (plug-in,
  needs a selected curve); SVG; Bifrost Graph (placed after SVG; the page names no parent). Construction Aids: Construction
  Plane (default XY plane; Size in grid units), Free Image Plane, Locator,
  Annotation, Measure Tools (Distance, Parameter, Arc Length). Scene
  Management: Universal Scene Description (Stage with New Layer, Stage From
  File), Scene Assembly (Assembly Definition, Assembly Reference), Empty
  Group, Sets (Set, Partition, Quick Select Set submenu), Asset (Create Asset,
  Create Asset with Transform). Exact order and primitive type lists are not
  given.
- `select`: All (Ctrl+Shift+A), All by Type, Deselect All (Alt+D), Hierarchy,
  Inverse (Ctrl+Shift+I), Similar (Similarity Tolerance applies to components,
  default 0.001), Grow (Shift+.), Shrink, Quick Select Sets, Type
  Object/Component (F8), Polygons components (Vertex F9, Edge F10, Face F11,
  Vertex Face Alt+F9, UV F12, UV Shell; Multi-Component mode F7), Contiguous
  Edges, Shortest Edge Path Tool, Convert Selection, Use Constraints, NURBS
  Curves (components Control Vertex, Curve point, Edit point, Hull; All CVs,
  First CV, Last CV, Cluster Curve), NURBS Surfaces (components Control
  Vertex, Hull, Isoparm, Surface Patch, Surface Point, Surface UV; CV
  Selection Boundary, Surface Border). All by Type and Quick Select Sets
  children are dynamic.
- `modify`, by page section (Transform, Pivot, Align, Rotate Order, Nodes,
  Naming, Attributes, Objects, Paint Tool, Asset): Transformation Tools, Reset
  Transformation, Freeze Transformations, Match Transformations; Center Pivot,
  Zero Pivot, Bake Pivot; Snap Align Objects, Align Tool, Snap Together Tool;
  Reorder Rotation...; Evaluate Nodes (a submenu); Prefix Hierarchy Names,
  Search and Replace Names; Add, Edit and Delete Attribute; Make Live, Replace
  Objects, Convert; Paint Scripts Tool, Paint Attributes Tool; Asset items
  (Add to Asset, Remove from Asset, Export Proxy Asset, Publish and Unpublish
  Attributes, Publish Connections, Lock and Unlock Unpublished Attributes,
  Publish and Unpublish Node, Assign Template, Select Asset Contents, Advanced
  Assets). Section headings are not menu levels; the page says both "Asset"
  and "Assets" in paths, so exact nesting is PROVISIONAL.
- `display`: Grid, Heads Up Display, Hide, Show, Toggle Show/Hide (the page
  also writes the path as "Toggle Hide/Show"; H applies it), Per Camera
  Visibility (Exclusive To Camera, Hidden From Camera, Remove From Exclusive,
  Remove From Hidden, Remove All For Camera, Remove All), Wireframe Color,
  Object Display (Template/Untemplate, Bounding Box/No Bounding Box,
  Geometry/No Geometry, Ignore/Use Hardware Shader, Fast Interaction),
  Transform Display (Local Rotation Axes, Rotate Pivots, Scale Pivots,
  Selection Handles), Polygons, NURBS, Animation and Rendering submenus (each
  only links to an options page), Frame All in All Views (Shift+A), Frame
  Selection in All Views (Shift+F), Frame Selection with Children in All Views
  (Ctrl+Shift+F). "User Interface" and "Object" are section headings.
- `windows`: Home (Alt+Home), Autodesk Assistant, Workspaces (reset to factory
  default or previously saved, Save Current Workspace As, Import Workspace
  Files, Delete Current Workspace, Disable Docking/Undocking), Editors
  (General: Attribute Editor, Channel Box / Layer Editor, Tool Settings,
  Hypergraph Hierarchy and Connections, Viewport, Asset Editor, Attribute
  Spread Sheet, Component Editor, Connection Editor, Channel Control, Display
  Layer Editor, File Path Editor, Namespace Editor, USD Layer Editor, Script
  Editor, Command Shell, Profiler, Evaluation Toolkit; Modeling: Modeling
  Toolkit, Paint Effects, UV Editor, XGen Editor, Crease Sets; Animation:
  Graph Editor, Dope Sheet, Time Editor, Sequencer, Trax Editor, Motion Trail
  Editor, Ghosting Editor, Quick Rig, HumanIK, Shape Editor, Pose Editor,
  Expression Editor; Rendering: Render View, Render Settings, Hypershade,
  Render Setup, Light Editor, Custom Stereo Rig Editor, Rendering Flags,
  Shading Group Attributes; Relationship Editors, item names not listed),
  UI Elements (Status Line, Shelf, Time Slider, Cut Slider, Range Slider,
  Command Line, Help Line, Toolbox, Hide/Show All UI Elements, Restore UI
  Elements), Settings/Preferences (Preferences, Performance Settings, Hotkey
  Editor, Color Settings, Marking Menu Editor, Shelf Editor, Panel Editor,
  Plug-in Manager), Outliner, Node Editor, Content Browser, Bifrost Graph
  Editor, Bifrost Browser, Playblast, Output Window, Minimize Application,
  Raise Main Window, Raise Application Windows.

## Menu sets

| ID | Menus named in the page's topic list | Source and boundary |
| --- | --- | --- |
| `set.modeling` (F2) | Mesh (whole-object actions), Edit Mesh (component actions), Mesh Tools (interactive tools; one active), Mesh Display, Curves, Surfaces, Deform, UV, Generate | [Modeling](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-A0E08BF2-0206-4518-8431-DC4E15AD4558.htm). Children of each menu are not listed. |
| `set.rigging` (F3) | Skeleton, Skin, Deform, Constrain, Control | [Rigging](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-CharacterAnimation/files/GUID-E8BE786A-AA88-46FD-A402-FBCD8855368B.htm). The page also shows one image (not inspected). |
| `set.animation` (F4) | Key, Playback, Audio, Visualize, Deform, Constrain | [Animation](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-3CCAA3C8-A480-4CB7-A85E-F5AB17D3FB1F.htm). Same image caveat. |
| `set.fx` (F5) | Not checked: PROVISIONAL | The menu-set page names the set and F5 only. |
| `set.rendering` (F6) | Lighting/Shading, Texturing, Render, Stereo | [Rendering](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-E4D6B80A-B812-4776-91C8-E5229CBF989F.htm). Same image caveat. |

## Menus outside the main bar

| ID | Documented inventory | Boundary |
| --- | --- | --- |
| `panel.menus` | Per view panel (options adjustable per panel in multi-view layouts); Shift+M toggles; linked sub-pages View, Shading, Lighting, Show, Renderer, Panels, Stereo. [Panel menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-BD5D066A-4876-4B56-8A9F-96E2243E22A3.htm) | No menu tree is itemized: gap. |
| `panel.toolbar` | Groups View (Select Camera, Lock Camera, Camera Attributes, Bookmarks, Image Plane, 2D Pan/Zoom, Blue Pencil); Camera Settings (Grid, Film Gate, Resolution Gate, Gate Mask, Field Chart, Safe Action, Safe Title); Shading (Wireframe 4, Smooth Shade All 5, Use Default Material, Wireframe on Shaded, Textured 6); Lighting (Use All Lights 7, Shadows); Isolate Select; Viewport 2.0 effects (SSAO, Motion Blur, Multisample AA, Depth of Field); X-Ray (X-Ray, Active Components, Joints); Color Management (Exposure, Gamma, View Transform, diagnostic only). Ctrl+Shift+M toggles. [Panel toolbar](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-42224988-F140-4BFF-849B-F89CCB554E13.htm) | Buttons vary by renderer; on-screen order and icons are image-only. |
| `context.viewport.marking` | [Marking Menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8BA1A3AA-4C44-4779-8B22-0AAE3627E8EB.htm) documents families: Polygon/NURBS right-click, Blue Pencil, Shift+right-click Polygon Tools, UV and UV Shell, NURBS curve, transform-tool menus (Ctrl+Shift+right-click, or hold Q, W, E or R and click), Input/Output (hold A and click), common, vertex, edge and face component menus, sculpting, Graph Editor (Shift+S and click), Convert Selection (Ctrl+right-click), Create Polygon (Shift+right-click, no selection). Item lists are not transcribed here. | Radial sector positions are not stated (tables only); runtime-only. |
| `context.other` | Editor-specific context menus: not audited. | Gap. |
| `shell.shelf`, `shell.sidebar` | Shelf tabs by category with common-task icons and custom shelves (Shift+Ctrl while choosing a menu item adds it); sidebar Modeling Toolkit, HumanIK, Attribute Editor, Tool Settings, Channel Box/Layer Editor, with docking and tabs. [Interface overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm), [Shelves](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-4A21F741-C9AC-4AE5-897E-B6F8C68ADF90.htm) | No default shelf button list: gap. |
| `shell.toolbox` | Select Q, Lasso and Paint Selection (no hotkey assigned), Move W, Rotate E, Scale R (each with a click marking menu), Last Used Y. [Tool Box](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-B345E162-0149-4E09-AC98-48DCFC227F33.htm) | Quick-layout buttons not covered. |
| `shell.statusline` | Menu-set selector, selection mode and mask buttons, lock selection, Highlight Selection mode, snapping (grids, curves, points, Projected Center which overrides other modes, view planes), Make Live with recent-live list, construction input/output popup, Construction History toggle, Render View, render, IPR and render settings buttons, Hypershade, Input Box (Absolute default, Relative, Rename, Select by name). [Status line](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-86E5CEDA-4100-40AE-8F95-346206CF8456.htm) | Default groups, masks and icon order: runtime gap. |
| `editor.*` | Editor-local menus, toolbars and options: not audited. | Gap, one audit per editor. |

[Animation hotkeys](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-KeyboardShortcuts/files/GUID-0639ADA0-EBE5-4703-A874-92C80E4A0516.htm)
is a context-specific shortcut list (Animation Operations, Playback Control,
Time Editor, Graph Editor), not a global catalogue.

## Option boxes and conditional controls

Treat a submenu as a hierarchy, not a flat list. An option box is its own
workflow: record how it opens, defaults and reset, every option, Apply/Close
behaviour, persistence and the resulting operation; leave unimplemented ones
visibly unsupported. Record state, default and effect for toggles; map plug-in,
selection, mode and context dependence as conditional. Record label, order,
hotkey and modifiers only when Autodesk documents them or the pinned runtime
confirms them.

## Completeness

This is not a complete Maya UI inventory. Open gaps: Help menu; Edit entries
past Unparent; FX menu set and the children of every menu-set menu; option
dialog fields and defaults; dynamic submenus; panel menus; shelf defaults;
editor and Outliner context menus; hotbox; plug-in menus; visual order,
enablement and radial sectors. Absence here is not evidence of absence in Maya.
Before any runtime-confirmed claim, record build and patch, OS, locale,
workspace, preferences, menu set, selection and mode, panel and the loaded
plug-in manifest. Appearing on this map upgrades no implementation status; see
[parity.md](parity.md) and the [UI parity inventory](ui-parity-inventory.md).
