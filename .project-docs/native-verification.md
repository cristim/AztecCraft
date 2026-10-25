# Native verification record

## macOS quit guard: Command-Q (#11), Dock Quit, logout, shutdown and SIGTERM (#261), partly verified natively

Command-Q, the Quit item, Dock Quit, logout and shutdown send `terminate:`, which exits inside AppKit
and never produces a winit `CloseRequested`. `crates/macos-quit-menu` adds `applicationShouldTerminate:`
to winit's application delegate; the hook returns "terminate later" and routes the request through the
existing close guard. SIGTERM (macOS and Linux) starts the same guard (PROVISIONAL). The #259
`performClose:` retarget of the Quit item was retired in #261, so Command-Q now reaches the hook. Headless
tests cover the pure termination logic and the hook on a hand-built delegate; the checks below are native.

Native runs (macOS, dirty scene made with the Cube shelf button, driven by an automation
tool, 2026-10-25):

| Check | Result |
| --- | --- |
| S0 spike case A: dirty scene, Dock-style quit (`NSRunningApplication terminate`), auto-Discard | passed: the prompt shows, the deferred reply runs during terminate-wait, `on_exit` runs exactly once, the process exits (no double exit) |
| S0 spike case B: same, then Escape | passed: the prompt takes input while AppKit waits, the app stays alive with the scene dirty; a second Dock-style quit prompts again |
| Real branch build (`53de50b`): the hook installs at startup | passed: no startup error |
| Real build: dirty scene + Dock-style quit, then Escape | passed: the prompt shows, the app stays alive, the scene stays dirty |
| Real build: first SIGTERM on a dirty scene | passed: the prompt shows and the app stays alive (before the change SIGTERM killed it) |
| Real build: second SIGTERM while the prompt is up | passed: the process exits (forced-exit path, PROVISIONAL) |
| Command-Q through the hook (build `b68054f`, after the retarget was removed) | passed: the prompt shows, Escape keeps the app alive |
| AztecCraft > Quit menu click through the hook | pending |
| Command-Q with no key window (no beep, the hook picks the window) | pending |
| Discard and Save after a Dock quit with real clicks | pending (S0 covered Discard only with auto-Discard; the modal ignores synthetic pointer events) |
| Clean scene + Command-Q or Dock Quit quits at once | pending |
| Real logout or shutdown, including Save on the prompt (expected to abort the OS action) | pending |
| A terminate arriving after a viewport close was sent (logout cancelled, the app still exits) | pending |
| Open panel up, then Dock quit | passed (build 61c7a87): Dock quit with the Open panel up is declined (logged), the panel stays usable, no hang; a later Dock quit prompts |
| Second Dock quit while the prompt is up, then Cancel | passed (main 2914585): no nested wait, the app stays responsive, a later quit prompts again |
| Second Command-Q while the prompt is up, then Cancel | pending |
| Quit menu click while the prompt is up, then Cancel | pending |
| A decline followed at once by a new terminate (expected: declined while the earlier reply is queued; a later quit prompts) | pending natively; covered headlessly by the hook harness (#268) |
| Save panel, alert key, pending text input or invalid draft (expected: decline, no hang) | pending (record the outcome; an unguarded exit or lost data is a finding) |
| Minimized window | pending |
| App hidden with Command-H | pending |
| About, Hide, Hide Others, Show All, Services and the red close button still prompts | pending |
| `--features mcp` build with a client attached, `request_close` and `resolve_file_action` during an owed terminate | pending |
| SIGTERM on Linux | pending |
| SIGINT and SIGHUP | out of scope (#261) |

Maya reference (PROVISIONAL where silent): the 2026 Help File menu page
(https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Basics/files/GUID-9D1021A8-60A4-4B72-9A90-7C5070A9E037.htm)
says Exit prompts to save when the scene was not recently saved and gives no hotkey; the MEL `quit` command
page (https://help.autodesk.com/cloudhelp/2026/JPN/Maya-Tech-Docs/Commands/quit.html) covers
`-force`/`-abort`. The dialog wording, Cancel button, Command-Q mapping and SIGTERM behavior are PROVISIONAL.

## Cold verification of material Diffuse editing (2026-10-19)

Verified source: `origin/main` commit [`7ca0ebf`](https://github.com/cristim/AztecCraft/commit/7ca0ebfd6709e3afd624d8fae057f93165fd517b), macOS (Darwin 25.1, Apple Silicon),
Rust 1.99.0, one independent session that did not write the feature. CI for that
commit ([Rust checks run 39319352346](https://github.com/cristim/AztecCraft/actions/runs/39319352346)) passed on all six jobs: macOS, Ubuntu and
Windows checks, software Vulkan viewport, dependency security and the macOS x86_64
cross-build. Fixtures are the synthetic in-test appearance specimens (one and two
shading-group users); no private scene was used.

| Command (filter `material diffuse`) | Result |
| --- | --- |
| `cargo test --locked` | lib 23, bin 15, `maya_ascii_framing` 1: all passed |
| `cargo test --locked --no-default-features` | 23 and 1 passed |
| `cargo test --locked --features mcp-test-client` | lib 23, bin 25 (includes `material_sdk_edit_inspect_history_and_repeated_save_share_the_native_word`), 1 passed |

Read and exercised: finite and nonnegative validation (`NativeScene::set_material_diffuse`
in `src/native_scene/appearance.rs`; `-1`, NaN and infinities refused with baseline
unchanged, tested over source values including `-0.0`, a negative word and
`1.0000000000000004`); eight-byte span patching in `src/native_scene/binary/encode.rs`
(only replaced words are written, byte-identical output when unchanged); draft
targeting, precise Enter/Escape, shared History and failed or cancelled Open in
`src/app/tests.rs`; repeated Save and SDK Open of saved output in
`src/mcp_tests/official_client/material.rs`; strict MCP arguments in
`src/mcp/tools.rs`.

Mutation (scratch worktree, removed): changing the negative check to
`value < -1.0e300` failed 2 tests (`diffuse_edits_preserve_raw_zero_words_unusual_source_and_failure_baselines`,
`material_history_noop_revert_and_save_snapshot_use_the_same_scene`); shifting the
patch span start by one byte failed 4 tests in `native_scene::binary::appearance`.
Both mutations were discarded and the baseline passes above were taken unmutated.

Not verified: Save/NCRV/Face/Grid composition. No test combines a material edit
with curves, Face mode and grid state in one saved scene, and none can today:
`binary::encode` returns unsupported when the scene has curves, binary NCRV Open is
refused, and ASCII Save is refused. Guard-matrix cells cover a material draft and Face
mode separately. The ordinary pre-commit hooks (format, Clippy, tests, build, secrets) passed on
this tree when this entry was committed. Also not run: a native visual or
interactive re-check of this build, Windows/Linux interaction, and any rendered
Phong fidelity or Maya parity. Issue #50 stays open.

## M1 material candidate, bounded macOS verification

The current source adds a real Common Material Attributes/Diffuse draft and
`set_material_diffuse` operation backed by the checked native shared-shader value.
The isolated default248/headless192/client314 tests and strict lint passed,
including real TextEdit events and the SDK edit/Undo/Save journey. A later targeted
SDK test also passed actual protocol Open of each saved output, changed document
tokens, inspected exact effective words and another Active Save from the reopened
origin; the retained independent verifier reproduced the corrected journey.
The SDK edit/inspection/History assertions are portable; Save and protocol
reopen assertions run only on Unix, where native binary writing is supported.
The independent source-word oracle and three intended core mutation failures with
exact restored positives are separately recorded. The production MCP build was
copied and hashed under its build lock. Root operated that macOS package: select
a supported shape, edit Diffuse with Enter, Save As to a fresh working file,
toolbar Undo to a dirty baseline and Redo to saved/clean, then File Open the saved
working file and inspect the retained value. An independent private byte check
found changes only inside the authorized eight-byte diffuse envelope; all five
original fixture hashes were unchanged. This proves that bounded native data
journey, not rendered Phong fidelity or other platforms. Published combined-parent
checks remain pending (cold verification and CI: see the 2026-10-19 entry
above). Originals must
never be Save destinations. Material rendering, assignment, textures and color
processing stay unavailable. The historical record below is unchanged.

Recorded 2026-10-14 for source commit `6c96345f7c9754b26dabff0d2ae0eec198441c9b`.
The verification operator exercised a locally packaged debug macOS application.
These results apply to that build and platform, without establishing Maya parity
or Windows/Linux native interaction support.

## Method and evidence boundaries

The journey used Open Computer Use to operate the running application and inspect
screenshots/controls. Text editing used foreground coordinate clicks. Native
file panels were held while OCU `choose_file` supplied a file choice, verifying
the application/panel/file integration. Interactive Finder browsing and choosing
files entirely through panel controls weren't exercised.

Saved fixture JSON was inspected on disk, and occupied-file protection was checked
by comparing file hashes before and after the failed save. Fixture files, paths
and captures remain private and aren't distributed with this repository.

## Observed workflow

| Action | Observed result |
| --- | --- |
| Create cube | Cube appeared in the Outliner and projected viewport |
| Set Translate X to `2`, then Enter | Channel edit committed and the cube moved in the viewport |
| Save As to a new file through the native panel | On-disk JSON had `version: 1` and the cube's translation `[2, 0, 0]` |
| Create plane, request New, then Cancel the unsaved-change modal | Both objects and the current selection were preserved |
| Undo plane creation | Plane was removed, leaving the edited cube |
| Open the saved fixture through the native panel and explicitly Discard unsaved changes | Saved cube with Translate X `2` loaded, selection cleared, and Undo/Redo were cleared |
| Save As to an occupied destination | Error appeared in the application and the existing file hash stayed unchanged |
| Open malformed JSON through the native panel | Invalid-JSON error appeared, while the selected cube, Translate X `2` and current document filename stayed unchanged |

The checks cover successful persistence, explicit cancellation and
discard decisions, no-clobber failure and failed-open state preservation. They
complement the automated core and UI-event tests with native application evidence.

## Remaining verification

- Windows and Linux native interaction and package launch checks remain pending.
- Full interactive file-panel navigation remains pending.
- GUI OBJ Export is disabled, so the complete native export journey remains pending.
- New icon work and subsequent modeling/MCP changes need verification against their own builds.
- Maya reference comparison, the complete functionality denominator and the original 80–90% full-functionality target remain unfulfilled. Overall Maya coverage is unmeasured.

## Native Maya binary Save integration scope

The preceding JSON results remain historical evidence for `6c96345`. They do not
verify the newer Maya binary writer, current native file lifecycle or current
MCP catalog.

The document adapter at `f7354a2` was checked with the ordinary vendored atomic
engine. Its focused Unix tests passed, and five retained private Maya binary
inputs each passed six working-copy save cycles: fresh Save As, edited Active
Save, repeated Save, second fresh Save As, a further edited Active Save and
another repeated Save. The checks compared the latest destination baseline,
original parse Arc, reopened internal storage words and unchanged prior leaves.
Original asset sizes and hashes matched before and after. Independent focused
and private-corpus execution was performed before the appearance-parent rebase;
the rebased runtime logs and original hashes were independently checked. This
is document-layer evidence, without a native window or file picker.

The application candidate composes that adapter with native Save/Save As,
Save/Discard/Cancel and the same typed MCP operations. It retains the current
appearance summaries and editing tools. Its current-branch Editor/official SDK
checks, independent committed verification and native GUI verification remain
pending. Device-free tests and controlled uncertain-write tests do not establish
a GUI journey or promise disk-fault behavior on every platform.

The all-five native acceptance journey is pending: open a working copy with
visible geometry, edit an existing supported channel, Save As to a fresh leaf,
edit and Active Save, repeat Save, verify Undo/Redo dirty state before reopening,
then use a second Save As and Active Save destination. Original inputs must
never be active write destinations. Queued New/Open/Close Save/Cancel/Discard,
occupied destinations, source conflicts, failed Open and native picker Cancel
also remain pending for this application candidate.

The synchronous native picker still holds the UI loop. A queued remote request
can time out during the panel; after Cancel, expired work must not mutate the
document. Full interactive panel navigation remains a separate acceptance item.
An observed macOS abort during an intercepted Open panel has no reproduced
initiating trigger; subsequent hooked and unhooked Open/Cancel passed with the
same binary. That observation supplies no confirmed cause or fix.

Windows writing, Maya ASCII, structural source rewriting, complete appearance
and Maya parity remain unsupported or unverified. No private paths, assets,
captures or raw numeric source values are distributed with this record.

## Grid: isolated automated and native evidence

On 2026-10-15, the isolated Grid candidate based on Save commit `4e8f87f`
passed thirteen distinct editor, schema and official SDK tests, plus two explicitly
selected headless GPU tests on Apple M1 Pro with Metal. The tested App source SHA256 was
`94a3c88346817a9d5c077972e68f200f1085e2ea1223919c0b37a716ed958274`;
the renderer SHA256 was
`00d2b93c236f1d4d61ddbc3d717932b55659d31c53e8fa3c8cecda53ffa03758`.
The twelve-file source/document manifest SHA256 was
`7bc6b7201f1dd3c4a6bd2f21401ada33649671245a526e4fb773c38a73730b8f`.
All twelve files matched before and after those runs. A later cached matrix
attempt reused another candidate's shared-target executable; its results were
rejected. The corrected run compiled this Grid worktree, listed eleven Grid
tests, and executed the exact Grid SDK test before the full matrix. Default
passed 238 tests with 25 ignored, core without default features 185 with 9
ignored, and MCP client 304 with 27 ignored. The client run executed Grid
setter/button, strict schema, tool catalog and SDK tests. Ignored tests are not
claimed as executed by those matrix commands.

Strict Clippy found one test-only default-field initialization issue. Two
independent review passes confirmed the initializer rewrite preserved behavior;
the affected Grid button test then passed. The resulting App source SHA256 is
`6daa6d9c85bdce47fbc8dbc3fe038a3198f80b2e908d826452118693c1a62995`.
Formatting, strict Clippy for default/core/MCP/client configurations and an
actual Grid production MCP compilation passed with Rust/Cargo 1.99.0. The
captured production executable SHA256 is
`b300f8043a82f778be458fb27733904f7dcfa271ce3c8c0e37ae5003d8a058d7`.
The earlier two explicitly executed GPU tests remain scoped unchanged-source
evidence; they were not rerun for the equivalent test-only initializer change.

Synthetic editor events exercised actual Grid button clicks, disabled modal and
closing controls, idempotent state, drafts and New/Open retention. The GPU test
used the production callback and retained resources across cold empty-hidden,
nonempty and warm empty-hidden frames. At 1x and 2x, empty-hidden frames matched,
the background pixel was `[31,31,31,255]`, visible and hidden grids differed,
and restored grid frames matched. The backend error latch remained empty.
The second GPU test traces an official SDK setter through the next fresh Editor
callback submission and checks healthy inspection; it does not read framebuffer
pixels. These checks establish the scoped automated behavior, not a native
window journey.

The macOS native window using that exact `b300f804` executable passed an empty
clean-scene Grid on/off/on sequence. Hiding Grid after creating a Cube preserved
its geometry, selection, displayed channels and dirty title marker. With Grid
off, native New prompted for unsaved changes; Discard of that session-created
Cube produced a clean empty scene while retaining Grid off. Grid was then turned
on in the empty scene. These are the observed controls and states, not protocol
inspection or Maya comparison.

Two isolated sensitivity probes at source manifest
`9969529ad46d2b215b27bd4efa512b695a0f4dbb14a2ca391b161956a917e65a`
each passed baseline, failed the intended assertion after one mutation, and
passed after exact restoration. Removing the authoritative setting assignment
failed the inspected visibility assertion. Unconditionally emitting grid
vertices failed the actual Metal framebuffer visibility assertion. An independent
reviewer verified raw logs, original/mutated/restored hashes and original
manifest/lock restoration. The shared canonical build target received no mutant output.

The integrated Grid commit `d4c2ed4` on published Save/material parent `d9ec058d`
passed two source/staged review passes, formatting, strict Clippy for all four
configurations and normal commit hooks. Current-parent default/core/MCP-client
suites passed 257/195/326 tests, with 26/10/28 ignored and no failures. A fresh
compile identified the actual landing worktree and Grid/material test inventory.
An independent committed-source check then explicitly ran four tests with zero
ignored: Grid SDK state, material SDK edit/History/repeated Save, Metal empty-frame
grid rasterization and SDK-to-fresh-callback submission. Both GPU tests used
Apple M1 Pro/Metal; only the rasterization test checks framebuffer pixels.

The current-parent production MCP executable SHA256 is
`f033f4c41f0ba70fa5861eb9d62d3a203a6556bb58c6990904e5bb3ec9c10b68`.
Native Open in a controlled macOS launch of that executable using the private
diagnostic modal hook retained Grid off after opening; turning Grid on then made
the grid visible. This adds the observed Open retention path to the
earlier scoped native checks. Remote setter visibility, camera navigation/resize,
draft/modal refusal and general supplied-scene preservation remain outside these
native observations. Feature-branch CI remains pending. Future Face functionality
is a separate change rather than a prerequisite for this Grid slice.
No Maya grid appearance, complete functionality or parity claim follows from
these results. Grid options, global/additional panels and settings persistence
remain unsupported; no shortcut was added.

## Vertex, Edge and Face Status Line buttons: native check

On 2026-10-17, a debug build of source commit `e2ec47a` (Status Line component
buttons) was run natively on macOS with an Apple M1 Pro and driven over its MCP
stdio server with a cube selected. Window screenshots were taken after switching to
each component mode and reviewed by eye.

| Mode | Observed in the window |
| --- | --- |
| Vertex | Header "Vertex selection"; purple markers on the 7 visible cube corners, the hidden corner not drawn; Vertex and Component buttons pressed |
| Edge | Header "Edge selection"; visible edges light blue; Edge and Component buttons pressed |
| Face | Header "Face selection \| visible centers only"; amber centres on the 3 visible faces; Face and Component buttons pressed |

Scope: the mode switches came from MCP `set_selection_mode`, not from clicking the
buttons, so the observed pressed states show the buttons reflect the mode, not that a
click dispatches it (button dispatch is covered by `status_line` tests). Object mode
was not captured: the driver reached Vertex before attaching. This is not a Maya
comparison; colours, sizes, marker shape and button order are provisional
application choices. The screenshots are not committed. Windows, Linux, release builds
and the hidden-corner margin at other distances or adapters are not covered (see
known-issues.md).
