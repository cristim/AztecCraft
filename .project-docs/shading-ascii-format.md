# Shading storage contract for authored Maya ASCII (issue #156, part 2)

Status: design note, written before the code. The ASCII writer does not emit shading yet (it refuses
`AsciiWriteRefusal::SessionMaterials`); the reader accepts only the three layer connects. This file fixes the exact
statements both sides will use, so the reader and writer tasks land in a safe order. Nothing here has been compared with
Maya (#5). No Maya install and no Maya-written `.ma` with materials is available locally, so **whether Maya loads these
statements is unproven.**

## Documented (Maya 2027 pin, reference.md)

- Nodes/objectSet.html (parent of `shadingEngine`) documents the attributes `dagSetMembers` (`dsm`), `partition` (`pa`) and
  `renderableOnlySet` (`ro`, "should not be changed directly"), and says that connecting a DAG node's `instObjGroups` to
  `dsm` puts the whole object in the set. The same page documents that shading groups connect to `renderPartition`. So the link `<shape>.iog -> <set>.dsm` is documented, not PROVISIONAL.
- CommandsPython/connectAttr.html documents `connectAttr` and its `-nextAvailable` (`-na`) flag for multi attributes.
- Commands/createNode.html (MEL) documents `createNode <type> -n <name>`; Nodes/lambert.html documents `color` (`c`) and
  `outColor` (`oc`); Nodes/shadingEngine.html documents `surfaceShader` (`ss`).
- Nodes/materialInfo.html describes hardware-texturing bookkeeping. It does not say the node is required.

Summarised, not copied.

## Statements AztecCraft reads and writes

Default link, one per shape that has no session material (this includes every shape of a scene without materials):

```
connectAttr "<shape>.iog" ":initialShadingGroup.dsm" -na;
```

Session material `lambertN` with group `lambertNSG` (N >= 2; `SessionMaterials::add` generates both names), one pair per
session material after the locked material 0, then one link per shape assigned to it. The `<material>SG` pattern matches
Maya-written files, but no Autodesk page documents it (PROVISIONAL):

```
createNode lambert -n "lambertN";
	setAttr ".c" -type "float3" r g b;
createNode shadingEngine -n "lambertNSG";
	setAttr ".ro" yes;
connectAttr "lambertN.oc" "lambertNSG.ss";
connectAttr "lambertNSG.pa" ":renderPartition.st" -na;
connectAttr "lambertN.msg" ":defaultShaderList1.s" -na;
connectAttr "<shape>.iog" "lambertNSG.dsm" -na;
```

Rules:

- A shape is linked to its assigned group or to `:initialShadingGroup`, never both. Unassigned materials are still
  written, because they are scene state.
- Material 0 (`standardSurface1`, the initial group's material) is locked and never written.
- `.c` is always written, even when the colour equals Maya's default. Channels are f32 values in 0..=1, formatted with
  Rust's shortest round-trip `Display`, as for `.vt` points.
- Authored shapes are never instanced (one root, one child per transform), so the unindexed `.iog` is enough.
- No `materialInfo` node and no `.ihi` are written (PROVISIONAL omission: neither page says they are required).
- `lambert1` and `standardSurface1` are reserved names. The writer refuses `NameCollision` when a material or group name
  equals any transform or shape name, since a later rename can collide with a name that was free at creation.

## PROVISIONAL (not stated by the docs; nothing compared with Maya, #5)

- The full statement set Maya itself writes for a lambert and its shading group. The minimum above is our choice.
- Placement of the `-na` links, their order, `setAttr ".ro" yes`, the `renderPartition` attribute `.st -na` and the
  `defaultShaderList1` connect.
- Whether Maya accepts, rewrites or drops any of them on load. If a later comparison disagrees, the reader stays exactly
  as wide as the writer, and fixing the writer is a new layout.

## Reopen rule

The file header stays `fileInfo "AztecCraft" "1"`; there is no layout marker, because the reader never interprets
`fileInfo` values.

- The reader counts the `"<shape>.iog" ":initialShadingGroup.dsm" -na` links it parses.
- The reopen gate (`decode_document`) re-emits the candidate and requires byte equality with the source. It re-emits the
  default links only if it parsed any (a private `DefaultLinks::{Omit, Emit}` parameter in the writer). Save always emits
  them.
- Byte equality therefore admits exactly two layouts: every shape linked, or none. Partial, duplicate and misordered
  links fail the comparison and are not Authored.
- A file saved by the current main has no links. It still reopens as Authored, and its next Save adds the links.
- Mesh-free scenes are unaffected.

## Order of landing

The reader arm lands before the writer: while the writer emits no links the gate mismatches and refuses, so the reader
arm is inert. The same holds for materials: the reader parses lambert, shadingEngine and the connects into a side table
the gate attaches, still refused by the writer's `SessionMaterials` check until the writer emits them.
