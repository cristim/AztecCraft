# MayaSceneKit fixture provenance

One public Maya Binary file, vendored as a test fixture only. It is read by unit tests
(`include_bytes!`) and is not part of the binary, the packaged builds or the library API.

| Field | Value |
| --- | --- |
| Source | https://github.com/yamahigashi/MayaSceneKit |
| Pinned commit | `95a11db062689285604b3a647385dc9388402865` |
| Path at that commit | `tests/fixtures/mb/owner_delete/file_owner_delete.mb` |
| Raw URL | https://raw.githubusercontent.com/yamahigashi/MayaSceneKit/95a11db062689285604b3a647385dc9388402865/tests/fixtures/mb/owner_delete/file_owner_delete.mb |
| Author | Takayoshi Matsumoto |
| License | MIT, "Copyright (c) 2026 Takayoshi Matsumoto"; the complete text is `LICENSE` in this directory, fetched from the same pinned commit |
| Size | 8,096 bytes |
| SHA-256 | `b193a81d5a8456bf74b1ff16874d94062d178462eb8cf9406100c787265cdfd5` |
| `LICENSE` SHA-256 | `8c850ccb7bd4c61d0725b718e65b0d068a29507d9d3d410e883186e4b82c04d9` |
| Fetched | 2026-10-20 |

The file is unmodified. Its FINF chunks name product "Maya 2026" and cut identifier
`202507081222-4d6919b75c`, so it was written by Maya; its leading HEAD holds VERS `2026`.

## Why it is here

Autodesk publishes no Maya Binary specification. The authored writer needs the aux word
that Maya itself stores on `DBL3` leaves. `src/native_scene/binary/write.rs` pins its
`DBL3_AUX` constant, and the framing constants it shares with this file, to the bytes of
this fixture with a test, so the value cannot drift from its cited public source.

Aux words observed in this file, by chunk kind (this is why one constant is not shared
across kinds). Groups: the root `Maya`, the leading `HEAD`, `XFRM` and `DCAM` carry `0`;
the trailing `HEAD` carries `0xb8010000`. Leaves: `CREA` `0`; `DBL3` `0xb8010000` (7 of
7); `DBLE` `0xb8010000` (25) and `0x24000000` (12); the HEAD children `VERS`, `UVER`,
`MADE`, `ICON`, `INFO` and `OBJN` `0`, and `LUNI`, `AUNI`, `TUNI`, `TDUR` and `CHNG`
`0x24000000`. The file has no `MESH` chunk.

## Notice

The MIT license requires its notice in copies of the fixture. `LICENSE` here carries it,
and the root `NOTICE` lists the fixture. No endorsement by the upstream author or by
Autodesk is implied. The file contains no private scene content.
