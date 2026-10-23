# Inserting an absent default translate leaf into an opened Maya Binary (issue #19)

Status: PROVISIONAL, implemented for translate (issue #19). Maya omits default transform leaves (translate 0, rotate 0, scale 1)
from `.mb` files, so an opened scene's transform often has no `t` chunk to edit. This note defines the
only situations in which AztecCraft inserts a missing `t` `DBL3` chunk when saving an opened `.mb`, and
where each rule comes from. **No AztecCraft-written file containing an inserted record has been opened
in Maya; acceptance of inserted records is unproven (#5).** Only AztecCraft's own decoder reopening
the output is checked. No PR text may claim more. Rotate and scale are not in this slice and keep their refusal.

## What the official documentation pins

Semantics only; the pages say nothing about file contents.

- Transform node: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/transform.html.
  `translate`, `rotate` and `scale` are `double3`; their children default to 0.0 cm, 0.0 deg and 1.0.
  The compound rows are flagged "storable".
- `MFnTransform`: https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/class_m_fn_transform.html.
  Translation is held in centimetres, rotation in radians.
- `setAttr -type double3`: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/setAttr.html.
  Three doubles; nothing on omitting defaults.
- Supported file formats (2014): https://download.autodesk.com/global/docs/maya2014/en_US/files/File_management_Supported_file_formats.htm.
  Binary files use 64-bit indices; no internal structure.

Autodesk publishes no `.mb` specification. The third-party description
(https://mayatools.readthedocs.io/en/latest/binary.html) covers tag, size and padding only and does not
document `CREA`, `DBL3`, `DBLE`, the 32-bit header word or record order. Everything below beyond the
semantic defaults is therefore chosen conservatively: copy what Maya itself wrote in the same file, or
refuse.

## Structural facts from a private corpus (aggregate only)

Five Maya-written files, 65 `XFRM` groups; the files are not distributed. Every group is `FOR8` and a
direct child of the root, with child alignment 8 and no padding.

- 27 of 65 transforms hold only `CREA`; these groups can receive an absent translate leaf when
  the file-wide aux/padding checks and the rest of the grammar below pass. Others hold `CREA`, an
  optional `DBLE` `v`, then `DBL3` records.
- `DBL3` key sets seen: (t,r), (t), (t,r,s), (r,s), (s,rp,sp), (s). Order is always t < r < s < rp < sp
  and `DBLE` `v` precedes every `DBL3`.
- Every `t`/`r`/`s` `DBL3` has a 27-byte payload (key, NUL, kind `0x20`, three big-endian f64) and 5 zero
  padding bytes, 48 bytes in all. `rp`/`sp` have 28 plus 4.
- The header aux word of `DBL3`/`DBLE`/`FLGS` is constant inside a file and differs between files. Its
  meaning is unknown, so the rule copies the file's own value.

## Grammar (PROVISIONAL)

A site for (owner transform, translation) exists only if all hold. Otherwise there is no site, decoding
still succeeds and the edit is refused as before (`UnsupportedEdit` / `AbsentChannelLeaf`).

1. The group is `FOR8` `XFRM`; every ancestor is a group up to the root (the container bounds depth).
2. The walk is over the raw container children, never over decoded records, because the decoder drops
   unrecognised tags. Children are exactly `CREA` first, at most one `DBLE` with key `v` and kind `0x24`
   next, then zero or more `DBL3`. Any other tag, nested group, second `DBLE` or other `DBLE` key
   means no site.
3. Every `DBL3` key is one of t, r, s, rp, sp (anything else, such as a shear or pivot-translate key the
   decoder ignores, means no site). Keys strictly ascend t < r < s < rp < sp, `t` is absent and the
   first key, if any, is `r` or `s`.
4. Every `DBL3` chunk in the whole file, in any group, has the same aux value A, and at least one
   exists; the group's `DBLE` also has A. Any disagreement, or no `DBL3` in the file, means no site.
   The value is never invented.
5. Every `DBL3` chunk in the whole file is canonical: payload `key NUL 0x20` plus 24 bytes (27 for
   single-letter keys, 28 for `rp`/`sp`) and all-zero padding to alignment 8. One exception anywhere
   means no site in the file, because the file then does not look like Maya's own output.
6. The record is inserted at the header start of the first existing `DBL3`, else at the end of the
   group's payload.
7. The record is `DBL3`, aux A, size 27, `t` NUL `0x20`, three big-endian f64 in internal centimetres,
   5 zero bytes: 48 bytes, a multiple of 8, so no ancestor needs re-padding. The size word of every
   ancestor grows by 48; parent padding must be unchanged or the edit is refused.
8. If all three final words are bit-equal to +0.0 nothing is inserted. -0.0 is a real edit.
9. The output must stay within the 16 MiB container limit (`ResourceLimit`).

Fixtures written by the test support builders use a different aux value and non-zero padding, so they
have no site, which keeps the existing refusal tests valid.

## Not established

The aux word's meaning, the record order Maya expects beyond what the corpus shows, and whether Maya
reads an inserted record are unknown. The ignored private differential test
(`native_scene::binary::encode::tests::private_differential`) strips one `t` record from a real
file, reinserts it and requires byte identity. It checks the mechanism against Maya's own writer in
the observed contexts; it does not show that Maya accepts an AztecCraft-written file containing an
inserted record.

Save As from an opened scene derives its bytes from the bytes parsed at Open, so an inserted record
is written from the same source the edit was checked against.
