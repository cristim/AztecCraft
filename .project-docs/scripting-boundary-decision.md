# Scripting boundary: what may ever be executed (issue #442)

Decision record. Docs only: no code changes, no parity claim, and every Maya behaviour named here
is PROVISIONAL until compared with the pinned reference (#5, see [reference.md](reference.md)).
Written against main `508328e4`; file:line anchors below were checked on that commit. It records
the boundary only. The sandbox design, limits, owner answers and slices live in
[scripting-adr.md](scripting-adr.md); the options and threats in
[scripting-threat-model.md](scripting-threat-model.md). Where this record and the threat model's
section 2 differ, this record wins.

## Decision

The owner answered the #57 scripting question on 2026-11-15: "B but try to mitigate it somehow
maybe by running python in a sandbox of some sorts". Reading, as recorded in
[scripting-adr.md](scripting-adr.md): AztecCraft may execute user-supplied script text, but only
inside a sandbox and only on an explicit human action. Everything else stays data. The boundary has
five parts:

1. **Typed commands**: a closed set of typed calls, each mapped to one existing Editor operation
   or read, through the same guards as MCP.
2. **Sandboxed Python**: Python 3 text runs only as bundled CPython in WASM (wasmtime) inside a
   separate helper process, and may call nothing except those typed calls.
3. **Never executed**: MEL (today; the ADR lists MEL as slice 2, which needs its own decision, so no MEL evaluator is decided), and any script embedded in a scene or other file.
4. **No in-process native plug-ins.**
5. **`commandPort`-style listeners carry typed commands only**, never script text.

## What may be executed

| Source | Executed? | Form |
| --- | --- | --- |
| Human-submitted Python (Script Editor Run) | Yes, decided; not yet implemented | Bundled CPython in wasmtime in the `script-host` helper; fresh store per run; talks to the editor only through the typed call set |
| Python requested over MCP (`run_script`) | Yes after approval, decided; not yet implemented | One pending approval per run; a native modal shows the exact code bound to its SHA-256; no tool approves; refused with `scripting_mcp_disabled` until a human enables scripting and, separately, the session MCP opt-in |
| Typed calls from a sandboxed script (the `ScriptCall` set is for the helper only) | Yes, calls are decided; broker not yet implemented | `ls`, `select`, `select_clear`, `rename`, `xform`, `undo_info_query`; anything else is `UnknownCall` |
| `commandPort`-compatible listener | Decided separately, not yet implemented | Typed commands only; authentication is its own decision (#449) |

Implemented today (verified): the call vocabulary and its argument validation
(`src/scripting/protocol.rs:105-130` the closed `ScriptCall` enum, `src/scripting/protocol.rs:210-241`
`from_wire`, whose final arm `_ => Err(ProtocolError::UnknownCall)` at line 241 refuses every other
name), the bounded framing (`src/scripting/frame.rs`) and the helper-side wasmtime engine
configuration (`src/scripting/guest/engine.rs:113-134`). All three sit behind the off-by-default
`scripting` cargo feature (`Cargo.toml:14`, `src/main.rs:7-8`) and are not reachable from the UI or
MCP: `src/scripting/mod.rs:4-8` marks the protocol and frame modules `#[allow(dead_code)]` until
the broker exists, and `src/scripting/mod.rs:9-12` builds the engine only for tests. The helper
binary, the OS layer, python.wasm, the broker, the Script Editor window and the MCP tools have no
code on `main` (a search of `src/` for `script-host`, `run_script`, `landlock`, `seccomp` and
`python.wasm` finds nothing outside `src/scripting/` comments). `known-issues.md:1683-1685` and
parity row SCR-001 in `.project-docs/parity-inventory/12-scripting-and-extensions.md:9` say the
same: nothing executes user scripts yet.

## What is refused

| Input | Policy | Anchor |
| --- | --- | --- |
| `scriptNode` and other script nodes, expressions, bundled MEL, file-embedded Python in scenes | Never executed, never run to make a scene load; kept as inert data | `AGENTS.md:39-41` |
| MEL text from any source | Never executed. MEL is not served by the sandbox and an evaluator is out of scope (slice 2 of the ADR may revisit it with a fresh decision) | Decision here; [scripting-adr.md](scripting-adr.md) "Slices" |
| MEL parsed by a fixed grammar into typed commands (#444 stage 1) | Not execution: nothing is evaluated, unsupported tokens are refused; a MEL evaluator stays undecided | [scripting-threat-model.md](scripting-threat-model.md) section 7 |
| Imported shelf, hotkey and preference MEL | Inert data | [scripting-threat-model.md](scripting-threat-model.md) section 2 |
| MCP string arguments as a language | Refused; typed fields only, except the approved `run_script` above | Decision here |
| Native plug-ins (`loadPlugin`, shared libraries, C++ API/ABI) | Never loaded in-process. A plug-in node in a scene is kept as an unknown node | [scripting-threat-model.md](scripting-threat-model.md) options G; #447 |
| Script text arriving through a socket | Refused: a listener speaks typed commands, never a language | Decision here |
| A run with no working sandbox | Refused with the reason; no unsandboxed run and no switch to skip the OS layer | [scripting-adr.md](scripting-adr.md) "Decision" |
| Python-level restrictions as the boundary | Never counted as a boundary; the WASM sandbox and the helper process are | [scripting-adr.md](scripting-adr.md) "Decision" |

Current behaviour of the scene reader, as evidence that scripts stay inert: the ASCII reader
admits a `script` node only for the two configuration nodes `uiConfigurationScriptNode` and
`sceneConfigurationScriptNode` (`src/native_scene/ascii.rs:548-552`), and for those it parses
only `.b` (the body, consumed as a string with `c.string_body()`) and `.st` (an integer)
(`src/native_scene/ascii.rs:879-895`); any other attribute, and any other script node, is
`ErrorKind::Unsupported` (`src/native_scene/ascii.rs:553`, `:891`). The body is read as text and
never handed to an evaluator. Whether the binary reader and every other load path keep scripts
inert is unverified here; the `AGENTS.md` rule applies to all of them.

## Approval path

- Script Editor Run is the human action itself; no scene event triggers it.
- MCP `run_script` creates a pending approval; a human approves it in a native modal showing the
  exact code with hidden and bidirectional characters made visible. Approval is per run, for the
  displayed SHA-256, and no MCP tool can approve, resolve or toggle it. Decided, not yet
  implemented.
- Two opt-ins gate MCP requests: scripting enabled by a human, and a separate session-only MCP
  request opt-in, both default off. The `scripting` cargo feature is off by default, and enabling
  it in a release package is owner-gated. Decided, not yet implemented (`Cargo.toml:14` is the
  feature).
- Each run is one undo chunk; nothing is saved without a normal Save. Decided, not yet
  implemented; the Maya grouping is PROVISIONAL.
- Recorded residual: an agent with desktop control can synthesise the click and the opt-in
  (Q-UIAuto, open, see [scripting-adr.md](scripting-adr.md)).

## Changes to the threat model

Threat-model rows that still say Python and MEL run only through a typed-subset parser or an
out-of-process client are history; the ADR and this record replace them. The Python 2 emulation of
2026-11-05 is withdrawn (Python 3 only), per the ADR. The question of the `commandPort`
listener's authentication (#449) and plug-in policy (#447) stay with their issues.

## What breaks if this is wrong

If a scene or an MCP string could reach an evaluator, opening a downloaded file would run code with
the user's permissions, which is the failure the Maya Security preferences exist to prevent
([scripting-threat-model.md](scripting-threat-model.md) section 5). Child issues that say "gated on
the ADR" (#443, #445 to #447, #449, and #444 from stage 2) are unblocked by this record on the terms above; #448 can start now.
