# AztecCraft

AztecCraft is a Rust desktop 3D authoring project with the objective of reproducing Autodesk Maya 2026's interface and behavior, including modeling, animation, rigging, simulation, rendering, scripting and existing plug-in compatibility.

The current prototype has a native desktop window for cube/plane/sphere/cylinder/cone creation, selection, numeric transforms, undo/redo and perspective navigation. Open stages Maya binary scenes or legacy JSON documents into the authoritative native model. This candidate adds Unix and Windows Maya binary Save/Save As for supported existing-record edits through the ordinary vendored writer; current application checks and native Save QA remain pending. Overall Maya coverage is unmeasured.

[Quick start](#quick-start) · [Current capabilities](#current-capabilities) · [Build and test](#build-and-test) · [Roadmap](#roadmap-and-publication) · [Project documentation](.project-docs/INDEX.md)

## Quick start

1. Get a build: [download the CI artifact for your OS](#ci-builds), or [build from source](#build-and-test).
2. Unpack it and get past the unsigned-build prompt for your OS (steps under [CI builds](#ci-builds)).
3. Launch it. Use File, Open Scene to load `examples/extruded-cube.aztec` from a checkout, or create a cube from the shelf, select it and edit its channels. Undo is Ctrl/Cmd-Z; redo is Ctrl/Cmd-Y or Ctrl/Cmd-Shift-Z.

This is an early prototype. It does not reproduce Maya's feature set or behavior; the [capability table](#current-capabilities) lists what exists and what is pending.

## Current capabilities

| Area | This checkout | Evidence or next step |
| --- | --- | --- |
| Scene data | Cube/plane creation, validated quad meshes, stable IDs, rename, remove and transform edits | [Scene workflow tests](tests/scene_workflows.rs) and [mesh validation tests](tests/mesh_workflows.rs) |
| Transform math | Native f64 channels and pivots in declared units, with checked conversion to the derived viewport | [Native model](src/native_scene.rs) and view tests |
| Invalid edits | Reject non-finite or unsupported edits atomically; finite native values outside viewport representation remain authoring data | Native model and history tests |
| History | Undo/redo create, remove, rename, transform and single-face extrusion commands, with failed/no-op edits preserving history | [History workflow tests](tests/history_workflows.rs) |
| Native scene files | Bounded Maya binary decoding with retained source bytes; legacy JSON reads retained; provisional Unix and Windows existing-record binary writing; on Unix only, Save As of a new scene writes Maya Binary (Maya acceptance unverified) | [Document adapter](src/document.rs) and [binary codec](src/native_scene/binary.rs); document-layer repeat-save checks are separate from pending native Save QA |
| Camera math | Orbit/pan/dolly, perspective projection, rays, nearest-geometry picking and framing | [Viewport workflow tests](tests/viewport_workflows.rs) |
| Desktop shell and viewport | Create/select cube, plane, sphere, cylinder or cone, edit name/transform channels, undo/redo, delete and frame selected, with depth-tested opaque geometry | [Native shell](src/app/mod.rs) and [viewport UI](src/viewport_ui.rs) |
| Face selection | F11/Face menu and F8 Object/Face toggle (from Hierarchy it goes to Face); F9/F10, the Select > Components menu and the Status Line Vertex, Edge and Face buttons also enter Vertex and Edge modes, visible center markers, exact-face feedback and face framing | Shared typed operations and [Center selection policy](.project-docs/development.md#face-center-selection); native/Maya comparison remains separate |
| File actions in the window | New/Open and provisional eligible Save/Save As, unsaved-change Save/Don't Save/Cancel worded after Maya 2025.1's prompt (PROVISIONAL), source-change guards and uncertain-write reconciliation | [File integration tests](src/file_ui.rs); current integration and native Save verification pending |
| OBJ export | File > Export All / Export Selection and MCP `export_obj` write world-space polygon geometry of the native scene to a new `.obj` file | [OBJ workflow tests](tests/obj_workflows.rs), [native export tests](tests/obj_native_export.rs); export option boxes unsupported |
| OBJ import | File > Import... and MCP `import_obj` add the meshes of an `.obj` file to the scene as new objects, one per group, in one undo step; normals, UVs and materials are dropped, and a scene opened from `.mb` or `.ma` refuses it | [import tests](src/obj/import/tests.rs), [round trip with the exporter](tests/obj_import_roundtrip.rs); the import option box, Group and Single Object are unsupported |
| MCP | Optional stdio server controls the tools listed in the [generated catalogue](.project-docs/mcp-tools.md), including Move, Rotate and Scale, Save/Save As, checked material Diffuse data, viewport Grid visibility, Face, Vertex and Edge selection, in its newly launched native editor window | [Tool contract and limits](.project-docs/mcp.md); isolated application/SDK checks and bounded macOS material/Grid/Face QA remain separately scoped, with current-parent verification pending |
| Modeling | Core API extrudes one quad along its object-space normal, including signed/zero distances; native Extrude is limited to one picked face by Thickness (shelf button and Options window); manipulator and all-face extrusion pending | [Extrusion workflow tests](tests/extrusion_workflows.rs) |
| UVs, animation and rigging | Full workflows pending | [Implementation plan](.project-docs/plan.md) |
| Materials | Candidate Common Material Attributes/Diffuse control and MCP editing of an existing checked Phong scalar; shared users use one History value | Core, real-widget, SDK protocol reopen and bounded macOS edit/Undo/Save/reopen checks pass; committed cold and combined-parent publication checks pending. Material rendering, assignment, textures and color processing remain unavailable |
| Simulation and rendering | Pending | Reference scenes and output comparisons required |
| Maya formats, scripting and plug-ins | Pending | Existing scene/script/plug-in fixtures required |

Core workflow tests and UI event regression tests cover implemented behavior. The exact Maya reference installation hasn't been pinned, so they don't establish visual or functional parity. Unsupported modeling, animation, material and other controls are disabled in the prototype.

The native shelf and toolbox use colorful vector tool symbols with readable labels.
Unsupported transform tools remain disabled, and Select remains a mode indicator.

[The extruded cube example](examples/extruded-cube.aztec) is a synthetic AztecCraft
scene generated through two real extrusion commands. Open it with File → Open to
inspect the resulting L-shaped quad mesh. Tests reconstruct it, exercise
undo/redo, save/reload, reflected OBJ export and camera picking. It is not a Maya
reference scene; native Extrude covers one picked face by Thickness; the manipulator workflow is not implemented.

## Build and test

Install Rust through rustup (and GNU Make for the `make` targets). [rust-toolchain.toml](rust-toolchain.toml) pins Rust 1.99.0, rustfmt and Clippy, and `rust-version = "1.99"` in [Cargo.toml](Cargo.toml) is the minimum. With rustup, Cargo installs the pinned toolchain on first use in the checkout. On Linux install `pkg-config`, `libwayland-dev` and `libxkbcommon-dev` first (the packages CI installs). Run these commands from the repository root:

```sh
cargo run --locked
```

| Command | What it does |
| --- | --- |
| `cargo run --locked` | Build and launch the native desktop prototype |
| `make check` | Check formatting, lint, test and build with the default desktop feature |
| `make build` | Build the library and desktop binary with locked dependencies |
| `make test` | Run core workflow and desktop UI event tests |
| `make lint` | Check formatting and run Clippy with warnings treated as errors |
| `cargo test --locked --no-default-features` | Run the core workflow suites without the desktop target |

If the build fails with an error that a package requires a newer rustc, or with unstable-feature errors, your compiler is older than 1.99. A non-rustup Rust (a distribution package, for example) ignores `rust-toolchain.toml`: run `rustup update` or install Rust through rustup, then check `rustc --version`.

The `desktop` feature is enabled by default and uses eframe/egui with wgpu. The core suites can run without a display using `--no-default-features`. [Development instructions](.project-docs/development.md) describe system prerequisites, controls, scene conventions and optional local hooks.

For MCP, build with `cargo build --release --locked --features mcp` and configure the client to launch that binary with `--mcp-stdio`. It opens a new native editor window and requires a graphical session. Default build packages omit MCP. Run `cargo test --locked --features mcp-test-client` for the official SDK client suite. See [MCP setup, tools and shutdown behavior](.project-docs/mcp.md).

### CI builds

There is no release yet: publishing one needs the owner's authorization. Every push builds development packages in CI, and they are the only prebuilt binaries. Open Actions, the "Rust checks" run for a commit, then its Artifacts section. Artifacts are named `azteccraft-<platform>-<arch>-<commit>`, where the arch is `X64` or `ARM64` for the main jobs (for example `azteccraft-macos-ARM64-<commit>`) and the literal `x86_64` for the Intel macOS job, and expire after 7 days. GitHub wraps each artifact in a zip, so unzip it once to get the archive and `manifest.json` (SHA-256 of the archive and binary). Details and caveats in [CI build downloads](.project-docs/development.md#download-ci-builds).

| OS | Archive inside the artifact | Unpack and run |
| --- | --- | --- |
| macOS (Apple silicon) | `azteccraft-<version>-macos-aarch64.zip` | Unzip, then open `AztecCraft.app` |
| macOS (Intel) | `azteccraft-<version>-macos-x86_64.zip` | Same. Cross-compiled in CI and never launched there |
| Windows (x86_64) | `azteccraft-<version>-windows-x86_64.zip` | Unzip, then run `azteccraft\azteccraft.exe` |
| Linux (x86_64) | `azteccraft-<version>-linux-x86_64.tar.gz` | `tar xzf` the archive, then run `azteccraft/azteccraft` |

`<version>` is the `Cargo.toml` package version (currently 0.1.0). Each archive is portable: there is no installer, and it runs from wherever you unpack it. It holds the executable and a `licenses/` directory (under `Contents/Resources/licenses` inside the app bundle on macOS). The application has no settings or configuration persistence (eframe's persistence feature is not enabled); it only writes files you save or export. The builds are not notarized, so the first launch needs one extra step:

- macOS: try to open the app once, then choose System Settings, Privacy & Security, Open Anyway (shown for about an hour; [Apple's instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac)), or run `xattr -dr com.apple.quarantine AztecCraft.app`. Right-click, Open no longer works on macOS 15 and later.
- Windows: in SmartScreen choose More info, then Run anyway.
- Linux: run `chmod +x azteccraft/azteccraft` if the executable bit was lost. A graphical session and a graphics driver that wgpu supports are required (CI renders with software Vulkan).

The minimum supported OS versions are not established. CI builds on macOS 15, Windows Server 2025 and Ubuntu 24.04, and the packaged-binary smoke check only runs the usage exit (no window). Graphical launch of the packages on each OS is unverified (Windows and Linux are tracked in [#8](https://github.com/cristim/AztecCraft/issues/8)). Default-feature builds omit MCP.

## Architecture and platforms

[src/native_scene.rs](src/native_scene.rs) owns native authoring state and retained source data. [src/history.rs](src/history.rs) applies commands to that model, while [src/document.rs](src/document.rs) stages file candidates before replacement. [src/view_scene.rs](src/view_scene.rs) derives bounded geometry for camera math and the wgpu geometry viewport. The legacy JSON and OBJ library utilities remain separate from native document writing.

History is in-memory and unbounded. Imported edits require proved ownership and refuse unsupported source changes. Provisional Unix and Windows binary Save preserves supported existing records and retains uncertain installations for reconciliation; it does not support Maya ASCII or structural rewriting. Native Save QA remains open. The opaque GPU geometry preview uses depth testing and hardware clipping; materials, transparency, normal shading, textures and document color management remain unsupported. It does not establish Maya rendering or navigation parity.

Cross-platform desktop support and packaged launch behavior still need native evidence for macOS, Windows and Linux. The [CI matrix](.project-docs/builds.md) defines checks for those platforms, and a configured job doesn't establish a passing build. The eframe/egui UI remains provisional, with docking, focus, marking menus and Maya-style editing still to be compared against the reference.

## Roadmap and publication

Development proceeds through small, independently reviewed local commits:

1. Add the OBJ export and import option boxes, import into opened Maya files, and expand interoperability tests.
2. Verify the complete create/edit/undo/save/reopen/export journey through native dialogs, then establish platform packages and launch checks.
3. Build reviewed modeling, UV, animation and rigging slices, followed by materials, simulation, rendering, interchange, scripting and extensions.
4. Compare the complete audited workflow inventory with the pinned Maya build, including existing plug-ins and all major domains.

**The user has authorized publishing this early prototype and merging it to `main`.** That instruction supersedes the original requirement to wait for 80–90% of all Maya 2026 functionality before the first push. The 80–90% full-functionality target and identical-interface-and-behavior objective remain unmet acceptance requirements, including simulation, rendering, scripting and existing plug-in compatibility.

The [100-workflow research seed](.project-docs/inventory-seed.md) is provisional and doesn't supply the denominator. Its measurement proposal uses fixed domain budgets, zero credit for untested/partial workflows and mandatory compatibility evidence, so adding trivial rows can't inflate coverage. Exact final parity is a separate acceptance requirement.

Native verification must cover startup, creation/selection, numeric edits, undo/redo, navigation, file operations, error handling and visibly unsupported controls. [The macOS verification record](.project-docs/native-verification.md) covers scene editing and file integration using OCU-supplied native panel choices. Full interactive panel navigation, other platforms and the native-dialog GUI Export journey remain pending. Reference screenshots/outputs are needed before any fidelity claim. See [known limitations](known-issues.md).

## References and development

The [reference dossier](.project-docs/reference.md) links Autodesk's documentation and the Craft projects researched for product and implementation ideas. AztecCraft is an independent implementation, and those sources don't establish Maya compatibility or grant a license for this project.

See [the build specification](.project-docs/build-prompt.md), [the detailed plan](.project-docs/plan.md) and [contribution guidance](CONTRIBUTING.md) before making changes. Reference assets, private scene files and personal paths stay out of commits unless explicitly approved for distribution.

## License

AztecCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 ArtCraft Team and the AztecCraft contributors. Required notices are in [NOTICE](NOTICE).
