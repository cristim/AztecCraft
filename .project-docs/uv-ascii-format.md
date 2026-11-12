# UV storage contract for authored Maya ASCII (issue #156)

Status: task 1 only. Nothing writes or reads UVs in a file yet; writers still refuse session UVs.

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
- Text form of an f32 value (planned: the writer's existing shortest round-trip f32 formatting).
