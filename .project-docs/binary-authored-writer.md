# Authored Maya Binary writer (issue #156)

Status: in progress. The pure writer in `src/native_scene/binary/write.rs` builds a whole file for a
scene of bare transforms or of transforms over one mesh each: framing builders, the profile constants,
the `HEAD` group, `XFRM` and `DMSH` groups, and a self-check that decodes the bytes before they are
returned. `encode_authored` has no Editor caller yet. This note records what the writer may
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

Observed grammar therefore comes from these sources:

- **Writer profile: one public Maya 2025 file.** `tests/fixtures/gt-tools/dummy_mesh.mb`
  (gt-tools, MIT, pinned commit, sha256 and license in its `UPSTREAM.md`). The `VERS` string and the
  aux word of every chunk kind the writer emits come from this file alone, so no two Maya releases
  are mixed. `write/profile.rs` names each value and `write/profile/tests.rs` reads the file's bytes to
  pin it. Offsets: `VERS` 0x28 (`2025ff03`); root `Maya` aux `0x47020000` at 0; `HEAD` group aux 0 at
  0x14; `HEAD` children aux 0 for `VERS` `UVER` `MADE` `ICON` `INFO` `OBJN` `INCL` and `0xa8000000` for
  `LUNI` 0x100, `TUNI` 0x118, `AUNI` 0x130, `TDUR` 0x148; `XFRM` group aux 0 at 0x380, its `CREA` aux 0
  at 0x394, `DBL3` aux `0x4d020000` at 0x3dc; `CREA` flag `0x8c` on the one user node
  `Dummy_Mesh` at 0xca4, and `0x8d` on the four default cameras (0x394, 0x5bc, 0x814, 0xa4c); `MESH` aux `0xa8000000` at 0x11324.
- The public Maya 2026 file in `tests/fixtures/mayascenekit/` is kept for read tests only. It stores
  different values for the same kinds (root aux 0, `DBL3` aux `0xb8010000`), and the writer takes
  nothing from it.
- Five Maya-written files in a private local corpus that is never committed; only aggregate,
  anonymised counts from it appear here.

## Saved files are Maya 2025-format

The profile is Maya 2025's, so files this writer produces are Maya 2025-format, not 2026-format. That
Maya 2026 opens them is **PROVISIONAL**: Autodesk's pages on opening scenes saved in an earlier
release (https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/Opening-a-Legacy-scene-file-in-a-newer-version-of-Maya.html,
https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm,
found by search 2026-10-20; the first returned HTTP 403 to the fetch tool and neither page was read in
full) describe opening older scenes in general, and none speaks about files from a third-party writer.
Nothing here has been opened in Maya (issue #5).

## Classification of every emitted element

D = documented by Autodesk, O = observed in Maya-written files or enforced by the decoder, O1 = observed
once, I = invented (not observed, or a value chosen). Every I item is PROVISIONAL and is named in PR
bodies and `known-issues.md`.

| Element | Choice | Class |
| --- | --- | --- |
| Root `FOR8 Maya`, child alignment 4, exact sizes, zero padding | container grammar | O |
| Root, `HEAD` group, `XFRM` group, `CREA` and `DBL3` aux | one named constant per kind, from the Maya 2025 file (see above). The root aux `0x47020000` is not 0 there, unlike the 2026 file | O1, pinned by tests |
| Leading `HEAD` order and constants | `VERS` `UVER` `MADE` `ICON` `INFO` `OBJN` `INCL` `LUNI` `TUNI` `AUNI` `TDUR` with that file's payloads and aux (`INCL` payload is the six bytes `undef(`, as stored). Byte-identical to the file's chunks, tested | O1 |
| `HEAD` chunks omitted | `CHNG` (wall-clock timestamp, irreproducible), `FINF`, `PLUG`, `BCSZ` and the trailing `HEAD` (writing application and plug-in provenance, which would be fabricated) | I |
| `MESH` aux | `0xa8000000` at 0x11324, the file's only `MESH`; pinned by a test that reads it | O1 |
| Shape `DMSH` group aux and `CREA` aux | 0 and 0. The file's only `MESH` sits in a skinned intermediate shape (group aux `0x4d020000` at 0xfd8, `CREA` aux `0x47020000` at 0xfec); its plain shape (0xe5c, `CREA` at 0xe70) has 0 and 0 but no `MESH`. A fresh mesh is a plain, user-created shape, so it takes the plain pair. PROVISIONAL: the file never shows a `MESH` in a plain shape | I |
| `MESH` key | `o`. The file's `MESH` key is `ci`, on an intermediate shape (`io` 1 at 0x1044), which would not describe a visible shape; `o` is what the repo's synthetic wire fixtures use and the private corpus stores (aggregate only). It is not in the Maya 2025 file. PROVISIONAL, one constant (`MESH_KEY`) | I |
| `VERS` | `2025ff03`, the exact string in the Maya 2025 file. Not tied to the ASCII header, which keeps its own `2026` text. The decoder never reads it | O1 |
| Internal units cm / radians through `Scalar::internal` | | D |
| Transform key set `t`, `r`, `s`, `DBL3` kind 0x20 | always all three | O |
| Transform `CREA` flag `0x8c` | the value the Maya 2025 file stores on its one user node `Dummy_Mesh`, while its four default cameras carry `0x8d`. The likely meaning is user-created (`0x8c`) versus default/startup (`0x8d`); that is an inference from one file, not "owns a mesh" | O1, pinned by test; the meaning is INFERRED |
| `XFRM` content | `CREA` then `t`, `r`, `s` `DBL3` always; Maya 2025 omits default channels and adds `DBLE v`, `ATTR`, `FLGS`, so this group is shorter than any Maya wrote | I |
| Shape `CREA` flag `0x04` (0xe70, 0xfec), float kind `0x20`, corner flag set (reverse `0x80`, terminal `0x60`), 20-byte zero trailer, UV header 0,1,0 and set name `map1` | the fixture's `MESH` stores the same UV header, set name and trailer; pinned by a test | O |
| Normals count 0 | `n` is user-specified normals only | D |
| `DMSH` with only `CREA` + `MESH` (no `FLGS`, `vir`, `vif`, `STR`, `FLT2`) | no Maya-written `DMSH` is this short (29 of 29 carry them); unknown whether Maya loads it or defaults the rest | I |
| Omitted trailing `HEAD`, `CHNG`, `FINF`, `PLUG`, default scene groups, `CONS` | conservative omission; no provenance is fabricated | I |
| Empty UV set `map1` (0 points, 0 ids) | no fabricated values; the decoder admits exactly `points == 0 && ids == 0` (a set with only one of them, or any other count mismatch, is still rejected) | I |
| Edge word0 bit31 | set on boundary and non-manifold edges (any edge not used by exactly two face loops), clear on interior edges, so a closed mesh is all clear. From the Task 1.0 counts (99.6% of boundary, 100% of non-manifold edges set). PROVISIONAL: 37.3% of interior edges are also set in Maya's files, so it is not a full rule | I |
| UUID bytes: stored v4, Microsoft GUID byte order (`Uuid::to_bytes_le`) | observed in the Maya 2025 fixture: all seven `XFRM` and `DMSH` `CREA` identities (e.g. `persp` at file offset 0x3ab, the `Dummy_Mesh` shape at 0x101a) carry the v4 nibble in identity byte 7 and the variant bits in byte 8, which is the mixed-endian layout; a test pins it | O (layout), I (values) |
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
face holes and unsupported channels. The Binary writer adds `MaterialAssigned` (some shape assigned a non-initial shading group, tested on the
assignment values, so a created but unassigned lambert or a shape reassigned to the initial group is
accepted; the ASCII emitter ignores materials and is unchanged), `MissingUuid` (later commit), edge count over 2^24, counts
over u32, output over 16 MiB and resource overruns.

## Self-check and tests

The writer decodes its own output with `binary::decode` and compares names, DAG pairing, units, `t/r/s`
internal words, points, edge endpoints and face loops before returning bytes. UUIDs, `NativeId`s and edge
smoothness differ by design. A raw-byte test checks the 16 `CREA` identity bytes, which the decoder skips.
An ignored differential test rebuilds chunks from a private corpus given by environment variable and
compares them to Maya's bytes; it never commits paths or content.

The self-check currently compares units, node count and order, names, DAG pairing and the t/r/s/pivot
bit patterns, and for each mesh shape its name, point f32 bit patterns, edge endpoint pairs and face
loops of directed edges (no holes or point offsets).

## Reopen trigger

Revisit the profile and the I rows if Maya rejects, repairs or rewrites a file written with these
choices, or if a public, licensed Maya 2026 file with a mesh becomes available (`known-issues.md`); issue
#5 is the standing request for a Maya 2026 read-back.
