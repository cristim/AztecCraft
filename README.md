# AztecCraft

AztecCraft is a Rust desktop 3D authoring project with the objective of reproducing Autodesk Maya 2026's interface and behavior, including modeling, animation, rigging, simulation, rendering, scripting and existing plug-in compatibility.

The current prototype has a native desktop window for cube/plane/sphere/cylinder/cone creation, selection, numeric transforms, undo/redo and perspective navigation. Open stages Maya binary scenes or legacy JSON documents into the authoritative native model. This candidate adds Unix Maya binary Save/Save As for supported existing-record edits through the ordinary vendored writer; current application checks and native Save QA remain pending. Overall Maya coverage is unmeasured.

[Current capabilities](#current-capabilities) · [Build and test](#build-and-test) · [Roadmap](#roadmap-and-publication) · [Project documentation](.project-docs/INDEX.md)

## Current capabilities

| Area | This checkout | Evidence or next step |
| --- | --- | --- |
| Scene data | Cube/plane creation, validated quad meshes, stable IDs, rename, remove and transform edits | [Scene workflow tests](tests/scene_workflows.rs) and [mesh validation tests](tests/mesh_workflows.rs) |
| Transform math | Native f64 channels and pivots in declared units, with checked conversion to the derived viewport | [Native model](src/native_scene.rs) and view tests |
| Invalid edits | Reject non-finite or unsupported edits atomically; finite native values outside viewport representation remain authoring data | Native model and history tests |
| History | Undo/redo create, remove, rename, transform and single-face extrusion commands, with failed/no-op edits preserving history | [History workflow tests](tests/history_workflows.rs) |
| Native scene files | Bounded Maya binary decoding with retained source bytes; legacy JSON reads retained; provisional Unix existing-record binary writing | [Document adapter](src/document.rs) and [binary codec](src/native_scene/binary.rs); document-layer repeat-save checks are separate from pending native Save QA |
| Camera math | Orbit/pan/dolly, perspective projection, rays, nearest-geometry picking and framing | [Viewport workflow tests](tests/viewport_workflows.rs) |
| Desktop shell and viewport | Create/select cube, plane, sphere, cylinder or cone, edit name/transform channels, undo/redo, delete and frame selected, with depth-tested opaque geometry | [Native shell](src/app/mod.rs) and [viewport UI](src/viewport_ui.rs) |
| Face selection | F11/Face menu and F8 Object/Face toggle (from Hierarchy it goes to Face); F9/F10, the Select > Components menu and the Status Line Vertex, Edge and Face buttons also enter Vertex and Edge modes, visible center markers, exact-face feedback and face framing | Shared typed operations and [Center selection policy](.project-docs/development.md#face-center-selection); native/Maya comparison remains separate |
| File actions in the window | New/Open and provisional eligible Save/Save As, unsaved-change Save/Discard/Cancel, source-change guards and uncertain-write reconciliation | [File integration tests](src/file_ui.rs); current integration and native Save verification pending |
| OBJ export | File > Export All / Export Selection and MCP `export_obj` write world-space polygon geometry of the native scene to a new `.obj` file | [OBJ workflow tests](tests/obj_workflows.rs), [native export tests](tests/obj_native_export.rs); export option boxes and OBJ import unsupported |
| MCP | Optional stdio server controls the tools listed in the [generated catalogue](.project-docs/mcp-tools.md), including Move, Rotate and Scale, Save/Save As, checked material Diffuse data, viewport Grid visibility, Face, Vertex and Edge selection, in its newly launched native editor window | [Tool contract and limits](.project-docs/mcp.md); isolated application/SDK checks and bounded macOS material/Grid/Face QA remain separately scoped, with current-parent verification pending |
| Modeling | Core API extrudes one quad along its object-space normal, including signed/zero distances; native Extrude and all-face extrusion pending | [Extrusion workflow tests](tests/extrusion_workflows.rs) |
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
reference scene; native Extrude remains unsupported until a real history and
manipulator workflow is implemented.

## Build and test

Install Rust through rustup and GNU Make. [rust-toolchain.toml](rust-toolchain.toml) pins Rust 1.99.0, rustfmt and Clippy. Run these commands from the repository root:

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

The `desktop` feature is enabled by default and uses eframe/egui with wgpu. The core suites can run without a display using `--no-default-features`. [Development instructions](.project-docs/development.md) describe system prerequisites, controls, scene conventions and optional local hooks.

For MCP, build with `cargo build --release --locked --features mcp` and configure the client to launch that binary with `--mcp-stdio`. It opens a new native editor window and requires a graphical session. Default build packages omit MCP. Run `cargo test --locked --features mcp-test-client` for the official SDK client suite. See [MCP setup, tools and shutdown behavior](.project-docs/mcp.md).

### CI builds

Every push builds test binaries for macOS (aarch64, x86_64), Linux (x86_64) and Windows (x86_64). Open Actions, the run for a commit, then Artifacts. They are not on the Releases page, and each download is a zip containing the archive. The builds are not notarized: on macOS unzip, try to open the app once, then choose System Settings, Privacy & Security, Open Anyway (shown for about an hour; [Apple's instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac)), or run `xattr -dr com.apple.quarantine AztecCraft.app` in a terminal; on Windows choose More info, Run anyway in SmartScreen; on Linux `chmod +x azteccraft` if needed. Details in [CI build downloads](.project-docs/development.md#download-ci-builds).

## Architecture and platforms

[src/native_scene.rs](src/native_scene.rs) owns native authoring state and retained source data. [src/history.rs](src/history.rs) applies commands to that model, while [src/document.rs](src/document.rs) stages file candidates before replacement. [src/view_scene.rs](src/view_scene.rs) derives bounded geometry for camera math and the wgpu geometry viewport. The legacy JSON and OBJ library utilities remain separate from native document writing.

History is in-memory and unbounded. Imported edits require proved ownership and refuse unsupported source changes. Provisional Unix binary Save preserves supported existing records and retains uncertain installations for reconciliation; it does not support Maya ASCII, structural rewriting or Windows writing. Native Save QA remains open. The opaque GPU geometry preview uses depth testing and hardware clipping; materials, transparency, normal shading, textures and document color management remain unsupported. It does not establish Maya rendering or navigation parity.

Cross-platform desktop support and packaged launch behavior still need native evidence for macOS, Windows and Linux. The [CI matrix](.project-docs/builds.md) defines checks for those platforms, and a configured job doesn't establish a passing build. The eframe/egui UI remains provisional, with docking, focus, marking menus and Maya-style editing still to be compared against the reference.

## Roadmap and publication

Development proceeds through small, independently reviewed local commits:

1. Add OBJ import and the export option boxes, and expand interoperability tests.
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
