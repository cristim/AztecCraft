# Implementation plan

Status: early native prototype implemented, with remaining workflow and verification work below. The user has authorized prototype publication and merge to `main`.

## Product and publication gate

The source-backed cross-family scope and staged delivery gates are in the
[Maya 2027 feature and API roadmap](maya-2027-feature-roadmap.md). It uses Maya
2027.2 as the current documentation lens only; it does not replace the Maya
2026 acceptance target or claim the family inventories are complete.

Final objective: a Maya-identical Rust desktop application. Work in incremental commits. Historically the user required verified 80–90% coverage of **all Maya 2026 functionality, including simulation, rendering, scripting, and plugin compatibility** before the first GitHub push. The subsequent explicit instruction to push/merge to `main` supersedes that first-push restriction and authorizes early prototype publication. The full-functionality target and exact parity remain unmet acceptance requirements. Expand and pin the complete workflow inventory before reporting any percentage, without redefining it as a small modeling subset.

The integrated prototype contains scene edits, command history, validated JSON
documents, geometry-only OBJ export, native shell and projected viewport.
New/Open/Save As and File > Export All / Export Selection (OBJ) are connected to the UI.
Native file-dialog interaction, cross-platform packaging and Maya reference
comparison still need verification. The numbered tasks below retain the original
implementation sequence and proof obligations rather than imply completion of
every subcase.

## Atomic local tasks

1. Reference and scaffold: `AGENTS.md`, `README.md`, `.gitignore`, `.editorconfig`, `.project-docs/*`, `known-issues.md`. Evidence: source links, truthful scope, no parity/publish claim; two independent review passes. Commit before source implementation.
2. Scene model/transforms: `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/scene.rs`, `tests/scene_workflows.rs`, build targets and development instructions. Stable IDs, cube/plane geometry and transform editing for the desktop editor. Proof: topology, transformed vertices, ID uniqueness and invalid-input rejection. No speculative graph/plugin abstraction.
3. Undo/redo: `src/history.rs`, `tests/history_workflows.rs`. Desktop editing commands require reversible create/delete/transform actions. Proof: multi-action undo/redo, redo invalidation, failed-action nonmutation, exact scene restoration.
4. Persistence: `src/document.rs`, `tests/document_workflows.rs`. Versioned JSON for desktop save/open. Initial saves are create-new/no-clobber; occupied paths retain original bytes. Proof: round trips, malformed/version-invalid files, occupied paths, failed-open nonmutation, staged-write failures. Reuse a suitable inspected save library rather than silently copying its implementation.
5. OBJ export: `src/obj.rs`, `tests/obj_workflows.rs`. Desktop export writes world-transformed polygon geometry. Proof: multi-mesh vertex offsets, transformed vertices and malformed-name handling; no import/format compatibility claim. The native File menu items and MCP `export_obj` now write NativeScene geometry (see the issue #22 section).
6. Native shell: `src/main.rs`, `src/app/mod.rs`, dependency lock and instructions. Dense dark provisional Maya-style menu/shelf/Outliner/Channel Box UI; real create/select/numeric editing and history, unsupported controls explicit. Proof: native build/startup, interaction checks. This spike does not establish pixel parity or docking completeness.
7. Viewport: `src/viewport.rs`, `tests/viewport_workflows.rs`, `src/app/mod.rs` integration. Orbit/pan/dolly, projection, primitive drawing, picking and framing. Proof: projection/picking tests and native navigation/selection; projected drawing is provisional, not a depth-tested production renderer.
8. File/UI integration: `src/app/mod.rs`, native workflow checks. Connect save/open/export actions to tested scene APIs, surface errors, preserve current scene on invalid open and protect unsaved work. Proof: UI create/edit/undo/save/reopen/export journey. Export is connected; the full journey through native dialogs remains to be verified.
9. Modeling vertical slices: topology/component selection and extrusion, construction history, bevel/weld/loops and normals, each independently committed and reviewed. Test topology validity and realistic editing workflows before broadening tools.
10. Subsequent independently reviewed slices: UV, animation, rigging, materials, renderer, interchange, scripting and extended systems from `parity.md`. Plan each with exact workflows and reference evidence before writing code.

Tasks 2–8 have real callers: desktop authoring actions and scene file handling. Scene data remains separate from viewport caches so persistence and editing are testable without a window. Projected drawing is a fidelity spike, not a replacement for a depth-tested renderer.

## File > Export All / Export Selection as OBJ (issue #22)

Maya 2026 research (cited and summarised, not copied):

- [Export objects to a new file](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ManagingScenes/files/GUID-664060EB-EA35-4CD3-83EB-CBCAF59B4495.htm):
  File > Export All writes the whole scene and File > Export Selection only the selection, in a
  chosen format; their Export Options set the format. OBJ is not native and needs the objExport
  plug-in. No hotkey is documented.
- [Export All/Selection Options](https://help.autodesk.com/cloudhelp/2025/ENU/Maya-ManagingScenes/files/GUID-A1E550E4-1510-4D45-A811-5E424A8D3924.htm)
  (2025 page; the 2026 page exists in Japanese with the same GUID): the options are reached from the
  two File menu items; Include Children defaults on; Export Selection also writes the selected
  nodes' parents; File Type Specific Options change with the format.
- [Supported data export formats](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ManagingScenes/files/GUID-864BD203-C437-4481-8BFC-3A6C1D2C824C.htm):
  OBJ export comes from the objExport plug-in, extension `.obj`.
- [Export All/Selection Options, 2027](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-A1E550E4-1510-4D45-A811-5E424A8D3924.htm)
  (the pinned release): repeats that Include Children defaults on and that the parents of selected
  nodes are exported, and states no object order. Parent meshes are unmet (`known-issues.md`).

Slice: both File menu items write world-space polygon geometry of the authoritative NativeScene
to a new `.obj` file (never overwrite), through `Operation::ExportObj`, shared with MCP
`export_obj`. Export Selection covers every selected object and its descendants (Include Children on).
The option boxes are shown disabled. PROVISIONAL or unmet: the OBJ option defaults (groups,
point groups, materials, smoothing, normals) are not stated in the pages above; hidden or
non-displayed instances are skipped, while the docs do not say whether Maya's Export All includes
them; parent transforms are folded into world positions rather than written as groups; output
has not been compared with Maya's objExport. OBJ import is a later slice.

## File > Import as OBJ (issue #22 remainder)

Maya research at the 2027 pin (cited and summarised, not copied; the pages were read through a
summarising fetch, so a reviewer should open them before the PROVISIONAL list is frozen):

- [Import files](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ManagingScenes/files/GUID-BB7ABDA1-86E6-4A83-B8E6-3F4D7F1B0F1B.htm):
  File > Import brings the file's data into the current scene (it adds, never replaces). Format
  options live in the Options panel of the Import browser or the option box beside File > Import.
  The page does not cover groups, OBJ or what is selected afterwards.
- [Import Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-37B31221-7BBC-40DA-AB33-47C333EF4E14.htm):
  Group (default off) puts the imported objects under one transform; Remove duplicate shading
  networks; a file type list with Best Guess. OBJ is not mentioned and no hotkey is stated.
- [Supported data import formats](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ManagingScenes/files/GUID-69BC066D-D4D8-4B12-900C-CF42E798A5D6.htm):
  OBJ is a data import format, extension `obj`, plug-in cell blank; materials, normals, groups and
  units are not described. Whether the objExport plug-in must be loaded to import is not stated.
- [Open Options](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-203FC12C-6C80-497C-AC29-548DF6EC91B5.htm):
  the only page that describes OBJ reading: Multiple Objects makes one shape per group, Single
  Object one shape with an object set per group; overlapping groups make Maya fall back to Single
  Object. The default is not stated. Assumed to apply to Import as well (PROVISIONAL).
- [File menu](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm):
  the item order was not re-read, so the menu slot (Import... with a disabled option box between
  Save As and Export All) is PROVISIONAL.

Slice: File > Import... reads an OBJ and adds one root transform plus one mesh shape per `o` or
`g` group (Multiple Objects semantics), in one undo step, through `Operation::ImportObj` shared
with MCP `import_obj`. Accepted: `v`, `f` with 3 or more corners (all index forms, negative
indices), `o`, `g`, comments. Recognised and dropped, counted: normals, UVs, materials and other
non-geometry records. Refused with a typed error, never silently lost: free-form geometry, unknown
keywords, overlapping groups, bad indices, degenerate faces, non-finite or oversized numbers, files
with no face, and anything the viewport would refuse to display (concave or holed faces with 5 or
more corners). Import into an opened `.mb` or `.ma` is refused, as Create is.

PROVISIONAL (no page states them; each is pinned by a named test and listed in `known-issues.md`):
units of OBJ numbers; normals, UV and `.mtl` handling; negative indices; triangles and n-gons;
object naming and clash suffixes; face winding; unused vertices; whether the imported nodes are
selected afterwards; the undo label "Import"; the Multiple/Single default; coordinate and size
limits (`MAX_*` in `src/obj/import.rs`, rows in "Application policy constants"). Out of slice:
the option box, Group, Single Object with sets, UV/normal/material import, other formats.

Status: implemented for #22's first slice (reader, scene import, history command, File > Import...,
MCP `import_obj`, round-trip tests). INT-016 is Partial and the option box is INT-040 (Missing).

## Verification and review

Keep the same non-author reviewer across the two plan passes and each two-pass diff gate. Use a cold independent verifier for the completed vertical slice. Run formatting, Clippy, headless workflow tests, and native build. Record exact tree/commit identifiers for reviewed work. Reference comparison is blocked until a Maya installation/reference manifest exists. No positive parity claim without it.

## Isolation

Retain isolated worktrees for active changes, record ownership and reviewed hashes in the session plan, and preserve existing state. The current user instruction authorizes the prototype's GitHub publication and merge to `main`. Keep unsupported functionality and unmet acceptance criteria explicit when publishing.
