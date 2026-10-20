# Authored Maya Binary writer (issue #156)

Status: in progress. The pure writer in `src/native_scene/binary/write.rs` has framing builders and
pinned constants only; it has no caller and writes nothing yet. This note records what the writer may
emit, where each choice comes from, and what is invented. **Maya acceptance of any file this writer
produces is unproven.** AztecCraft's own decoder reopening a file is the only check, and no PR text may
claim more.

## Evidence and its limits

Autodesk publishes no Maya Binary specification. The pages consulted (2026-10-20) describe node
attributes and commands, never the container, and say nothing about the aux word, `CREA`, `DBL3`, `DBLE`,
`MESH` or `HEAD`:

- Mesh node: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/mesh.html. `vrts` (vt),
  `edge` (ed) and `face` (fc) are the file I/O arrays; `normals` (n) is "special attribute for file i/o
  of user specified normals", so an empty block means none specified. `visibleInReflections` and
  `visibleInRefractions` are not on the page, so their defaults are undocumented.
- `file` command: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/file.html.
  `mayaBinary` and `mayaAscii` are both ordinary save types.
- Save Scene As: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-049E009D-34FE-4767-B9B2-047FFB3142E8.htm.
  Says nothing about what plain Save does for an untitled scene (PROVISIONAL for the native UI).
- Units and `MFnTransform` (internal centimetres and radians): see [reference.md](reference.md).

Observed grammar therefore comes from two sources. First, a public MIT-licensed Maya 2026 file,
vendored with its license and provenance in `tests/fixtures/mayascenekit/` (read by tests). Second,
five Maya-written files in a private local corpus that is never committed; only aggregate, anonymised
counts from it appear here.

## Classification of every emitted element

D = documented by Autodesk, O = observed in Maya-written files or enforced by the decoder, O1 = observed
once, I = invented (not observed, or a value chosen). Every I item is PROVISIONAL and is named in PR
bodies and `known-issues.md`.

| Element | Choice | Class |
| --- | --- | --- |
| Root `FOR8 Maya`, child alignment 4, exact sizes, zero padding | container grammar | O |
| Group header aux | 0 for the root and `XFRM` (public fixture and every private file); `DMSH` 0 in the private corpus only | O |
| `DBL3` leaf aux | `0xb8010000`, the value on all 7 `DBL3` chunks of the public fixture; for `DBL3` only | O1 |
| Leading `HEAD` order and constants | as observed (`VERS`, `UVER`, `MADE`, `ICON`, `INFO`, `OBJN`, `INCL`, `LUNI` cm, `TUNI` film, `AUNI` deg, `TDUR`) | O |
| `HEAD` child aux | per kind. Public fixture: `VERS`, `UVER`, `MADE`, `ICON`, `INFO`, `OBJN` and the leading `INCL` 0; `LUNI`, `AUNI`, `TUNI`, `TDUR` `0x24000000`. Taken per kind when `HEAD` lands | O1 |
| `MESH` aux, `CREA` aux | `CREA` 0 in the public fixture. The fixture has no `MESH`, so the `MESH` aux is **unresolved** and blocks mesh emission | open |
| `VERS` | "2026", `MAYA_VERSION` shared with the ASCII header and pinned by test. Decoder never reads it; only 2025 and 2026 observed; reference.md keeps 2026 as the acceptance target | O (choice is the project profile) |
| Internal units cm / radians through `Scalar::internal` | | D |
| Transform key set `t`, `r`, `s`, `DBL3` kind 0x20 | always all three | O |
| Transform `CREA` flag `0x8c` | root-level transform owning a mesh | O1, pinned by test |
| Shape `CREA` flag `0x04`, `MESH` key `o`, corner flag set, 20-byte zero trailer | as observed | O |
| Normals count 0 | `n` is user-specified normals only | D |
| `DMSH` with only `CREA` + `MESH` (no `FLGS`, `vir`, `vif`, `STR`, `FLT2`) | no Maya-written `DMSH` is this short (29 of 29 carry them); unknown whether Maya loads it or defaults the rest | I |
| Omitted trailing `HEAD`, `CHNG`, `FINF`, `PLUG`, default scene groups, `CONS` | conservative omission; no provenance is fabricated | I |
| Empty UV set `map1` (0 points, 0 ids), header 0,1,0 | no fabricated values; needs a narrow decoder relaxation (`points == 0 && ids == 0`) | I |
| Edge word0 bit31 clear | see "Task 1.0" below | I |
| UUID bytes: stored v4 in RFC byte order | Maya's byte order is unproved | I (order) |
| Transform-then-shape ordering for several nodes; shape name rule | one `XFRM` then its `DMSH`, node order as in the scene | I |
| New file mode | inherited from the existing staged write and umask, not observed | I |
| No shading connections; meshes unassigned | decision | decision |

`Retention`, `AuthoredRefusalCode` and the other save-integration types are code structures, not file
format inventions.

## Task 1.0: edge bit31 against topology (private corpus, aggregate)

Read-only structural correlation over 5 files, 29 meshes, 61,429 edges. Adjacent-face normals come from
the face loops; "sharp" means the adjacent face normals differ by more than 60 degrees.

| Edge class | Edges | bit31 set |
| --- | --- | --- |
| all | 61,429 | 49.8% |
| boundary (one face) | 12,070 | 99.6% |
| non-manifold (3 or more faces) | 249 | 100% |
| interior, any angle (two faces) | 49,110 | 37.3% |
| interior, coplanar | 2,882 | 26.9% |
| interior, sharp | 4,481 | 27.4% |

Per mesh: 1 mesh has no edge set (it is closed and manifold), 1 mesh has every edge set, 27 are mixed.
Sharp interior edges are no more likely to be set than coplanar ones, so bit31 does not track hardness
or an angle threshold. It tracks boundary and non-manifold edges strongly, and the rest is unexplained.
The mapping "bit31 clear = hard edge" from the ASCII `.ed` third value therefore has **no evidence**.
All-clear is observed on one closed mesh, so the writer may emit it for closed meshes, but it stays I
(PROVISIONAL) and the decoder's `smooth: None` is the only honest read-back.

## Refusals (typed, path-free, nothing dropped)

The shared eligibility (`src/native_scene/authored.rs`) refuses: a retained source, curves, units other
than cm/deg/film, a hierarchy that is not flat transform-over-one-mesh, invalid, duplicate or reserved
names (the default Maya node names: `RESERVED` plus the ASCII decoder's `BUILTINS`, for every writer, since
a file omitting the default scene still collides with them on load), empty meshes, non-zero pivots, extras,
restrictions, effects, non-authored visibility, double-precision points, point offsets, non-hard edges,
face holes and unsupported channels. The Binary writer adds (later commits) `MaterialAssigned` (some
shape assigned a non-initial shading group, tested on values), `MissingUuid`, edge count over 2^24, counts
over u32, output over 16 MiB and resource overruns.

## Self-check and tests

The writer decodes its own output with `binary::decode` and compares names, DAG pairing, units, `t/r/s`
internal words, points, edge endpoints and face loops before returning bytes. UUIDs, `NativeId`s and edge
smoothness differ by design. A raw-byte test checks the 16 `CREA` identity bytes, which the decoder skips.
An ignored differential test rebuilds chunks from a private corpus given by environment variable and
compares them to Maya's bytes; it never commits paths or content.

## Reopen trigger

If Maya rejects, repairs or rewrites a file written with these choices, revisit the I rows; issue #5 is the
standing request for a Maya 2026 read-back.
