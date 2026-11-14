# Maya 2027 menu and interaction map

Reference: Autodesk Maya 2027.2 Help, consulted 2026-11-14. This is the
navigation map for AztecCraft planning—not a claim that AztecCraft currently
implements these controls. The existing per-workflow menu map in
[parity-inventory/README.md](parity-inventory/README.md#top-level-menu-to-family-map)
continues to own feature-to-family assignment. This document adds the
artist-facing menu hierarchy and the non-menu entry surfaces that must be
inventoried.

## Evidence boundary and inventory schema

This inventory is based on official Autodesk Maya 2027 Help pages linked
throughout. Here, **Documented** means the cited page names the menu entry or
surface; it does not imply that the page gives every row, visual order,
availability rule, option default, or gesture. **PROVISIONAL** means inferred
or not established by the cited page. **Runtime-only** means it requires
observation in the exact installed build. No Maya runtime was available for
this audit. The required Maya 2026 runtime comparison remains an independent,
unresolved acceptance requirement; Maya 2027.2 documentation is not evidence
of Maya 2026 behavior.

IDs below are stable path keys for this document, not Autodesk identifiers.
Global-menu rows use the lowercase menu name (`file`, `edit`, …); menu-set rows
use `set.<name>`; UI surfaces use dotted paths (`panel.*`, `context.*`,
`shell.*`). `editor.*` is an explicit wildcard for the editor-local menus that
separate per-editor audits still have to expand into concrete IDs. `†` denotes
an entry explicitly named/described by the cited page. `‡` denotes a category
whose children or exact placement are not fully specified by that page. A path
ending in `…` is an explicit coverage gap, not an assertion that the preceding
page has no further entries. Hotkeys are written only where the official page
states them; absence of a shortcut is not proof that none exists.

## Main menu bar

Autodesk's [Main menus and menu sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-3DB1C040-051F-4C2A-83F8-D806455FCEFF.htm)
states that seven menus are always available and the remaining menus switch
with the active menu set. The Help menu is also shown in the menu bar/map, but
is not included in that page's seven-menu statement.

The separate [Menus and menu sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D90A2BDB-FD05-4528-8A95-C33A02D15129.htm)
page documents the five workflow sets and selector shortcuts F2–F6. It also
covers the menu-bar toggle and custom/tear-off menu-set behavior. Use these
pages as the stable structural source, then check build-specific visible labels
and ordering against the pinned runtime.

| Main menu | Stable across menu sets? | Main responsibilities / child menus | Owner family |
| --- | --- | --- | --- |
| File | Yes | New/Open/Save/Save As; Import/Export; references; Send To / translators where installed. Some items are plug-in- or workspace-dependent. | INT, SCN, RND, SHL |
| Edit | Yes | Undo/Redo/Repeat, selection/clipboard and scene edit operations; Keys submenu for animation actions. | SCN, ANM |
| Create | Yes | Polygon and NURBS primitives; curves; text/SVG; lights/volume primitives; cameras/image planes; groups, sets, locators and measures; USD/Bifrost creation where supported. | MOD, CRV, MAT, VPT, SCN, DYN, INT |
| Select | Yes | Object/component modes, component masks, selection rules and hierarchy/related selection; NURBS curve/surface submenus. | SCN, VPT, CRV |
| Modify | Yes | Transform tools, reset/freeze, pivot, align, rotate order, nodes, naming, attributes, objects/conversion, paint tools and assets. | SCN, VPT, ANM, PIP, MOD, CRV, DYN, MAT, SCR, RIG |
| Display | Yes | Viewport/object/component display, visibility/selectability, HUD, grid and display options. | VPT |
| Windows | Yes | Preferences/settings, workspaces and editors; editor availability and menu placement may be plug-in-dependent. | SHL and the family owning each editor |
| Help | Menu-bar entry | Help/search, learning and release information. Its exact placement/contents should be checked against the pinned Maya UI. | SHL |

Primary Autodesk pages: [File](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm),
[Edit](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8AB8C493-9FC7-437C-82F5-323904ECA9DF.htm),
[Create](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-AF592D59-02CB-4B97-9C57-37795C8B1D8A.htm),
[Select](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D1BBC051-0EE7-4A91-B4FF-2A369764EC8B.htm),
[Modify](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-378D7FF6-492F-4DA4-A468-507E6ABF8B58.htm),
[Display](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-0657DA41-332E-4F3C-BDAE-4A9AD8CA2553.htm),
[Windows menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-67B0B363-94D9-4BF9-BDB6-75B966F5B144.htm),
and [interface overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm).
The exact Help-menu URL was not individually audited here: `PROVISIONAL`.

## Recursive global-menu inventory (documented pages)

Rows transcribe the scope of the individual Autodesk menu pages, with
option-box destinations where the page names them. This is recursive only as
far as those pages document. For complete option-dialog controls/defaults,
follow each linked options page; these are not all expanded here.

| ID | Menu / documented dropdown entries | Owner family / evidence and coverage boundary |
| --- | --- | --- |
| `file` | New Scene† (New Scene Options); Open Scene† (Open Options; the Status Line Open icon's right-click opens Recent Files, per the same page); Save Scene† (Save Scene Options); Save Scene As† (Save Scene As Options); Increment & Save†; Archive Scene† (Archive Scene Options); Save Preferences†; Optimize Scene Size† (Optimize Scene Size Options); Import† (Import Options); Export All† and Export Selection† (shared Export All/Selection Options); Send to Flame†, 3ds Max†, MotionBuilder†, Mudbox† (Send as Preset‡; documented children not fully transcribed here), Unity† (Unity Project Settings‡; documented children not fully transcribed here), Unreal† (Unreal selection/export children‡; documented children not fully transcribed here); Game Exporter†; Adobe After Effects Live Link†; Export to Offline File†; Assign Online File†; ATOM‡; Create Reference† (Reference Options); Reference Editor†; View Image†; View Sequence†; Project Window†; Set Project†; Recent Files†; Recent Projects†; Exit†. | [File menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm). The page explicitly conditions several Send To actions on matching apps/versions and documents nested children under Send to Mudbox/Unity/Unreal that this row has **not** fully transcribed; ATOM children are on its linked page. Translators/installed integrations and OS-dependent viewers are conditional. |
| `edit` | Undo† (Ctrl+Z); Redo† (Ctrl+Y); Repeat† (G; middle-click menu name repeats last chosen command); Recent Commands List†; Keys‡; Delete†; Delete By Type‡; Delete All by Type‡; Duplicate† (Ctrl+D); Duplicate Special† (Ctrl+Shift+D; options); Duplicate with Transform† (Shift+D); Transfer Attribute Values† (options); Group† (Ctrl+G; options); Ungroup† (options); Create LOD Group† and Generate LOD Meshes† (options); Ungroup LOD Group†; Parent† (P); Unparent† (Shift+P; Unparent Options). | [Edit menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8AB8C493-9FC7-437C-82F5-323904ECA9DF.htm). Keys submenu children belong to the linked Key menu. The fetched page ended at Unparent; remaining Edit entries/branches, if any, require audit. |
| `create` | Applications > Open Flow Studio†; Objects > NURBS Primitives‡ (options), Polygon Primitives‡ (options), Volume Primitives† (options), Lights† (Ambient, Directional, Point, Spot, Area, Volume; each has options), Cameras† (Camera, Camera and Aim, Camera/Aim/Up, Stereo Camera, Multi Stereo Rig; custom registered rigs conditional), Curve Tools† (CV Curve, EP Curve, Bezier Curve, Pencil Curve, Three Point Circular Arc, Two Point Circular Arc; option pages named for these tools), Type†, Sweep Mesh† (plugin and selected-curve preconditions), SVG†; Construction Aids > Construction Plane† (Pole Axis default XY, Size); Free Image Plane†; Locator†; Annotation†; Measure Tools† (Distance, Parameter, Arc Length); Scene Management > Universal Scene Description† (Stage with New Layer, Stage From File); Scene Assembly† (Assembly Definition, Assembly Reference); Empty Group†; Sets‡ (Set, Partition; Quick Select Set submenu); Asset† (Create Asset, Create Asset with Transform; option pages); other Create-page children are not exhaustively stated on this page. | [Create menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-AF592D59-02CB-4B97-9C57-37795C8B1D8A.htm). The source page documents listed controls and conditions; it does not establish exact order or full primitive type lists. Bifrost Graph is not established by this page; prior placement claim is PROVISIONAL. |
| `select` | All† (Ctrl+Shift+A); All by Type‡; Deselect All† (Alt+D); Hierarchy†; Inverse† (Ctrl+Shift+I); Similar† (Similarity Tolerance documented as 0.001 default for components); Grow† (Shift+.); Shrink†; Quick Select Sets‡; Type > Object/Component† (F8); Polygons > Components† (Vertex F9, Edge F10, Face F11, Vertex Face Alt+F9, UV F12, UV Shell; Multi-Component F7); Contiguous Edges† (options); Shortest Edge Path Tool†; Convert Selection† (options); Use Constraints† (options); NURBS Curves > Components‡ (CV, Curve point, Edit point, Hull), All CVs, First CV, Last CV, Cluster Curve; NURBS Surfaces > Components‡ (CV, Hull, Isoparm, Surface Patch, Surface Point, Surface UV), CV Selection Boundary, Surface Border. | [Select menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D1BBC051-0EE7-4A91-B4FF-2A369764EC8B.htm). Page documents these names and stated shortcuts. “All by Type” and Quick Select Sets children are dynamic/type-dependent. |
| `modify` | Transform > Transformation Tools† (options); Reset Transformation†; Freeze Transformations† (options); Match Transformations‡; Pivot > Center Pivot†, Zero Pivot†, Bake Pivot†; Align > Snap Align Objects† (options), Align Tool†, Snap Together Tool†; Rotate Order > Reorder Rotation…† (options); Nodes > Evaluate Nodes‡ (children not listed); Naming > Prefix Hierarchy Names†, Search and Replace Names†; Attributes > Add Attribute† (options), Edit Attribute† (options), Delete Attribute†; Objects > Make Live†, Replace Objects† (options), Convert‡ (link); Asset > Add to Asset† (options), Remove from Asset†, Export Proxy Asset† (options), Publish Attributes† (options), Unpublish Attributes†, Publish Connections† (options), Lock Unpublished Attributes†, Unlock Unpublished Attributes†, Publish Node†, Unpublish Node†, Assign Template† (options), Select Asset Contents†, Advanced Assets‡. | [Modify menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-378D7FF6-492F-4DA4-A468-507E6ABF8B58.htm). “Convert” and “Advanced Assets” children are delegated to linked submenu pages. The page does not enumerate Evaluate Nodes entries. |
| `display` | User Interface > Grid† (options), Heads Up Display‡; Object > Hide†, Show†, Toggle Show/Hide† (options; H applies toggle), Per Camera Visibility‡, Wireframe Color†, Object Display‡ (Template/Untemplate, Bounding Box/No Bounding Box, Geometry/No Geometry, Ignore/Use Hardware Shader, Fast Interaction), Transform Display‡ (Local Rotation Axes, Rotate Pivots, Scale Pivots, Selection Handles), Polygons‡ (linked options page), NURBS‡ (linked options page), Animation‡ (linked options page), Rendering‡ (linked options page); Frame All in All Views† (Shift+A); Frame Selection in All Views† (Shift+F); Frame Selection with Children in All Views† (Ctrl+Shift+F). | [Display menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-0657DA41-332E-4F3C-BDAE-4A9AD8CA2553.htm). The linked category pages contain further controls not expanded here. |
| `windows` | Home† (Alt+Home); Autodesk Assistant†; Workspaces‡ (predefined/custom workspace list, Reset to Factory/Previously Saved, Save Current Workspace As, Import Workspace Files, Delete Current Workspace, Disable Docking/Undocking); Minimize Application†; Raise Main Window†; Raise Application Windows†. | [Windows menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-67B0B363-94D9-4BF9-BDB6-75B966F5B144.htm). The UI Elements and Settings/Preferences trees are not present in the fetched Windows menu page and remain uncovered here. |
| `help` | Help menu contents not itemized by the audited Main menus page. | **PROVISIONAL / gap:** no item-level official Help-menu page audited; do not treat prior summary as a sourced inventory. |

These are documented page inventories, not the complete installed menu bar:
menu-set-specific menus are separately documented only at uneven depth.

## Menu-set selector and menus

The menu-set switcher exposes five workflow sets; Maya documents F2–F6 for
Modeling, Rigging, Animation, FX and Rendering respectively. The menu set is
part of the UI state, not a separate scene capability.

| Menu set ID | Page-supported menu names / known children | Source and coverage |
| --- | --- | --- | --- |
| `set.modeling` (F2) | Menu-set page names Mesh, Edit Mesh, Mesh Tools and explains their object-action / component-action / interactive-tool distinction. Other children shown in legacy/planning maps are not exhaustively confirmed by this page. | [Modeling menu set](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Modeling/files/GUID-A0E08BF2-0206-4518-8431-DC4E15AD4558.htm). Other menu names and every child need individual page/runtime audit. |
| `set.rigging` (F3) | Page establishes a Rigging-specific menu set, but its image-only contents are not text-transcribed in the page extraction. | [Rigging menu set](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-CharacterAnimation/files/GUID-E8BE786A-AA88-46FD-A402-FBCD8855368B.htm). Prior named list is **PROVISIONAL** unless separately sourced. |
| `set.animation` (F4) | Page establishes an Animation-specific menu set, but its image-only contents are not text-transcribed in the page extraction. | [Animation menu set](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Animation/files/GUID-3CCAA3C8-A480-4CB7-A85E-F5AB17D3FB1F.htm). Prior named list is **PROVISIONAL** unless separately sourced. |
| `set.fx` (F5) | The menu-set selector page establishes Dynamics/FX and F5; it does not give a recursive tree. | [Menus and menu sets](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-D90A2BDB-FD05-4528-8A95-C33A02D15129.htm). Every named menu/child in earlier summaries remains **PROVISIONAL** pending exact 2027 pages/runtime. |
| `set.rendering` (F6) | Page establishes a Rendering-specific menu set, but its image-only contents are not text-transcribed in the page extraction. | [Rendering menu set](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Rendering/files/GUID-E4D6B80A-B812-4776-91C8-E5229CBF989F.htm). Prior named list is **PROVISIONAL** unless separately sourced. |

Other cross-set menus and controls:

- **Modify > Objects > Convert** groups conversions by object type: polygon,
  subdivision, NURBS, curves and selected extended systems. The family inventory
  assigns each operation to its owning family; do not treat the Convert
  submenu as one feature.
- **Cache** contains geometry/cache workflows. Maya's menu map assigns
  geometry cache to DYN and Alembic/GPU Cache entries to INT.
- **MASH** and **Arnold** may contribute menus when their plug-ins/components
  are installed and loaded. Their exact 2027 menu pages/order and component
  versions remain to be verified; no default control should imply availability.
- **Create > Universal Scene Description** and Bifrost Graph creation are
  distinct plug-in-backed entry points; exact group placement is provisional
  until checked against their 2027 feature pages.

## Dropdowns, option boxes and nested controls

Represent each menu as a hierarchy rather than a flat list:

```text
Main menu
└── submenu / dropdown
    ├── command or toggle
    ├── nested submenu
    └── option-box affordance
        └── settings window/dialog and its documented defaults
```

- A submenu/dropdown groups related commands but is not itself proof that each
  command is implemented.
- An option box is a separate workflow: record how it opens, its initial and
  reset defaults, every option, Apply/Close behavior, persistence of settings,
  and the resulting operation. If AztecCraft has not implemented it, leave it
  visibly unsupported.
- Toggles and radio/menu choices need their state, default and downstream
  effect recorded. Plug-in-, selection-, mode- and context-dependent enablement
  must be mapped as conditional rather than assumed always active.
- Record the exact menu label/order, accelerator/hotkey, icon/button
  counterpart and platform-specific modifier only when Autodesk documents or
  the pinned Maya runtime confirms it; otherwise mark `PROVISIONAL`.

## Menus outside the main bar

The menu map also covers entry points that are easy to miss when comparing only
the top row:

| Surface / ID | Documented inventory | Coverage status |
| --- | --- | --- |
| View-panel menus `panel.menus` | Autodesk says menus are per view panel and available independently in multi-view layouts; Shift+M toggles panel menus. The page does **not** itemize View/Shading/Lighting/Show/Renderer/Panels trees. | [Panel menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-BD5D066A-4876-4B56-8A9F-96E2243E22A3.htm). **Gap:** every panel menu tree and state. |
| Panel toolbar `panel.toolbar` | Documented button groups: View (Select Camera; Lock Camera; Camera Attributes; Bookmarks; Image Plane; 2D Pan/Zoom; Blue Pencil); Camera Settings (Grid, Film Gate, Resolution Gate, Gate Mask, Field Chart, Safe Action, Safe Title); Shading (Wireframe 4, Smooth Shade All 5, Use Default Material, Wireframe on Shaded, Textured 6); Lighting (Use All Lights 7, Shadows); Isolate Select; Viewport 2.0 effects (SSAO, Motion Blur, MSAA, Depth of Field); X-Ray (X-Ray, Active Components, Joints); Color Management (Exposure, Gamma, View Transform). Right-click actions are individually described for several buttons. Toolbar toggles Ctrl+Shift+M. | [Panel toolbar](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-42224988-F140-4BFF-849B-F89CCB554E13.htm). Buttons vary by renderer; exact icons/order and all popup contents need runtime confirmation. |
| Viewport marking menus `context.viewport.marking` | The [Marking Menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8BA1A3AA-4C44-4779-8B22-0AAE3627E8EB.htm) page names object/NURBS right-click menus, Blue Pencil menu and its submenus, Shift+right-click Polygon Tools, UV/UV Shell, NURBS curve, transform tool menus (Q/W/E/R), input/output (A), common and vertex/edge/face component menus, and sculpting menus. Page supplies named actions and gestures, including object actions Select/Hierarchy/Similar/Make Live/DG Traversal/Inputs/Outputs/Paint/Metadata/Actions/UV Sets/Color Sets/Scene Assembly/material actions; polygon tools Target Weld, Sculpt, Fill Holes, Multi-Cut, Insert Edge Loop, Append, Soften/Harden, Extrude, Offset Edge Loop, Smooth, Subdiv Proxy, Crease, project/split curve, Mirror, Mapping, Triangulate/Quadrangulate, Separate/Combine, Booleans, Reduce, Cleanup, Connect, Quad Draw, Polygon Display; UV tools mapping/cut-sew/grab/cut/sew/delete/create shell/merge/auto seams/texture borders/editors; and component/tool-specific lists. **This row is a partial transcription of the page's named items, not its complete list.** | Official page documents named entries, not complete radial sectors, exact compass placement, all selection-specific variants, or plugin additions. **Runtime-only gaps:** exact sector layout, enabling, omitted entries, gestures in installed config. |
| Other right-click/context menus `context.other` | General principle and selected editor context exist; this audit has no recursive Outliner, editor row, Attribute Editor, Channel Box, UV Editor, Graph Editor, Node Editor, Render View/Setup or Script Editor context inventory. | **Gap:** each editor-specific context menu and condition. Do not infer from viewport menu documentation. |
| Shelf defaults `shell.shelf` | Interface overview documents category tabs, common-task icons, custom shelves and user-created tool/command shortcuts. | [Interface overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm), [Shelves](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-4A21F741-C9AC-4AE5-897E-B6F8C68ADF90.htm). **Gap:** no complete default tab/button list; workspace/config/runtime-dependent. |
| Tool Box `shell.toolbox` | Select Q (Q-click selection-mask marking menu); Lasso (no default hotkey stated); Paint Selection (no default hotkey stated); Move W (W-click marking menu); Rotate E (E-click marking menu); Scale R (R-click marking menu); Last Used Y. | [Tool Box](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-B345E162-0149-4E09-AC98-48DCFC227F33.htm). Page refers to quick-layout buttons separately; full visual inventory requires exact workspace/runtime. |
| Status Line `shell.statusline` | Menu-set selector (F2–F6); selection masks/mode buttons; lock selection; Highlight Selection mode; snapping to grid/curves/points/projected center/view planes; Make Live and recent-live dropdown; construction inputs/outputs popup; Construction History toggle; Render View/render/IPR/render settings buttons; Hypershade; Input Box modes Absolute (default), Relative, Rename, Select by name; the Open Scene icon's right-click Recent Files access is documented on the File-menu page (`file` row). | [Status line](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-86E5CEDA-4100-40AE-8F95-346206CF8456.htm). Default collapsed groups and full masks/icons/order are not text-listed; runtime gap. |
| Sidebar/docks `shell.sidebar` | Modeling Toolkit, HumanIK, Attribute Editor, Tool Settings, Channel Box/Layer Editor; tabs, docking and tab context actions are described in [Interface overview](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-F4FCE554-1FA5-447A-8835-63EB43D2690B.htm). | This names launcher surfaces, not their full editor menu/control inventories. |
| Editors `editor.*` | The interface overview names many editors, but this map does not recursively inventory their independent menus/toolbars/options. | **Gap:** audit each editor separately (Outliner, UV/Graph/Dope Sheet/Time/Node Editors, Hypershade, Reference Editor, Render Setup/View, Script Editor, Attribute/Channel Box, HumanIK, Tool Settings, etc.). |

Autodesk's [Marking Menus](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-8BA1A3AA-4C44-4779-8B22-0AAE3627E8EB.htm)
page documents multiple context-specific marking-menu families, their
triggering gestures and named actions. It is a source for those documented
families, not proof that every radial sector, installed plug-in entry or
selection-dependent menu state is listed.

## Evidence classes

- **Documented:** directly named or described on the cited 2027 Help page.
  Documentation may establish that a menu or interaction exists without giving
  its complete tree, order or defaults.
- **PROVISIONAL:** inferred from a neighboring page, family grouping, or
  incomplete menu overview; retain this label until an exact source or runtime
  observation resolves it.
- **Runtime-confirmed:** observed in the exact Maya build with its workspace,
  preferences, platform and plug-in manifest recorded.
- **Runtime-only:** visible enablement, order, exact option defaults, gestures,
  menu sectors, or plug-in contributions that the online docs do not specify.

The [Animation hotkeys](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-KeyboardShortcuts/files/GUID-0639ADA0-EBE5-4703-A874-92C80E4A0516.htm)
page is an example of a context-specific shortcut source, not a complete
application hotkey catalogue.

## Coverage gaps and completeness statement

Genuinely complete within this pass: the seven always-available menu names and
five workflow-set selector/hotkey facts; the named entries on the audited
File/Edit/Create/Select/Modify/Display/Windows pages (with explicit page limits,
including the Send To child branches this transcription did not fully expand);
the interface overview's named shell surfaces; the extracted Tool Box and
Status Line controls; the panel-toolbar text inventory; and a **partial**
transcription of the named marking-menu items on Autodesk's marking-menu page.
This is **not** a complete Maya UI inventory or exhaustive recursive item
catalogue.

Open source-coverage gaps: Help menu; remaining Edit branches; documented
children of File > Send to Mudbox/Unity/Unreal and other nested Send To
branches; all children of
dynamic submenus (e.g. by type/sets/nodes); option-dialog field/default and
Apply/Reset behavior; full Modeling/Rigging/Animation/FX/Rendering menu trees;
all panel-menu trees; shelf defaults; menu-bar/UI Elements and Preferences
trees; editor-local menus and context menus; Outliner contexts; hotbox; plugin
menus and version/availability; exact visual ordering, option-box icons,
enabled state and radial sectors. Source pages may omit content or expose it
only in images/other linked pages; absence from this transcription is not
evidence of absence from Maya.

Open runtime gaps: no Maya runtime was available. Record build/patch, OS,
locale, workspace/preferences, menu-set, selection/mode/panel context and loaded
plugin manifest before claiming runtime-confirmed coverage. The separately
pinned Maya 2026 acceptance comparison remains unresolved and must not be
substituted with 2027.2 documentation or inferred parity.

Autodesk's [Animation hotkeys](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-KeyboardShortcuts/files/GUID-0639ADA0-EBE5-4703-A874-92C80E4A0516.htm)
is a context-specific shortcut source, not a complete application hotkey
catalogue. Likewise, the interface overview documents surfaces such as shelves,
Tool Box and Status Line without enumerating every default control. Runtime
inventory must record the Maya build, platform, locale, workspace/preferences
and loaded plug-in manifest; menu visibility, order and option defaults may
depend on these.

No implementation status is upgraded by appearing on this map. Existing
AztecCraft state is recorded in [parity.md](parity.md) and the
[UI parity inventory](ui-parity-inventory.md); unsupported entries stay
visibly unsupported.
