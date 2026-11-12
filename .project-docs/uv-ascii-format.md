# UV storage contract for authored Maya ASCII (issue #156)

Status: tasks 1, 2 and 3a. A crate-private ASCII emit function writes the block below (tests only); the reader
parses it into a per-shape side table (`decode_with_uvs`) that nothing attaches yet. Save, the reopen gate and the binary
writer still refuse or ignore session UVs.

## Documented (Maya 2027 pin, reference.md)

The `setAttr` command page (CommandsPython/setAttr.html) lists the polyFaces value codes: `f n e...`
face edges (negative means reversed), `h n e...` hole edges, `mf`/`mh` obsolete since 3.0, replaced by
`mu set count uv...` (zero-based UV set index, number of vertices with valid UVs, then UV ids), one
`mu` per UV set, and `mc` for colour. Summarised, not copied.

## Model contract (this repository)

- One UV set, `map1`, with one (u, v) per outer corner of each face, in face order. No sharing, no seams,
  no holes (typed `UnsupportedUvTopology`).
- Corner ids are the running corner index `0..corner_count` in face order; the UV array length equals
  `UvSet::corner_count()`.
- Every stored value is exactly an f32 (`float2` is single precision). `UvSet::from_corners` and the planar
  projection round to the nearest f32; `NativeScene::validate` rejects any value that is not f32-exact, not
  finite, or overflows f32. Corner UVs closer than one f32 step collapse within a session; accepted.
  Planar UVs are in 0..=1, so rounding keeps exact 0 and 1.

## PROVISIONAL (not stated by the docs; nothing compared with Maya, #5)

- `setAttr ".uvst[0].uvsn" -type "string" "map1";`, `setAttr -s N ".uvst[0].uvsp[0:N-1]" -type "float2" u v ...;`
  and `setAttr ".cuvs" -type "string" "map1";`. The attribute names are corroborated by the binary reader
  (`src/native_scene/binary/appearance.rs` parses `uvst[0].uvsp`, `uvsn` and `cuvs` from Maya `.mb`); the ASCII
  text layout, `mu` placement inside an `f` row and whether Maya writes `uvst` before `.fc` are not.
- Text form of an f32 value: Rust's shortest round-trip `Display` of the f32, as for `.vt` points.
- Emitted layout per shape that has a set (shapes without one emit exactly the previous bytes), after the
  `.fc` rows, PROVISIONAL in every detail:
  - each `.fc` row gains ` mu 0 <corners> <id>...` (running corner ids, one `mu` for set 0), e.g. `f 4 0 1 2 3 mu 0 4 0 1 2 3`;
  - then `.uvst[0].uvsn` ("map1"), `.uvst[0].uvsp[0:N-1]` as a `float2` array written one `u v` row per line,
    terminated by `;`, then `.cuvs` ("map1"). Maya's own placement (it may put `uvst` before `.vt`) is unchecked.
- Writer admission: the extra output is charged to `resource_admission` (`mesh_uv_work`) and counted exactly by
  `prefix_bytes`; ids and counts must fit signed 32 bits.
