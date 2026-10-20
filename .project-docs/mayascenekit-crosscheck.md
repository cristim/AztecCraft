# MayaSceneKit cross-check (issue #156)

Maya's acceptance of files from the authored writer is unproven (see
[binary-authored-writer.md](binary-authored-writer.md)). MayaSceneKit
(https://github.com/yamahigashi/MayaSceneKit, MIT, pinned commit
`95a11db062689285604b3a647385dc9388402865`) is the one third-party `.mb` reader the project can run.
MayaSceneKit is a third-party reader, not Maya. At that commit it decodes the Maya-written, vendored
`tests/fixtures/gt-tools/dummy_mesh.mb` (no MESH issue; `decode_quality_distribution` has a DMSH/MESH
entry with quality `exact`) but reports the writer's cube as an unsupported MESH/DMSH. The writer emits
an empty UV set; that is the INFERRED cause, from the reader's mesh decoder source (it requires a
nonzero UV count) plus this contrast. No probe that adds UVs has been run.

The ignored test `native_scene::binary::write::msk_crosscheck::msk_decodes_maya_meshes_but_not_the_writers_mesh`
pins the contrast so any change on either side becomes visible:

- It first runs the same command on `dummy_mesh.mb` (the control) and requires no MESH issue and an
  exact DMSH/MESH decode entry.
- It writes a cube with the writer and runs
  `<bin> to-ascii <in.mb> <out.ma> --issues-json <issues.json> --mode forensic`. Forensic mode is
  required: the default mode exits 1 ("operation rejected by mode best_effort") and never writes
  the issues file. Forensic mode exits 0 and reports `state=unsupported`.
- The CLI must succeed and `issues.json` must hold an issue with `trace_tag` `MESH`, `trace_form`
  `DMSH` and `kind` `unsupported` (observed: node_type `mesh`, reason "no decoder registered for
  chunk tag", attr_name `<unknown-chunk>`).
- Otherwise (MayaSceneKit now decodes the mesh, the report format changed, a non-zero exit, or a
  missing or malformed JSON) the test fails. Then revisit `known-issues.md` ("Mesh Save is
  PROVISIONAL") and the `save_scene_as` description.
- The test is a no-op when `AZTECCRAFT_MSK_BIN` is unset. CI never builds or runs MayaSceneKit.

The matcher matches the real report observed from the pinned binary.

## Build and run (local only)

```sh
git clone https://github.com/yamahigashi/MayaSceneKit.git msk && cd msk
git checkout 95a11db062689285604b3a647385dc9388402865
cargo build --locked --release -p maya-scene-kit-cli   # the repo pins its own toolchain
cd /path/to/AztecCraft
AZTECCRAFT_MSK_BIN=/path/to/msk/target/release/maya-scene-kit \
  cargo test --lib msk_ -- --ignored
```

Do not commit the checkout, the binary or any output. Third-party code is built by a person on their
own machine, never in CI.
