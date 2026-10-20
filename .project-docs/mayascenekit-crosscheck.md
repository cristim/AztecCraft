# MayaSceneKit cross-check (issue #156)

Maya's acceptance of files from the authored writer is unproven (see
[binary-authored-writer.md](binary-authored-writer.md)). MayaSceneKit
(https://github.com/yamahigashi/MayaSceneKit, MIT, pinned commit
`95a11db062689285604b3a647385dc9388402865`) is the one third-party `.mb` reader the project can run.
At that commit it refuses a mesh with an empty UV set, which is what the writer emits.

The ignored test `native_scene::binary::write::msk_crosscheck::msk_refuses_the_writers_empty_uv_mesh`
documents that refusal so any change becomes visible:

- It writes a cube with the writer and runs
  `<bin> to-ascii <in.mb> <out.ma> --issues-json <issues.json>` in the default mode.
- If the CLI succeeds, `issues.json` must hold at least one issue whose `trace_tag` or `trace_form`
  is `MESH` or `DMSH`. If it fails, its output must name `MESH` or `DMSH`.
- If neither holds (MayaSceneKit now decodes the mesh, or refuses for another reason) the test fails.
  Then revisit `known-issues.md` ("Mesh Save is PROVISIONAL") and the `save_scene_as` description.
- The test is a no-op when `AZTECCRAFT_MSK_BIN` is unset. CI never builds or runs MayaSceneKit.

The matcher was written from the CLI source at the pinned commit (`to_ascii.rs`,
`output_contracts.rs`) and exercised only against stand-in scripts that mimic its issues JSON. It has
not run against a real MayaSceneKit binary yet; if the first real run fails on the field names, fix
the matcher to the real report and record the observed issue text here.

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
