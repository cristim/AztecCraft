# AztecCraft

AztecCraft is a Rust desktop 3D authoring project with the objective of reproducing Autodesk Maya 2026's interface and behavior, including modeling, animation, rigging, simulation, rendering, scripting and existing plug-in compatibility.

The current prototype has a native desktop window for cube/plane creation, selection, numeric transforms, undo/redo and perspective navigation. New/Open/Save As are connected to validated JSON scene files, and a library API exports OBJ geometry. Overall Maya coverage is unmeasured.

[Current capabilities](#current-capabilities) · [Build and test](#build-and-test) · [Roadmap](#roadmap-and-publication) · [Project documentation](.project-docs/INDEX.md)

## Current capabilities

| Area | This checkout | Evidence or next step |
| --- | --- | --- |
| Scene data | Cube/plane creation, validated quad meshes, stable IDs, rename, remove and transform edits | [Scene workflow tests](tests/scene_workflows.rs) and [mesh validation tests](tests/mesh_workflows.rs) |
| Transform math | Scale, fixed-axis XYZ Euler rotation in degrees, then translation, including zero/negative scales | [Coordinate conventions](.project-docs/development.md) and scene tests |
| Invalid edits | Reject invalid names, non-finite transforms and transformed-coordinate overflow without changing the scene | Scene tests |
| History | Undo/redo create, remove, rename, transform and single-face extrusion commands, with failed/no-op edits preserving history | [History workflow tests](tests/history_workflows.rs) |
| Native scene files | JSON v1 for quad meshes, validated load, 16 MiB limit and create-new saves that preserve occupied destinations | [Document workflow tests](tests/document_workflows.rs) |
| Camera math | Orbit/pan/dolly, perspective projection, rays, nearest-geometry picking and framing | [Viewport workflow tests](tests/viewport_workflows.rs) |
| Desktop shell and viewport | Create/select cube or plane, edit name/transform channels, undo/redo, delete and frame selected, with projected mesh drawing | [Native shell](src/app.rs) and [viewport UI](src/viewport_ui.rs) |
| File actions in the window | New/Open/Save As, unsaved-change confirmation and error presentation | [File integration tests](src/file_ui.rs) and [scoped macOS native verification](.project-docs/native-verification.md); full panel navigation and other platforms remain pending |
| OBJ export | Library API writes world-space polygon geometry to a new file | [OBJ workflow tests](tests/obj_workflows.rs), with GUI Export still disabled |
| MCP | Optional stdio server controls twelve tools in its newly launched native editor window | [Tool contract and limits](.project-docs/mcp.md), with real Editor/official SDK tests; native protocol/window verification remains separate |
| Modeling | Core API extrudes one quad along its object-space normal, including signed/zero distances; native face UI and all-face extrusion pending | [Extrusion workflow tests](tests/extrusion_workflows.rs) |
| UVs, animation and rigging | Full workflows pending | [Implementation plan](.project-docs/plan.md) |
| Materials, simulation and rendering | Pending | Reference scenes and output comparisons required |
| Maya formats, scripting and plug-ins | Pending | Existing scene/script/plug-in fixtures required |

Core workflow tests and UI event regression tests cover implemented behavior. The exact Maya reference installation hasn't been pinned, so they don't establish visual or functional parity. Unsupported modeling, animation, material and other controls are disabled in the prototype.

The native shelf and toolbox use colorful vector tool symbols with readable labels.
Unsupported transform tools remain disabled, and Select remains a mode indicator.

[The extruded cube example](examples/extruded-cube.aztec) is a synthetic AztecCraft
scene generated through two real extrusion commands. Open it with File → Open to
inspect the resulting L-shaped quad mesh. Tests reconstruct it, exercise
undo/redo, save/reload, reflected OBJ export and camera picking. It is not a Maya
reference scene; native face selection/extrusion controls remain pending.

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

## Architecture and platforms

[src/scene.rs](src/scene.rs) owns validated scene state, [src/history.rs](src/history.rs) applies commands using scene snapshots, and [src/document.rs](src/document.rs) loads and saves native JSON through pinned `craft-atomic` create-new writes. [src/viewport.rs](src/viewport.rs) supplies camera math to [the desktop shell](src/app.rs) and [viewport UI](src/viewport_ui.rs). The scene model remains independent of the window so editing and file handling can be tested headlessly.

History is in-memory and unbounded, and isn't stored in scene files. Loading accepts finite quad geometry with valid distinct indices, preserving empty, unused, degenerate and non-manifold authoring data. Native topology editing is pending, and saving requires a new destination. The camera core doesn't provide a depth-tested renderer or establish Maya navigation parity. See [the detailed limits](.project-docs/development.md).

Cross-platform desktop support and packaged launch behavior still need native evidence for macOS, Windows and Linux. The [CI matrix](.project-docs/builds.md) defines checks for those platforms, and a configured job doesn't establish a passing build. The eframe/egui UI remains provisional, with docking, focus, marking menus and Maya-style editing still to be compared against the reference.

## Roadmap and publication

Development proceeds through small, independently reviewed local commits:

1. Connect GUI Export to the implemented OBJ API and expand interoperability tests.
2. Verify the complete create/edit/undo/save/reopen/export journey through native dialogs, then establish platform packages and launch checks.
3. Build reviewed modeling, UV, animation and rigging slices, followed by materials, simulation, rendering, interchange, scripting and extensions.
4. Compare the complete audited workflow inventory with the pinned Maya build, including existing plug-ins and all major domains.

**The user has authorized publishing this early prototype and merging it to `main`.** That instruction supersedes the original requirement to wait for 80–90% of all Maya 2026 functionality before the first push. The 80–90% full-functionality target and identical-interface-and-behavior objective remain unmet acceptance requirements, including simulation, rendering, scripting and existing plug-in compatibility.

The [100-workflow research seed](.project-docs/inventory-seed.md) is provisional and doesn't supply the denominator. Its measurement proposal uses fixed domain budgets, zero credit for untested/partial workflows and mandatory compatibility evidence, so adding trivial rows can't inflate coverage. Exact final parity is a separate acceptance requirement.

Native verification must cover startup, creation/selection, numeric edits, undo/redo, navigation, file operations, error handling and visibly unsupported controls. [The macOS verification record](.project-docs/native-verification.md) covers scene editing and file integration using OCU-supplied native panel choices. Full interactive panel navigation, other platforms and the disabled GUI Export journey remain pending. Reference screenshots/outputs are needed before any fidelity claim. See [known limitations](known-issues.md).

## References and development

The [reference dossier](.project-docs/reference.md) links Autodesk's documentation and the Craft projects researched for product and implementation ideas. AztecCraft is an independent implementation, and those sources don't establish Maya compatibility or grant a license for this project. No project distribution license has been selected in this checkout.

See [the build specification](.project-docs/build-prompt.md), [the detailed plan](.project-docs/plan.md) and [contribution guidance](CONTRIBUTING.md) before making changes. Reference assets, private scene files and personal paths stay out of commits unless explicitly approved for distribution.
