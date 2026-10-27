# Developing AztecCraft

Work locally on small Rust desktop authoring slices, with implementation and independent review owned separately. Read [AGENTS.md](AGENTS.md), [the implementation plan](.project-docs/plan.md), [the reference dossier](.project-docs/reference.md) and [known limitations](known-issues.md) before changing behavior.

## Local workflow

Use an isolated branch/worktree when work is non-trivial or another session may use the checkout. Name the real user workflow, files to change and evidence needed, and review the plan before implementation. Search the existing module and shared code before adding a helper or abstraction.

Run `make check` for formatting, Clippy, tests and the library build. For a focused scene change, `cargo test --locked --test scene_workflows` runs the current workflow suite. See [development instructions](.project-docs/development.md) for the pinned toolchain and conventions.

Keep each commit independently reviewable. Use a distinct reviewer for 2 plan/diff passes and record the exact reviewed tree or commit. A bug regression must reproduce the failing behavior before the fix and pass afterward. Native UI work also requires interaction verification once an executable exists. Tests and screenshots don't establish parity without comparison against the pinned Maya reference.

## Picking up work

Follow [Collaborating on issues](AGENTS.md#collaborating-on-issues) for claims, branch names, approvals and the merge rule. Do not restate it elsewhere.

- Look for the `good first issue` and `help wanted` labels.
- Comment on the issue to claim it, branch `type/<issue-number>-short-slug`, and open a draft PR early with `Closes #<n>`.
- Push each reviewed, atomic commit to the draft PR. Take it out of draft when CI is green, then wait for two approvals and maintainer sign-off (see AGENTS.md); never merge your own PR.
- When CodeRabbit autofix runs on a PR, put documentation changes in their own `docs` commit, not inside the `perf` or `fix` commit it produces.
- Bug fix: skip "Adding a feature"; find the code via the code map, add a regression test that fails before the fix, run `make check`.

## Code map

| Area | Where |
|---|---|
| Core library (no UI) | `src/native_scene*`, `src/document*`, `src/history*`, `src/view_scene*`, `src/viewport.rs`, `src/mesh.rs`, `src/obj.rs`, `src/maya_ascii*`, `src/maya_binary*` |
| Typed operations | `src/editor_operations.rs` (`Operation`, replies, errors) |
| App shell | `src/app/mod.rs`, `src/app/dispatch.rs`, `src/app/inspect.rs`, `src/app/menus.rs`, `src/app/shortcuts.rs`, `src/app/panels.rs` |
| App features | `src/app/features/*.rs`, listed in `src/app/features/mod.rs` |
| File actions (Open, Save, import, export) | `src/file_ui.rs` |
| Viewport | `src/viewport_ui*`, `src/viewport_renderer*`, `src/viewport_shader.wgsl`, `src/move_handles*` |
| MCP server | `src/mcp.rs`, `src/mcp/*` (transport, args, `registry.rs`, `features/*.rs`) |
| Icons | `src/ui_icons.rs`, `src/tool_icons.rs` |
| Unit tests | next to the code (`tests.rs` or `<module>/tests.rs`), `src/app/tests.rs` |
| Editor protocol tests | `src/mcp_tests/*` |
| Safety-net tests | `src/guard_matrix_tests.rs`, `src/entry_point_tests.rs` |
| Integration tests and snapshots | `tests/*.rs`, `tests/support/`, `tests/snapshots/`, `src/snapshots.rs` |
| Docs | `.project-docs/*` (start at [INDEX.md](.project-docs/INDEX.md)), `known-issues.md` |

## Adding a feature

A feature is a typed operation that the native UI and MCP both dispatch. Build it as a vertical slice. Unfinished controls must show as unsupported.

1. **Core change.** Put the logic in the library (for example `src/native_scene/` or `src/history/`) with unit tests. The core must not depend on UI widgets.
2. **Operation variant.** Add a variant to `Operation` in `src/editor_operations.rs`, plus any reply or error it needs.
3. **Guards and dispatch.** In `src/app/dispatch.rs`, classify the variant and add its arm in `apply_operation_guarded` (native and remote share it). Remote-only checks live in `remote_operation_guarded`. Add the variant to the status match in `native_operation` if it should not report "Scene edited". Decide its modal behavior (refuse, allow, or handler decides) in `apply_operation_guarded` and `remote_operation_guarded`. The guard behavior of each variant is recorded in [guard-ledger.md](.project-docs/guard-ledger.md). Keep that ledger true: update it when you change a guard, a side effect or an ordering.
4. **Feature file.** Add `src/app/features/<feature>.rs` (declared in `src/app/features/mod.rs`) when the feature has its own Editor methods or UI hooks. Otherwise dispatch arms are enough (Grid has no feature file).
5. **Native UI.** Hook it into `menus.rs`, `panels.rs` or `shortcuts.rs`. Anything not implemented stays visibly disabled and says why.
6. **MCP tool.** Add a `ToolSpec` (name, description, schema, `parse`, `idempotent`, `read_only`) to `TOOLS` in `src/mcp/features/<feature>.rs`. Declare the module in `src/mcp/features/mod.rs`. Add one entry for it to `FEATURES` in `src/mcp/registry.rs`. Regenerate the table with `UPDATE_DOCS=1 cargo test --features mcp registry::tests::tool_doc`. MCP and app tests live in the bin target, so test filters need no `--lib`. Do not edit `.project-docs/mcp-tools.md` by hand. `src/mcp/tools.rs` still has hand-edited test tables: add a sample-arguments arm in `advertised_catalogue_schemas_and_handlers_match_exactly` (the `_` fallback only fits scene_token-only tools), and list the tool in `only_absolute_state_setters_are_annotated_idempotent` if it is an idempotent state setter. Both tests fail until you do.
7. **Editor protocol tests.** Add tests in `src/mcp_tests/` that drive the real Editor protocol, including drafts, modal state, close handling and scene tokens.
8. **Docs.** Update the coverage inventory in [mcp.md](.project-docs/mcp.md), [parity.md](.project-docs/parity.md) and [known-issues.md](known-issues.md). State what is supported, unsupported and unverified.

### Test gates

These fail on purpose when you add a feature. Fix the cause. Do not just regenerate.

- **Catalog and inspection snapshots** (`tests/snapshots/mcp_tool_catalog.json`, `inspection_*.json`). Run `UPDATE_SNAPSHOTS=1` only for an intended change, then review the diff and explain it in the PR.
- **Guard matrix** (`src/guard_matrix_tests.rs`). `expand` is an exhaustive match, so a new `Operation` variant does not compile until you add its samples there (and a seed in `seeds`). Regenerate `tests/snapshots/guard_matrix*.txt` and review it. A new state or editor field may also need a `State` variant and setup in `build` (`src/guard_matrix_tests.rs`). If guard behavior changes, update [guard-ledger.md](.project-docs/guard-ledger.md) in the same commit.
- **Entry-point gate** (`src/entry_point_tests.rs`). Operations must reach the editor only through the guarded pipelines. A new caller needs a reviewed allowlist entry, not a bypass.
- **Tool table drift test.** `.project-docs/mcp-tools.md` must match the registry (step 6).

Then run the commands in [development.md](.project-docs/development.md). `make check` covers formatting, Clippy, tests and the build. CI runs on Windows, Linux and macOS. Golden files must use LF line endings (`tests/snapshots/**` is marked `eol=lf`) and be compared through `src/snapshots.rs`, never by raw file comparison.

### Where conflicts happen

Parallel features touch the same few places: the `Operation` enum, the dispatch arms and the `FEATURES` list. Append one entry per line in the existing order, and rebase rather than resolve by picking a side.

## Scope and evidence

Unsupported controls must visibly say they're unsupported. Update documentation with the implemented behavior and verification limits, and distinguish work completed in a branch from behavior integrated into the current checkout. Store reference versions, fixture hashes and comparison outcomes so another reviewer can reproduce the result.

Keep reference assets, private scenes, secrets and personal paths out of commits unless explicitly approved for distribution. Preserve existing user data and logs, and obtain per-item confirmation before destructive cleanup. Inspect licenses and preserve required attribution before adopting upstream code. Unless you state otherwise, contributions are accepted under the project's dual license, MIT OR Apache-2.0 (see [README](README.md#license)).

## Publication and acceptance

The user has authorized publishing this early prototype and merging it to `main`, superseding the original first-push restriction. The original 80–90% target across **all Maya 2026 functionality**, including simulation, rendering, scripting and existing plug-in compatibility, remains an unmet acceptance requirement. The complete denominator, exact threshold, component boundaries and evidence remain unresolved. Release/signing and other external actions still require authorization within the current request.

[The initial workflow seed](.project-docs/inventory-seed.md) is a research starting point. It doesn't define completeness or authorize a percentage, and overall coverage remains unmeasured. Use fixed domain budgets, critical must-pass workflows and existing plug-in fixtures when the inventory is audited. Publishing a prototype doesn't establish the final identical-interface-and-behavior requirement.
