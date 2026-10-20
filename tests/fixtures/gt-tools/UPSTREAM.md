# gt-tools fixture provenance

One public Maya Binary file, vendored as a test fixture only. It is read by unit tests
(`include_bytes!`) and is not part of the binary, the packaged builds or the library API.

| Field | Value |
| --- | --- |
| Source | https://github.com/TrevisanGMW/gt-tools |
| Pinned commit | `05837d6621b2256138c1fdf7e7a4546945dd935d` |
| Path at that commit | `gt/tools/auto_rigger/templates/package_resources/Biped_Base/geo/dummy_mesh.mb` |
| Raw URL | https://raw.githubusercontent.com/TrevisanGMW/gt-tools/05837d6621b2256138c1fdf7e7a4546945dd935d/gt/tools/auto_rigger/templates/package_resources/Biped_Base/geo/dummy_mesh.mb |
| Author | Guilherme Trevisan |
| License | MIT, "Copyright (c) 2020 Guilherme Trevisan"; the complete text is `LICENSE` in this directory, fetched from https://raw.githubusercontent.com/TrevisanGMW/gt-tools/05837d6621b2256138c1fdf7e7a4546945dd935d/LICENSE |
| Size | 553,616 bytes |
| SHA-256 | `9eb842b4568ec5324b52f50dfb1ce792133946269d1e9d85d405dc645cd229c3` |
| `LICENSE` SHA-256 | `a9dc8021d8a699d305265dcfae38a8d1f61010390b43257a96657237a217e84b` |
| Fetched | 2026-10-20 |

The file is unmodified. Its FINF chunks name product "Maya 2025" and cut identifier
`202409190603-cbdc5a7e54`, so it was written by Maya 2025; its leading HEAD holds VERS `2025ff03`.

## Why it is here

Autodesk publishes no Maya Binary specification. The authored Maya Binary writer takes its
whole profile, the `VERS` string and the aux word of every chunk kind it emits, from this one
file. Pinned by tests: root and group headers, HEAD children, `CREA`, `DBL3`, the plain `DMSH`
group and its `CREA`, and the `MESH` aux (`0xa8000000` at 0x11324), so no two Maya releases
are mixed. `src/native_scene/binary/write/profile.rs` names each value and its tests read this
file's bytes to pin it, so a constant cannot drift from its cited source. Offsets and values
are in `.project-docs/binary-authored-writer.md`.

## Notice

The MIT license requires its notice in copies of the fixture. `LICENSE` here carries it, and
the root `NOTICE` lists the fixture. No endorsement by the upstream author or by Autodesk is
implied. The file contains no private scene content.
