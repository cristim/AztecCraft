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
menus use the lowercase menu name, menu sets `set.<name>`, modeling-set menu
blocks `set.modeling.<menu>`, surfaces dotted
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
| `set.modeling` (F2) | Mesh (whole-object actions), Edit Mesh (component actions), Mesh Tools (interactive tools; one active), Mesh Display, Curves, Surfaces, Deform, UV, Generate | [Modeling](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-A0E08BF2-0206-4518-8431-DC4E15AD4558.htm). The Mesh, Edit Mesh, Mesh Tools, Mesh Display, Curves, Surfaces and UV children are audited in the section below; Deform (owned by RIG) and Generate (owned by DYN) remain unaudited here. |
| `set.rigging` (F3) | Skeleton, Skin, Deform, Constrain, Control | [Rigging](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-CharacterAnimation/files/GUID-E8BE786A-AA88-46FD-A402-FBCD8855368B.htm). The page also shows one image (not inspected). |
| `set.animation` (F4) | Key, Playback, Audio, Visualize, Deform, Constrain | [Animation](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-3CCAA3C8-A480-4CB7-A85E-F5AB17D3FB1F.htm). Same image caveat. |
| `set.fx` (F5) | Not checked: PROVISIONAL | The menu-set page names the set and F5 only. |
| `set.rendering` (F6) | Lighting/Shading, Texturing, Render, Stereo | [Rendering](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-E4D6B80A-B812-4776-91C8-E5229CBF989F.htm). Same image caveat. |

## Modeling menu set menus (documented pages)

One block per menu, transcribed from each menu's own page; ids are
`set.modeling.<menu>`. Paths are quoted only where the page states them
("Select Mesh > X >"); other grouping names on the pages (Mirror, Transfer,
Optimize, Remesh, Tools, Vertex Color Sets, Vertex Bake Sets, Display
Attributes, Per Instance Sharing, Components, Vertex, Edge, Face, Curve,
Modify, Edit, Create, Cut/Sew, Editors) are page sections, not documented menu
levels, and items under them are listed directly with nesting PROVISIONAL.
Option boxes exist for most items (each page links options topics); option
dialog contents are not expanded here. Family mapping (planning, not Autodesk
text): Mesh, Edit Mesh, Mesh Tools, Mesh Display → MOD; Curves, Surfaces →
CRV; UV → UVE.

- `set.modeling.mesh` ([Mesh](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-797DD7AE-11D4-4399-8B31-BA0B67D33650.htm)):
  Booleans submenu (page states Mesh > Booleans > <operation>; Union, Difference
  (A - B), Difference (B - A), Intersection, Slice, Hole Punch, Cut Out, Split
  Edges, each with its documented effect; Boolean Operation Options); Combine
  (options; polyUnite node, Component Tags note); Separate (all shells, or
  faces selected first); Conform (wraps vertices onto another surface;
  options); Fill Hole (region bounded by three or more edges); Reduce
  (polyReduce node, optionally UVs and vertex colors; paintable via
  Mesh Tools > Paint Reduce Weights Tool; options); Smooth (options); Unsmooth
  (polyUnsmooth; the page locates Unsmooth Options via Mesh > Smooth > option
  box: PROVISIONAL oddity); Triangulate; Quadrangulate (options); Remesh
  (options); Retopologize (Esc cancels; options); Mirror (options);
  Symmetrize (needs topological symmetry); Clipboard Actions submenu (Copy
  Attributes with options, Paste Attributes, Clear Clipboard with options);
  Transfer Attributes (UV, color per vertex, vertex position; options);
  Transfer Shading Sets (options); Transfer Vertex Order; Cleanup (options);
  Smooth Proxy submenu (page states Mesh > Smooth Proxy > Subdiv Proxy /
  Remove Subdiv Proxy Mirror; Subdiv Proxy keeps the original as connected
  proxy; Crease Tool listed also at Mesh Tools > Crease Tool; Toggle Proxy
  Display with Ctrl+' and Both Proxy and Subdiv Display with ~ are grouped
  under Smooth Proxy on the page without stated paths: nesting PROVISIONAL;
  Subdiv Proxy and Remove Subdiv Proxy Mirror have options).
- `set.modeling.editmesh` ([Edit Mesh](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-8FF5F5FC-2065-438A-AFC8-65A17FA7842F.htm)):
  Add Divisions (options; window contents follow the current component mode);
  Bevel (options); Bridge (border edge pairs; options); Circularize (options);
  Collapse (edges; for faces prefer Merge to Center); Connect (options);
  Detach (vertex/face/edge behaviours; ignores unshared vertices); Extrude
  (faces, edges, vertices; Keep Faces Together toggles via
  Ctrl+Shift+right-click while the Show Manipulator shows; options); Merge
  (threshold; options); Merge to Center; Transform (move, rotate, scale
  components along their normals, history node; options); Flip; Symmetrize;
  Average Vertices (options); Chamfer Vertices (options); Reorder Vertices;
  Delete Edge/Vertex (Ctrl+Delete or Ctrl+Backspace); Edit Edge Flow
  (options); Flip Triangle Edge; Spin Edge Backward; Spin Edge Forward (all
  three also in the Shift+right-click Edge-mode marking menu as
  Flip/Spin Edge); Assign Invisible Faces (options); Duplicate (faces;
  options); Extract (options); Poke (options); Wedge (options); Project Curve
  on Mesh (options); Split Mesh with Projected Curve (options). The page's
  options-topic list additionally names Smart Bevel Options with a Smart
  Bevel description without naming it as a menu entry: PROVISIONAL where it
  lives (Bevel item or separate).
- `set.modeling.meshtools` ([Mesh Tools](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-57E220F7-937B-4026-9BF3-5775D54EB594.htm)):
  Show/Hide Modeling Toolkit; Append to Polygon (options); Connect tool
  (options); Crease (options; Shift+right-click tip); Create Polygon
  (options); Insert Edge Loop (options); Make Hole (options); Multi-Cut
  (options); Offset Edge Loop (options); Paint Reduce Weights (options; works
  with Mesh > Reduce); Paint Transfer Attributes (options); Quad Draw
  (options); Sculpting Tools submenu (page states Mesh Tools > Sculpting
  Tools > <tool name> > for the sculpt tool settings; the linked
  [Sculpting shelf](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-820F611F-3380-4D33-AC69-EEC62A59109B.htm)
  page lists the tools as Sculpt (Ctrl+1), Smooth (Ctrl+2), Relax (Ctrl+3),
  Grab (Ctrl+4), Pinch (Ctrl+5), Flatten (Ctrl+6), Foamy (Ctrl+7), Spray
  (Ctrl+8), Repeat (Ctrl+9), Imprint, Wax, Scrape, Fill, Knife, Smear,
  Bulge); Slide Edge (options); Target Weld (options; same mesh only).
- `set.modeling.meshdisplay` ([Mesh Display](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-57FBF97C-F60F-4D8A-8801-852B8B56D4EF.htm)):
  Average (options), Conform, Reverse (options), Set to Face (options), Set
  Vertex Normal (options), Harden Edge (Normal angle 0), Soften Edge (Normal
  angle 180), Soften/Harden Edge (Method Angle or Texture Borders), Lock
  Normals, Unlock Normals, Vertex Normal Edit Tool (options) under the page's
  Normals section; Apply Color (options), Paint Vertex Color Tool (options);
  Create Empty Set (options), Delete Current Set, Rename Current Set, Modify
  Current Set (polyColorMod), Set Keyframe for Vertex Color, Color Set Editor
  (options) under Vertex Color Sets; Prelight (Maya) (options), Assign New
  Set, Assign Existing Set, Edit Assigned Set under Vertex Bake Sets; Toggle
  Display Colors Attribute, Color Material Channel ([options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-04408333-F529-4193-9C38-E18A53BDFAB6.htm):
  None, Ambient, Ambient+Diffuse, Diffuse (the page's default), Specular,
  Emission; selecting CPV Modulate on a Layered Texture also ignores this
  setting), Material Blend Setting ([options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-B62EED3D-CCE7-4A06-B057-856276653459.htm):
  Overwrite, Add, Subtract, Multiply, Divide, Average, Modulate2X) under
  Display Attributes; Select Shared Instances, Share Instances under
  Per Instance Sharing. Section groupings other than Normals are page
  sections without stated paths: nesting PROVISIONAL.
- `set.modeling.curves` ([Curves](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-44AC128C-47DC-4751-8457-22CE823B6F97.htm)):
  under the page's Modify section: Lock Length (hold l as momentary hotkey),
  Unlock Length, Bend (options), Curl (options; page warns against applying
  twice), Scale Curvature (options), Smooth (page writes the options path as
  Curves > (Modify) Smooth), Straighten (options); under Edit: Duplicate
  Surface Curves (options), Align (options), Add Points Tool, Attach, Detach,
  Edit Curve Tool (Tangent Manip. Size), Move Seam, Open/Close (options),
  Fillet (options), Cut (options), Intersect (options), Extend submenu (page
  states Curves > Extend > Extend Curve and Curves > Extend > Extend Curve on
  Surface, each with options), Insert Knot (options), Offset submenu
  (Curves > Offset > Offset Curve / Offset Curve on Surface, each with
  options), CV Hardness (options), Fit B-spline (options), Project Tangent
  (options), Smooth (page writes Curves > (Edit) Smooth with its own options
  page; two Smooth entries exist, one per section), Bezier Curves submenu
  (Curves > Bezier Curves > Tangent Options: Smooth Anchor Tangents, Break
  Anchor Tangents, Even Anchor Tangents, Uneven Tangents per the
  [Bezier Curve Tangent Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-3CA09C8F-6D4C-459F-8123-EE435D3B8413.htm)
  page; Curves > Bezier Curves > Anchor Presets: Bezier, Bezier Corner,
  Corner per the [Bezier Curve Anchor
  Presets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-F2F18AE2-1005-40B3-9535-7EF756E7F6B0.htm)
  page), Rebuild (options), Reverse Direction (options).
- `set.modeling.surfaces` ([Surfaces](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-7251C28B-8198-4188-BBF2-A4C25F26A606.htm)):
  under the page's Create section: Loft, Planar, Revolve, Extrude, Boundary,
  Square, Bevel and Bevel Plus (each with options), Birail submenu (Surfaces >
  Birail > Birail 1 Tool / Birail 2 Tool / Birail 3+ Tool); under Edit NURBS
  Surfaces: Duplicate NURBS Patch (options), Align (options), Attach
  (options), Attach Without Moving, Detach (options), Move Seam, Open/Close
  (options), Intersect (options), Project Curve on Surface (options), Trim
  Tool (options), Untrim (options), Extend (options), Insert Isoparms
  (options), Offset (options), Round Tool (options), Stitch submenu (Surfaces
  > Stitch > Stitch Surface Points / Stitch Edges Tool / Global Stitch, each
  with options), Surface Fillet submenu (Circular Fillet, Freeform Fillet,
  Fillet Blend Tool, each with options), Sculpt Geometry Tool (options page
  shared with poly/subdiv sculpting), Surface Editing submenu (Surfaces >
  Surface Editing > Surface Editing Tool, options), Booleans submenu (Union
  Tool / Difference Tool / Intersection Tool; trims two surfaces; options),
  Rebuild (options), Reverse Direction (options).
- `set.modeling.uv` ([UV](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-3100AC8D-C82C-4F17-A556-39760FEE91AC.htm)):
  under Editors: UV Editor, UV Set Editor (editor launchers); under Create:
  Assign Checker Shader (default off), Automatic (options), Best Plane
  Texturing Tool (page requires all faces selected or the Polygon mask),
  Camera-Based (options), Contour Stretch (options), Normal-Based,
  Cylindrical (options), Planar (options), Spherical (options); under
  Cut/Sew: Auto Seams, Cut UV Edges (also in the UV Editor toolbar), Sew UV
  Edges (also toolbar), Split UVs, Delete UVs, Merge UVs (options; preferred
  over Sew for non-manifold shells), 3D Cut and Sew UV Tool (options; drag to
  cut, Ctrl+drag to sew, Shift/Ctrl+Shift extend along the loop; the page
  warns that Ctrl+Shift+S toggles the Keyframe tangent marking menu), 3D Grab
  UV Tool (options).

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
past Unparent; the children of the Rigging, Animation, FX and Rendering
menu-set menus; Modeling > Generate and Modeling > Deform; option dialog
fields and defaults; dynamic submenus; panel menus; shelf defaults; editor
and Outliner context menus; hotbox; plug-in menus; visual order, enablement
and radial sectors. Absence here is not evidence of absence in Maya.
Before any runtime-confirmed claim, record build and patch, OS, locale,
workspace, preferences, menu set, selection and mode, panel and the loaded
plug-in manifest. Appearing on this map upgrades no implementation status; see
[parity.md](parity.md) and the [UI parity inventory](ui-parity-inventory.md).
