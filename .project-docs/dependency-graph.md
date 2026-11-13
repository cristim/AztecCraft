# Dependency graph (DG) model for Maya ASCII history, drivers and instancing

Design note for issue #17. Docs only: nothing here is implemented, no parity is claimed and every
Maya behaviour below is PROVISIONAL until compared with the pinned reference (#5, see
[reference.md](reference.md)). Written against main `1f845f1`.

## 1. Why a DG, and what main has today

Maya ASCII expresses construction history, drivers and instancing as nodes plus `connectAttr`
links (and `parent -add` for instances). Main has no place to put them:

| Fact (source) | Today |
| --- | --- |
| `Decoder::connect` (`src/native_scene/ascii.rs`) | Accepts exactly three literal links (default layer, default render layer, render list). Any other `connectAttr` returns `Unsupported`; each of the three may appear once (bitmask `connections`). |
| `Decoder::create` / `relationship` | Admits `transform`, `nurbsCurve`, `mesh` (document decode only) plus the metadata envelope classes; `relationship` accepts only four fixed `lightLinker1` tuples; `parent`, `addAttr`, `disconnectAttr`, `lockNode` are not parsed. Any other command or `createNode` class refuses the whole candidate. Name lookup (`find`) is exact byte equality over the owners table, so `|a|b` path plugs refuse. |
| `NativeScene` | `nodes` (transforms), `shapes` (meshes), `curves`, `dag: Vec<DagEdge>` (parent/child, so a shape or transform may already have several parents), `units`, `source: SourceRetention` (whole-file bytes plus contiguous statement ranges), materials, session state. No node, plug or connection type. |
| Mesh geometry | Stored as authoritative `NativeMesh` points/faces, read from `setAttr .vt/.ed/.fc`. A mesh-bearing `.ma` is admitted only when the writer reproduces the source byte for byte (`ascii/document.rs:70-100`, adopted as `Authored`, so it is savable); otherwise `Unsupported`. A history-fed mesh can never pass this gate, so history needs its own admission path (section 6). |
| Save | Opened `.ma` is read-only; opened `.mb` saves by patching retained bytes (`SourceRetention.leaves`). The authored writer (#156) emits fresh scenes only and has no history. |
| MCP | `inspect_scene` shows nodes, curves and counts; no connection or history data. |

Consequence: a `polyCube` node feeding `pCubeShape1.i` is not inert. The shape's points exist
only as the output of the node, so admitting the scene means evaluating the node into geometry.
That is why history was deferred (#17 comments of 2026-11-12). Drivers (animCurve, expression,
setDrivenKey) add time or script evaluation and stay out of scope here (section 9).

## 2. Principles

1. Inert by default for data, default-deny for effects. A node or statement is retained as source
   bytes and never interpreted or executed (scripts and expressions stay data, per AGENTS.md), but
   only when it is proven unable to change anything the viewer or evaluator reads.
2. Parsing never widens admission. A statement is admitted only by an explicit allowlist entry.
3. Evaluation is whitelist-only, pull-based, bounded and finite-checked.
4. Derived geometry is never the save model. The DG data is additive, rebuilt from the source on
   Open, and empty when `source` is `None`.
5. No dummy tools: nothing is advertised through UI or MCP until a slice implements it.

## 3. Minimal typed model

Build on, not beside, the decoder's per-node table (`Builder.owners` with `Kind`, `ascii.rs:324-339`).
That table lives only inside `decode_with_uvs` and is dropped after decode, so it stays the
parse-time index and the graph keeps a compact retained node list (`DgNode`: class and name as
source ranges, the `createNode` statement range, and the `NativeId` for Known nodes) for the
lifetime of the scene; `inspect_graph` needs it. The DG adds only what that table lacks, in a new core module
`src/native_scene/dg.rs`. Sketch, not final:

```rust
pub struct DgNodeId(u32);                  // dense index into DgGraph.nodes, never from file text
pub enum NodeClass { Known(KnownClass), Inert(InertClass) }
// Known: Transform, Mesh, NurbsCurve, plus per slice PolyCube, PolySphere.
// Inert: an allowlisted DG-only class (section 4), class word kept as a source range.
pub struct AttrPath(Vec<AttrSeg>);         // "iog[0].og" -> [Name, Index(0), Name]
pub struct Plug { node: DgNodeId, path: AttrPath }
pub struct Connection { src: Plug, dst: Plug, flags: ConnFlags, statement: Range<usize> }
pub struct DgNode { class: NodeClass, name: Range<usize>, statement: Range<usize>, native: Option<NativeId> }
pub struct DgGraph { nodes: Vec<DgNode>, connections: Vec<Connection>, typed_attrs: Vec<(DgNodeId, AttrPath, f64/i64, Range<usize>)> }
```

Rules:

- Plug text is parsed once with checked arithmetic: segment count, index (u32) and total length are
  capped. Node names resolve exactly as today (exact bytes through `find`); a plug naming a node
  by DAG path, an ambiguous or missing node is `Malformed` or `Unsupported`, never guessed.
- Known classes carry a static attribute table (long and short name, type, default, unit storage)
  from public docs; `w` and `width` are one plug. An attribute absent from the table is unknown: a
  setAttr on it is retained only if the class's allowlist says it is inert, and evaluation refuses
  if it reads one.
- Inert nodes hold no typed values. Their setAttr statements, including quoted script bodies, are
  retained ranges only; MCP shows class word and name, never values.
- `NativeScene` gains `dg: DgGraph`. The existing `shapes`, `curves`, `nodes` stay the authority
  for geometry and transforms; the DG refers to them by `NativeId`, so there is one model.
  `dg` is empty and validated empty when `source` is `None` (authored projection, clones, undo).
  Debug is redacted like `SourceRetention`.

What breaks if deleted: without `Plug`/`AttrPath` a link can be matched only by whole-string
equality, which is why `connect` cannot grow today; without `statement` ranges the connection list
cannot be checked against `SourceRetention.records`.

## 4. Admission policy

Each `connectAttr`, `setAttr`, `createNode` and `parent` statement is classified, in order:

1. The existing three metadata links, `relationship` tuples and envelope nodes: unchanged.
2. Both endpoints are a whitelisted pair (section 5): admitted and typed.
3. **Any destination on a Known node** (transform, mesh, curve, whitelisted history) that is not
   on an explicit inert list is refused `Unsupported`. This is default-deny: it covers channels,
   `visibility`, `offsetParentMatrix`, `inheritsTransform`, `drawOverride`, geometry inputs,
   material assignment (`iog[n]` into a non-initial shading group), dynamic attributes and
   anything not yet thought of. The inert list starts empty beyond today's links and grows only
   with evidence.
4. Neither endpoint is a Known node (a link between two Inert nodes): admitted, retained.
5. Anything else: `Unsupported`.

Inert classes are an explicit allowlist of DG-only node classes (initial candidates: `script`,
`groupId`, `objectSet` without members on admitted nodes; the final list is slice 1's research
output, each with a documented page or a Maya sample). These classes are inert only while unconnected:
membership links (`groupId`, `objectSet` into `iog`) have a Known endpoint and refuse under rule
3, so slice 1 promises synthetic unconnected samples, not realistic Maya scenes. "Unknown class" is not "inert": a world
level `createNode joint`, `ikHandle` or any transform subclass is a visible DAG object, and stays
refused, as does any node with `-p`.

In slice 1 every `setAttr` without a leading `.` refuses (today's transform setAttr is
relative-only, `ascii.rs:654-675`); a plug with an explicit node name (`setAttr "pCube1.tx" 5`)
addresses that node in Maya whatever was created last, so it can never be "retained and ignored".
A later slice may admit it only by classifying its target node. Relative `setAttr` on an Inert node
is retained as a range. Flags use the existing `attr_flags` parser
(`ascii.rs:266-294`); flags it refuses today (`-l`, `-cb`, `-av` and similar) keep refusing until a
slice adds them with evidence.

Time-dependent sources (`time1.outTime`, animCurve outputs, expressions) hit rule 3 whenever they
reach a Known node and so refuse until a time slice exists. `addAttr`, `disconnectAttr`,
`lockNode`, `file -r` and every other command keep refusing.

What breaks if rule 3 is deleted: `connectAttr "animCurveTL1.output" "pCube1.tx"` or an
`offsetParentMatrix` link opens as the static value, a wrong scene shown as right. What breaks if
the allowlist is replaced by "unknown means inert": a joint or ikHandle disappears from view.

## 5. Evaluation strategy

Scope: a fixed whitelist, each entry with a documented node page, a test oracle and an evidence
checklist (section 8).

| Class | Output plug into | Semantics source | Notes |
| --- | --- | --- | --- |
| `polyCube` | `<mesh>.i` (inMesh) | Maya Tech Docs `Nodes/polyCube` | Docs list `w/h/d` (1.0 cm), `sw/sh/sd` (1), `cuv` (3), `tx` (1). The page states neither the output attribute name nor an axis attribute; slice 3 confirms both against the reference. Vertex, edge and face order are not in the docs (section 8). |
| `polySphere` | `<mesh>.i` | `Nodes/polySphere` | Docs list `r` (1.0 cm), `sa` (20), `sh` (20), `cuv` (2), `tx` (1). Same gaps. |
| shape instance edge | `parent -add` | Maya `parent`/`instance` docs | Not a node evaluation: a second `DagEdge` for an existing shape. `transform_paths` already enumerates instance paths under `MAX_TRANSFORM_PATHS` and `MAX_TRANSFORM_DEPTH`. `parent` is not parsed today, so this needs a new command. The ASCII form Maya writes (`parent -s -nc -r -add`) is recalled, not read: PROVISIONAL, slice 5 reads and cites the pages first. |

Evaluation reuses the existing generators instead of new ones: node attributes map onto
`PrimitiveOptions` and run through `PrimitiveOptions::validate` (`primitive_options.rs:215`:
subdivisions 1 to 128, sphere axis 3 to 128, height 2 to 128, `MAX_FACES` 20,000), with the cube
built by `NativeMesh::with_options` and the sphere by `polygon_sphere`
(`mesh/sphere.rs:7`, `pub(super)`, so slice 4 widens its visibility). Values the validator
rejects (sphere `sa` or `sh` below its minimum, non-finite or non-positive radius and sizes) refuse
`Unsupported`; the document does not invent a second set of limits. Both generators' Maya vertex
order is itself provisional.

Algorithm, written for the first node that takes an input (slices 3 and 4 are one hop: creation
nodes have no inputs and mesh-to-mesh links are refused by rule 3, so cycle and depth checks are
unreachable there and are not built in those slices):

1. Roots are shapes whose geometry plug has an incoming connection.
2. Build the upstream set with an explicit worklist, never recursion. Colour white/grey/black; a
   grey hit is a cycle and refuses `Unsupported`.
3. Evaluate in topological order; counts use `checked_mul`/`checked_add` before any allocation.
4. Write the result into the shape's `NativeMesh` marked derived. No new edit restriction is added:
   every ASCII-sourced scene already refuses edits (`authoring_read_only_reason`,
   `native_scene.rs:1313-1318`); a per-shape `restrictions` entry (getter `edit_restrictions()` at
   `native_scene.rs:476`) belongs to slice 6, when edits first become possible.
5. Geometry conflict: a shape whose geometry plug has an incoming connection and also carries
   authored geometry setAttrs (`.vt`, `.ed`, `.fc`; `finish_meshes`, `ascii/mesh.rs:311-319`)
   refuses `Unsupported`; the evaluator never overwrites authored parts. Real Maya history shapes
   carry further mesh setAttrs (`.uvst`, `.cuvs`, `.dcc` and similar) that the mesh parser likely
   refuses, so slice 3 proves synthetic scenes only until the Maya sample in section 8 shows what
   Maya writes.

Bounds: every visit and generated element charges the existing `Work` meter; exhaustion is
`Resource`, never a partial scene. The node-count and depth constants (starting 4,096 and 64) are
introduced with the first slice that has an input-taking node, with a boundary test built from
that node. Evaluation uses no time input. A non-centimetre linear unit refuses a history scene
like curves do (`ascii.rs:1260`); the bounding-box oracle is therefore in centimetres. No
`unwrap`/`expect`/panics on input-reachable paths; non-finite results refuse `NonFinite`.

What breaks if the whitelist is deleted: any unknown history node is evaluated as identity,
silently wrong geometry. What breaks if the cycle/depth machinery were built now: nothing, which
is why it waits.

## 6. Open/Save round-trip stays lossless

- Retention is unchanged: `SourceRetention` keeps every byte and contiguous statement ranges (the
  decoder keeps the whole `Arc` plus `records`, `ascii.rs:1198-1213`). Unknown bytes stay exact
  because a `.ma` with unadopted content is never rewritten. `Connection.statement` and typed
  attribute ranges index that `records` list and `SourceRetention::validate` checks them.
- **History admission path (new, slice 3a).** Today a mesh `.ma` is either writer-identical
  (`Authored`, savable) or refused. A history-fed scene is neither, so it is admitted as a
  third, read-only outcome: `Retained` with derived shapes. Its proof is not byte identity but:
  every statement is recognized (rule 2 to 4), the evaluated mesh counts and bounds are finite and
  within limits. It is never adopted as `Authored`,
  so Save and Save As of it stay refused with the same read-only reason (any ASCII-sourced scene is read-only already);
  the code change is the `AsciiDecoded::Retained` contract (its doc comment says mesh-free,
  `ascii/document.rs:26-27`) and a branch in `decode_document` before `Err(unsupported())`
  (`document.rs:87-92`) taken only when the DG holds evaluated shapes. The history path needs no
  adoption, but to avoid opening on Windows what plain mesh files refuse there
  (`adoption_supported == false`, `document.rs:94-99`), it stays gated by `adoption_supported` too
  and refuses `AdoptionUnsupported` on Windows until a reason to differ is documented; this must not weaken
  the existing adoption gate for non-history scenes (regression test: a plain mesh scene still
  requires byte identity). The same applies to Inert nodes in mesh scenes: they also fail byte
  identity today, so slice 1 helps curve-only and empty `.ma` files only; mesh scenes with
  Inert nodes wait for slice 3's path.
- Writer-identical mesh `.ma` adopted as `Authored` has an empty DG by construction (the writer
  emits no history and no Inert nodes); `validate` asserts it.
- Opened `.ma` otherwise stays read-only ("Maya ASCII writing of opened `.ma` scenes:
  unsupported", [mcp.md](mcp.md)). The DG is never serialized.
- Binary connections are not covered; a DG for `.mb` needs its own decode slice and evidence.
- A later edit of a history-fed scene (slice 6) targets input attributes (e.g. `polyCube.w`) as
  source-bound intent, re-derives outputs, and refuses anything not provably re-writable.
- Gate for every slice: Open leaves retained bytes identical, the same candidate without the new
  statements opens identically, and a refused Open keeps the baseline scene.

## 7. MCP exposure (read-only first)

- One new read-only tool, `inspect_graph` (Maya map: `listConnections`, `listHistory`, `nodeType`;
  PROVISIONAL), in its own slice after retention, not a widening of `inspect_scene`, so that
  tool's reply budget is untouched.
- It requires `scene_token` (only `inspect_scene` is token-free, mcp.md:56) and goes through the
  busy and modal guards. Reply: node list (`id`, `name`, `class`, `known`, in/out degree), connection
  list (`source`, `destination`, `evaluated`), paged with `node_limit`/`cursor` and the stale
  cursor rule of `inspect_scene`, hard caps on every list. Never CV arrays, script bodies,
  Inert-node values or paths. Evaluated nodes add their typed inputs and derived output counts,
  marked `derived`.
- No mutating graph tool (`connect_attr`, `disconnect_attr`, history delete) until an edit slice.
- Same-change obligations: synchronized schema, `mcp-tools.md`, coverage-inventory rows, real
  Editor protocol tests (token, draft, modal, close, busy), a baseline-preserving refusal test,
  and updates to the `open_scene` row and the Files inventory row whenever admission widens
  (they currently say read-only static curve `.ma`, mcp.md:104).
- No UI; Outliner and Node Editor surfaces stay visibly unsupported. Slices 1a to 5 are provable
  headlessly (display locked); no native visual check is claimed.

## 8. Staged slices, oracles and reference evidence

Each slice is at most about 400 changed lines, lands separately, and states its limits in
`development.md`. Oracles are independent of the code under test; each fail-before is "this
scene is refused or reads differently on main".

| # | Slice | Test oracle (fail-before) | Evidence required from the pinned reference (#5) before any parity claim |
| --- | --- | --- | --- |
| 1a | Inert allowlist plus `createNode` and relative `setAttr` retention for curve-only and empty `.ma`, with the `DgNode` list (no new links). About 250 lines with tests. | A curve scene with an allowlisted Inert node opens (refused on main) with curve results equal to the same scene without it; world-level `joint`, `-p` unknown class, absolute-plug setAttr, flags the parser refuses all refuse and keep the baseline; mutation: replacing the allowlist by "unknown is inert" must fail the `joint` test. | A Maya-written sample for each allowlisted class showing what Maya emits and that it does not affect display; until then "retained, not interpreted". |
| 1b | `AttrPath`/`Plug` parser, typed `Connection` list replacing the `connections` bitmask for the existing three, rules 3 to 5 as default-deny. About 300 lines. | Existing tests unchanged; `offsetParentMatrix`, `drawOverride`, `tx` links, duplicate links, over-long or index-overflow plugs refuse and keep the baseline; mutation: removing the default-deny must fail the `offsetParentMatrix` test; Inert-to-Inert link admitted. | None beyond 1a. |
| 2 | Read-only `inspect_graph` over slices 1a and 1b's graph. | Official-client tests: paging, stale cursor, caps, missing or stale `scene_token`, busy/modal/draft guards, no values or script bodies in replies. | None beyond 1a. |
| 3a | History admission outcome (`Retained` with derived shapes, the `adoption_supported` gate, geometry-conflict refusal) with an attribute table and the `PrimitiveOptions` mapping, proven first by `polyCube`. About 400 lines; split the admission outcome from the polyCube table if it exceeds that. | A synthetic polyCube-fed `.ma` opens (refused on main) with V = 2(sw*sh+sw*sd+sh*sd)+2, F = 2(sw*sh+sw*sd+sh*sd), Euler 2, bounding box (w,h,d) cm; out-of-range subdivisions or sizes, `MAX_FACES` boundary, non-cm unit, mesh-to-mesh link, unknown input and a connected plug plus authored `.vt` all refuse with baseline kept; Save refuses with the read-only reason; plain mesh scenes still need byte identity. Compare with the native cube only for counts and bounds. | (a) Maya-written `.ma` of a default polyCube with history and of the same cube with history deleted, same units: vertex and face order, output plug name (`out` or other), axis, `cuv`/`tx` meaning and the extra mesh setAttrs Maya writes. Order parity stays PROVISIONAL until (a). |
| 4 | `polySphere` (widen `polygon_sphere` visibility). | V = sa*(sh-1)+2, F = sa*sh, vertices at radius r (poles at (0,+-r,0) for the +Y axis only); minimum and cap boundaries; counts alone already pass on the generator, so the fail-before is the scene opening. | Maya-written default polySphere with and without history: vertex order, pole handling, axis. |
| 5 | `parent` command and shape/transform instance edge into `dag`. | One mesh under two transforms gives two `transform_paths` entries with distinct world transforms; an instance cycle refuses; `MAX_TRANSFORM_PATHS` boundary; MCP `inspect_scene` lists both paths; either is selectable. | Maya-written scene from `instance` on a transform and on a shape: exact ASCII statements, per-instance storage (`instObjGroups[n]`) and naming. |
| 6+ | Each with its own design addendum: history edits through input attributes, node-count/depth bounds with the first input-taking node, curve creators (`makeNurbCircle`), then drivers once a time model exists. | Per addendum. | Maya reference for evaluation order, time and expression semantics. |

Parity statement rule: until a slice's evidence cell is satisfied, its `development.md` entry and
MCP schema text say PROVISIONAL, "not compared with Maya", as units did (b347baf).

Slices 3 to 5 depend only on slices 1a and 1b (slice 2 depends on 1a and 1b; 3 to 5 do not depend on 2); slice 5 may land first. All touch
`src/native_scene/ascii.rs` and `NativeScene`, near #156/#166 work, so each fetches and rebases
before committing and avoids files in open PRs.

## 9. Out of scope, with reasons

- Drivers (animation curves, expressions, set-driven keys): need time or script evaluation;
  scripts never execute. They refuse under rule 3.
- Binary connection decoding, deformers (skin, blend shape), history on NURBS creators and any
  graph mutation.
- Node Editor and Outliner graph surfaces.

## 10. Sources consulted

- polyCube node: <https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/polyCube.html>
  (read 2026-11-13; `w,h,d,sw,sh,sd,cuv,tx`; no axis or output name listed).
- polySphere node: <https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/polySphere.html>
  (read 2026-11-13; `r,sa,sh,cuv,tx`).
- Not read for this note: the `parent`, `instance`, `setAttr` and `connectAttr` command pages and
  the node pages of the candidate Inert classes. The slice that depends on them reads and cites
  them first.
