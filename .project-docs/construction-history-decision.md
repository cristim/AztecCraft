# Construction history: bake with undo first (issue #45, slice 1)

Decision record. Docs only: no code changes, no parity claim, and every Maya behaviour named here
is PROVISIONAL until compared with the pinned reference (#5, see [reference.md](reference.md)).
Written against main `de2e2bd0`; file:line anchors below were checked on that commit.

## Decision

Modeling operations bake geometry directly into the mesh and are undone through the existing
`History`. No construction-history node graph is built in this slice. Nodes (editable operation
parameters, Channel Box INPUTS, `constructionHistory` toggle) are a separate, later decision, to be
taken with the Bevel and Loop designs in hand and after the read-only DG slices of
[dependency-graph.md](dependency-graph.md) land. Issue #45 stays open as the tracker for that.

## What the code does today

- Extrude bakes. `Command::ExtrudeFace { shape, face_index, distance }` (`src/history.rs:85`)
  runs `candidate.extrude_face(...)` (`src/history.rs:589-595`), which replaces the shape's mesh
  with `Mesh::extruded_face` output (`src/native_scene.rs:1860-1880`, `src/mesh.rs:65`). Nothing
  records the operation or its distance after the call returns.
- Undo is whole-scene snapshots. `History::execute_labeled` clones the scene, applies the command
  to the clone (`src/history.rs:510`) and records an `Entry { scene, label, pose_frame }`
  (`src/history.rs:217-223`, `src/history.rs:653-666`); `undo`/`redo` swap snapshots back
  (`src/history.rs:689-722`) through `restore_authoring` (`src/native_scene.rs:1255`). The queue is
  Finite, default 50 steps (`UNDO_LIMIT`, `src/history.rs:238`). An extrude is one step labelled
  "Extrude" (`src/history.rs:148`); inside an open chunk it merges into the chunk
  (`src/history.rs:658-659`). A baked operation therefore gets undo, redo, labels and the failure
  guarantee (a refused command leaves scene and history untouched) with no new machinery.
- One shared entry point. The native Extrude button (`src/app/features/extrude_options.rs:107-123`)
  and the MCP `extrude_face` tool (`src/mcp/features/extrude.rs:27-37`) both build
  `Operation::ExtrudeFace` (`src/editor_operations.rs:682`). A history node would have to be created
  at that one seam, which keeps the later change small.
- Edits need an authored scene. `structural_edit` refuses any scene with a parsed source
  (`src/native_scene.rs:1437-1443`), and the save authority exists only when `source` is `None`
  (`src/native_scene.rs:808-811`). Extrude additionally refuses meshes with UVs
  (`src/native_scene.rs:1873-1875`).
- No DG exists. `.project-docs/dependency-graph.md` section 1 records that the decoder admits only
  three literal `connectAttr` links, that `NativeScene` has no node, plug or connection type, and
  that the authored writer emits fresh scenes with no history. Its section 6 keeps any history-fed
  scene read-only, and its slice 6 (history edits through input attributes) is where editable
  parameters would arrive.
- Documents do not persist history: undo history is in-memory only (`.project-docs/parity.md:36`,
  "documents don't persist history/camera state"; that sentence also says "History is unbounded",
  which is stale against `UNDO_LIMIT` and is not relied on here). The Polygon modeling row lists
  "no construction history" among unsupported items (`.project-docs/parity.md:15`).

## Why bake first

1. Nodes need a graph the editor does not have. A node for Extrude means typed nodes, plugs,
   connections, an evaluator and a decision on what the mesh shape stores (derived output versus
   authoritative points). `dependency-graph.md` stages that over slices 1a to 6 and keeps it
   read-only until slice 6. Building a parallel node store for Extrude now would be thrown away or
   would fix the graph shape before those slices are reviewed.
2. The save model cannot carry nodes yet. Meshes are stored as authoritative points and
   the authored writer emits fresh scenes with no history (`.project-docs/dependency-graph.md:17-18`;
   the writer's contents are in [binary-authored-writer.md](binary-authored-writer.md)); Maya
   acceptance of even that is unproven.
   Writing `polyExtrudeFace` nodes with `connectAttr` links would be new, unverified file grammar.
3. Snapshot undo already satisfies the acceptance criterion that failed operations preserve the
   baseline. Nodes would add parameter editing, which no current caller needs.
4. Maya documents history as optional: the Status Line toggle and `constructionHistory -toggle off`
   (issue #45 sources S1, S3), the per-command `-constructionHistory` flag on poly commands (S5,
   S6) and `delete -constructionHistory` (S7). A baked result is therefore a state Maya users can
   produce. Maya's default toggle state is unverified, and AztecCraft always behaves like
   toggle-off. No parity claim is made.

## Effect on the Maya save model

- Saved `.ma` and `.mb` geometry is the baked result: the mesh points and faces after the
  operation, as the authored writer already writes them (`dependency-graph.md` section 1,
  "Save" row).
- Nodes are not saved as Maya construction history in this slice. No `polyExtrudeFace`,
  `polyCube` or other poly node, and no `connectAttr` to `inMesh`, is written. Undo history stays
  in memory and is not saved.
- A Maya-authored file whose meshes are fed by history nodes is unchanged by this decision: it
  stays refused or read-only as `dependency-graph.md` section 6 describes, and any later admission
  of it is that document's slice 3a, not this one.
- Consequence for users: after reopening a saved scene, an operation can no longer be adjusted;
  only the geometry remains. The UI and MCP text must not imply otherwise. Controls for history
  (Channel Box INPUTS stub #60, Construction History toggle, Inputs/Outputs, Delete History) stay
  visibly disabled with a reason until nodes exist, per the #45 acceptance criteria.

## What Bevel and Loop design needs from this decision

- Bevel (a new issue, not yet filed) and Loop tools are designed as baked, undoable `Command`
  variants in the same shape as `ExtrudeFace`: a typed `Operation`, a `Command` arm in
  `src/history.rs`, a core mesh function returning a new mesh, one shared path for native UI and
  MCP, and refusal on sourced scenes and on UV-bearing meshes until UV handling is decided.
- Their options (bevel width, segments, loop position) are taken as operation parameters and are not
  retained afterwards. The option windows must not claim editable history.
- Keep each operation's core function pure (mesh in, mesh out, finite checks), as
  `Mesh::extruded_face` is (`src/mesh.rs:65-75`), so a later node can wrap it as its evaluator
  without rewriting it. That is the only forward-compatibility cost this decision asks for.
- Do not add node, plug or "history" fields to `Command`, `Operation` or the MCP schemas ahead of
  the node decision (no dummy tools, no speculative abstraction).
- Bevel and Loop need no change to undo: they inherit the snapshot queue, so the 50-step default
  and `UNDO_LIMIT_MAX` memory bound for a Finite queue (`src/history.rs:238-242`) apply to them as well.

## When to revisit

Revisit when the DG read-only slices (`dependency-graph.md` section 8, slices 1a to 3a) are on main
and a Maya reference sample of a history-fed `polyExtrudeFace` file exists under #5. The revisit
decides: node store shape, whether mesh shapes keep authoritative points beside the node output, the
`.ma`/`.mb` writer grammar for history, and the Construction History toggle default. Until then
this record stands.
