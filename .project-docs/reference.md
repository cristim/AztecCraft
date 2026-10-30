# Reference dossier

## Maya

Provisional documentation reference: Autodesk Maya 2027.2, the latest available release verified on 2026-10-18 through Autodesk's [release index](https://www.autodesk.com/support/technical/article/caas/tsarticles/ts/1cLmCcZqflW7nlsH4HafOl.html), [2027.2 release notes](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2027_2_HTML.html) and [What's New](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-49B18BA5-F029-4395-83B5-8B216DBE5914.htm). This dated verification establishes availability, not an exact build, hotfix or installed reference.

Current entry points: [Maya 2027 Help](https://help.autodesk.com/view/MAYAUL/2027/ENU/) and [Maya 2027 Developer Help](https://help.autodesk.com/view/MAYADEV/2027/ENU/).

The original Maya 2026 acceptance target remains active and unmet, including issue #5's requested 2026 installation. Updating the documentation reference does not satisfy that request or establish 2026 parity. Existing 2026 feature citations and comparison evidence retain their original attribution. No installed reference build or user-owned reference scenes have been newly verified; platform, display scale, preferences, workspace and plugins remain unconfirmed. Exact-parity acceptance remains blocked on this evidence.

[The workflow inventory seed](inventory-seed.md) records candidate domains and primary source links. Its 100 initial entries aren't a complete denominator, and overall coverage remains unmeasured. Audit commands, node types, menus, extensions and real production workflows against the installation before assigning a percentage.

## Reference manifest to establish

For each empirical comparison, record the exact installed Maya release, update/hotfix and build, plus full product entitlement, platform/CPU architecture, GPU/driver, locale/display scale, workspace/preferences, OCIO configuration and time/linear/angular units. A comparison with a newer runtime does not itself establish 2026 parity. Record every installed plug-in's version, API version, loaded state and entitlement. Keep machine-specific paths and user-owned assets local.

The component evidence below belongs to the historical Maya 2026 documentation baseline. Resolve current component versions and entitlements separately; do not infer them from the newer Maya year.

Component versions can change independently of the Maya year. [Bifrost](https://help.autodesk.com/cloudhelp/ENU/Bifrost-MayaPlugin/files/Bifrost_MayaPlugin_install_bifrost_for_maya_html.html) is included but separately released, and includes Graph, legacy Fluids, BOSS and `bifcmd`. [MayaUSD](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-USD/files/GUID-8FB49D7F-8651-47CE-80FC-5C940E568C97.html) is included and also separately versioned. Pin Arnold/MtoA, LookdevX, MASH, XGen and Substance versions rather than substituting their latest releases.

[Maya 2026.1](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-WhatsNew/files/GUID-5D9FAEE0-CB9A-41AA-9A7D-014F266C6578.htm) introduces MotionMaker, while [2026.2](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-WhatsNew/files/GUID-0C8224D8-23ED-417D-9630-1F925DC1767C.htm) changes further component versions. Features specific to an update belong in the baseline only when that update is pinned.

Flow Retopology is documented as included with Maya 2026, but cloud execution and Flow Wedging need service/entitlement verification. Golaem is documented as part of the Media & Entertainment Collection, so its inclusion in the requested boundary remains unresolved. Existing third-party scripts and plug-ins also require an explicit fixture inventory, architecture/API checks and execution evidence. A plug-in written only for AztecCraft doesn't prove compatibility with an existing Maya plug-in.

## Bounded ASCII Save documentation comparison

The 2026 and 2027 documentation audit on 2026-10-18 found no material change in the following fields only. It does not establish full command equivalence, query-form coverage, general transform composition parity or runtime compatibility.

| Topic | Compared fields | Audited official pages |
| --- | --- | --- |
| Save As | Menu entry, format choices, ASCII default, extensions and documented shortcut | [2026 English][save-2026], [2027 English][save-2027] |
| `requires` | Product/version requirement and missing-plugin metadata semantics | [2026 English][requires-2026], [2027 English][requires-2027] |
| `currentUnit` | Linear/angular/time numeric interpretation, supported units and animation-retiming default | [2026 English][units-2026], [2027 English][units-2027] |
| `fileInfo` | String keyword/value pairs and preservation through ASCII/binary saves | [2026 English][info-2026], [2027 Japanese][info-2027] |
| `transform` | Translate/rotate/scale defaults, rotation order and internal centimeters/radians | [2026 English node][transform-2026], [2027 Japanese-route node with English body][transform-2027], [2026 English MFnTransform][api-2026], [2027 English MFnTransform][api-2027] |

The 2027 English `fileInfo` and transform-node fetches failed during the audit; the table identifies the official Japanese-route fallbacks used. Conclusions are limited to those pages and fields. The planned ASCII writer's `requires maya "2026"` declaration remains a project compatibility-profile choice. This documentation update changes no output and does not enable ASCII Save or prove acceptance by Maya.

The [2027 fixed-issues list](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ReleaseNotes/files/2027_release_notes/MAYA_FIXEDISSUES_2027_HTML.html) includes same-value Channel Box edits (MAYA-141075), negative-scale rig opening (MAYA-140177) and math-node data loss after saving (MAYA-140532). That list also incorporates 2026-update fixes, and the earlier patch was unpinned, so these entries do not establish differences from the old reference.

[save-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-049E009D-34FE-4767-B9B2-047FFB3142E8.htm
[save-2027]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-049E009D-34FE-4767-B9B2-047FFB3142E8.htm
[requires-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/requires.html
[requires-2027]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/requires.html
[units-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/currentUnit.html
[units-2027]: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/currentUnit.html
[info-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Commands/fileInfo.html
[info-2027]: https://help.autodesk.com/cloudhelp/2027/JPN/Maya-Tech-Docs/Commands/fileInfo.html
[transform-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Tech-Docs/Nodes/transform.html
[transform-2027]: https://help.autodesk.com/cloudhelp/2027/JPN/Maya-Tech-Docs/Nodes/transform.html
[api-2026]: https://help.autodesk.com/cloudhelp/2026/ENU/MAYA-API-REF/cpp_ref/class_m_fn_transform.html
[api-2027]: https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_transform.html

## Authored Maya Binary evidence

Autodesk publishes no Maya Binary specification. What an authored `.mb` may contain, its sources (a vendored public Maya 2025 fixture (the writer's profile source; the Maya 2026 file is used for reads only) and aggregate private-corpus facts) and the invented choices are in [the authored writer note](binary-authored-writer.md). It states that Maya acceptance is unproven.

The grammar for inserting an absent default translate leaf into an opened `.mb` (copy the file's own aux word or refuse) is in [the leaf insertion note](binary-leaf-insertion.md). It is provisional and Maya acceptance is unproven.

## Storytold research

Inspected upstream manifests on 2026-10-14:

- https://github.com/storytold/artcraft: creative scene staging and artist-oriented workflows; product inspiration, not a Maya compatibility implementation.
- https://github.com/storytold/craft-libs/blob/main/Cargo.toml: Rust 2024 workspace, egui 0.36, shared UI/settings/atomic-save components, MIT OR Apache-2.0 workspace license.
- https://github.com/storytold/photocraft/blob/main/Cargo.toml: modular Rust workspace, eframe/egui 0.36, headless document/operations layers, pinned craft-ui revision `5c874d51eeb3de70e735c8ce87a59a7dedd47675`.
- https://docs.rs/eframe/0.36.2/eframe/: current native application API.

Additional source inspection: `crates/atomic/src/lib.rs` provides `atomic_write` and create-new `write_new` with same-directory staging, content flush, cleanup and permission handling. The document layer consumes narrowly vendored `craft-atomic` from upstream revision `457a078809690994b22debdffb703ff36d3129df`, plus the reviewed injectable writable-preflight hook and four test lint fixes. [Vendor provenance](../vendor/craft-atomic/UPSTREAM.md) records exact hashes, the public reconstruction patch, complete licenses and update procedure. `crates/ui/src/lib.rs` exports layout, preferences, render, theme and widgets. Only the UI module exports were inspected, not suitability of each widget.

Upstream atomic-save code is consumed as one source-controlled path dependency. Its full licenses and notices accompany packaged binaries. This prerequisite does not implement native Maya Save or establish upstream acceptance. The scene/history/document/camera core, OBJ API and native editor prototype exist. eframe/egui supplies the current shell and projected drawing, with scene data independent of UI. Manifests alone don't establish reuse suitability. AztecCraft is dual-licensed MIT OR Apache-2.0 (see the root `NOTICE`); the vendored crate keeps its own notices.

## Fidelity gate

egui is provisional, not proven capable of identical Maya UI. Evaluate docking, marking menus, numeric editing, focus routing, styling, and reference screenshots before adopting it as the final UI platform. A first projected viewport establishes functionality, not a production renderer or Maya parity.

The native executable has been built and launched locally, and automated UI event tests exercise editing and file integration. The complete native verification journey remains pending, especially file-dialog interaction and GUI Export. Verification must exercise startup, primitive creation, selection, numeric edits, undo/redo, orbit/pan/dolly, dialogs, save/reopen/export, unsaved-work handling and errors on each claimed platform. Capture exact commit/build identifiers and outcomes. Differential Maya comparisons additionally require matching fixtures, settings and retained output evidence.
