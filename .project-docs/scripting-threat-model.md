# Scripting and plug-in threat model

Design note for issue #57 (scripting and plug-in compatibility). It states what AztecCraft may ever
execute, what was evaluated, and the smallest first slice that can be proven without an
interpreter. It is a proposal: the owner-only decision in section 6 is open, and until it is made
nothing in this document authorizes executing any script. Nothing here is a Maya parity claim (#5).

## 1. Rule today

- Imported scene scripts, expressions and bundled MEL are inert data. They are never executed to
  make an unsupported scene load (`AGENTS.md`, "Bind replacement/save decisions...").
- No network listener and no server is started by default; automation (MCP) is opt-in and uses the
  same typed operations and guards as the native UI (`AGENTS.md`, "Automation is part of the
  feature"; `.project-docs/guard-ledger.md`).
- Every state change goes through an `Operation` (`src/editor_operations.rs`), guarded by the
  scene token, draft, modal, close and busy checks. Some variants compile only under `cfg(test)` or
  the `mcp` feature.

## 2. What may ever be executed

| Source of text | Execute? | Notes |
| --- | --- | --- |
| Scene file contents (`scriptNode`, expressions, bundled MEL, file-embedded Python) | Never | Security preferences for files that run code on load are also listed on #58; coordinate, do not duplicate. Kept as data, round-tripped byte-faithfully where the writer supports it, surfaced as "present, not executed". Same as the strictest Maya Security preference [S9]. |
| MCP string arguments | Never as a language | Typed fields only. No tool takes a "MEL" or "Python" string to evaluate. |
| User-typed Command Line text (#63) | Only the typed subset in section 7, and only after the owner decision | One command per line, parsed to a typed command, resolved against the scene, then dispatched through the normal guards. |
| Script Editor buffer | Same as Command Line | A buffer is a list of Command Line lines, not a program. No procedures, no `eval`. |
| User-chosen script file (a `source`-like action) | Not in the first release | Path-taking; would need its own decision and the existing path validation. Never triggered by opening a scene. |
| Shelf button commands, marking menus, hotkeys, imported shelf or preference MEL files (#65, #32) | Never when imported | Imported ones are inert data. A user's own shelf entry would be a typed command only. |
| Plug-ins (`loadPlugin`) | Never in-process native code | See section 4. |
| `commandPort` text | Same typed subset, opt-in listener (#57 owner decision 2026-11-05) | Subject to section 3. |

Invariant: a parse that fails or contains any unsupported token yields no commands: the whole
buffer is parsed before anything runs, so a syntax error runs nothing. Runtime failure is a
different case: the editor protocol has no rollback across several operations (undo chunks exist but do not
nest, `.project-docs/mcp-tools.md`; parity is #391). Behaviour on a runtime refusal at line
N (a guard rejection after lines 1 to N-1 applied) is therefore: stop, report the line, keep the
applied lines as ordinary undoable operations. No promise of atomic scripts is made. There is no
fallback to a general evaluator and no string-to-code path.

## 3. Threats

| Threat | Vector | Mitigation in the recommended design |
| --- | --- | --- |
| Arbitrary code execution from a scene file | `scriptNode` on load; `File > Open -executeScriptNodes` defaults to true in Maya [S7][S8] | Scene scripts stay inert; no execute-on-load path exists. A test opens a fixture containing a marker script and asserts no side effect. |
| Prompt-injected or untrusted MCP caller runs code | A tool that evaluates a string | No evaluating tool. Command subset is typed and resolved into existing `Operation`s (each slice-1 command maps to exactly one), so a caller can do through the Command Line only what existing tools already allow. |
| Unauthenticated remote execution | A `commandPort` listener runs commands with the user's permissions and requires no authorization [S5] | Opt-in, localhost by default, same typed subset and guards, no tokens or scene payloads in logs. Non-localhost bind needs an explicit user setting. A listener is a separate decision from the interpreter and is tracked in its own issue. |
| Sandbox escape from an embedded interpreter | CPython or a MEL evaluator linked into a native editor with file, process and network access | No general interpreter embedded (section 4). |
| Native plug-in compromise | Loading a third-party shared library into the editor process | Not supported; no ABI loading. |
| Resource exhaustion | Huge scripts, deep nesting, loops | Typed subset has no loops or procedures; input length and line count are capped (limits set in the child issue and covered by boundary tests). |
| Secret leakage | Errors or logs echoing scene payloads or script text | Errors name the line number and the unsupported token class, not the payload. |
| Path abuse | `source`, `file`, `loadPlugin` take paths | Out of the first subset. Any later path-taking command reuses the existing 1 to 4096 byte, no-NUL path validation and file guards. |

## 4. Options evaluated

| Option | What it gives | Security cost | Verdict |
| --- | --- | --- | --- |
| A. No execution at all (today) | Safe | None | Baseline; keeps #57 unsatisfied. |
| B. Typed command subset, pure parser, no evaluator | Command Line and Script Editor usable for a documented command list; MCP and UI share operations | Parser bug surface only; cannot do more than existing operations | Recommended default. |
| C. Embedded MEL interpreter (own implementation) | Real MEL language: procs, loops, expressions, `eval` | Full language reachable from any text source; `system`, file and `eval` semantics must be removed or sandboxed one by one; very large surface (MEL and Python `maya.cmds` share one command set [S1]) | Needs maintainer decision; staged after B if ever. |
| D. Embedded CPython | `maya.cmds`-style scripting; Python 3 era; Python 2 semantics emulation for compatibility versions 2011 to 2021 was decided on #57 (owner, 2026-11-05) | CPython has no in-process sandbox; imports, `ctypes`, `os`, `subprocess` reachable unless a restricted build and audit hook policy is maintained; dependency, packaging and licence cost | Needs maintainer decision; not a first-slice candidate. |
| E. Out-of-process interpreter speaking the typed protocol (stdio MCP-like, or `commandPort` compatible) | The user's own Python or MEL tool drives the editor; the editor executes nothing | Authority is whatever the OS gives the separate process; the editor only accepts typed operations through the opt-in channel | Compatible with B; the lowest-risk way to meet Python demand. |
| F. WASM or other capability runtime for scripts | Capability-limited execution, no ambient file or network | New runtime, no Maya-language compatibility, large design effort | Evaluated, not recommended for #57. |
| G. Native plug-in loading (shared libraries, C++ API/ABI) | Real Maya plug-in compatibility [S11][S12][S13] | Arbitrary native code in-process, ABI tied to Maya's C++ API which this editor does not implement | Not supportable; compatibility policy issue records the position. A plug-in node found in a scene is preserved as an unknown node, as `requires` does when a plug-in is missing [S13]. |

Sandbox and capability notes: any option that evaluates general text (C, D, F) must run with no
file, process or network capability by default and with each capability granted per operation by an
explicit user setting. Nothing in this repository provides that today, so those options start with
a design, not code.

## 5. Why an embedded interpreter is a security decision

A native editor runs with the user's full permissions and opens files from third parties. An
interpreter turns every text input (a scene, a pasted snippet, an MCP argument, a socket) into a
code-execution path, and Maya's own history (execute-on-load defaults, the 2022 Security
preferences [S9], an unauthenticated `commandPort` [S5]) shows the cost of doing it by default. The
choice between B, C, D and E also fixes the dependency set, the packaging licences and the public
promise to users, so it is the maintainer's call, not an implementation detail.

## 6. Owner-only decision

Decision needed: may AztecCraft ever execute user-supplied script text, and in what form?

Recommended default (proposed, not yet decided):

1. Never execute anything from scene files or MCP string arguments.
2. Allow execution only of an explicit typed-command subset (option B), parsed by a pure function
   into a typed command, resolved against the scene into existing `Operation`s (each slice-1 command to exactly one) and dispatched through the normal guards, with no
   expressions, variables, procedures, loops, `eval`, `system` or path-taking commands in the
   first release.
3. Serve Python and MEL-language demand through out-of-process clients (option E) before any
   embedded interpreter. Revisit C and D only with a written sandbox design and a fresh decision.
   This recommendation is in tension with the 2026-11-05 decision that Python 2 semantics are
   emulated for 2011 to 2021: with an external interpreter the editor cannot emulate them. The
   concrete proposal is that the editor emulates only what it owns (the per-version command and
   flag gate, #178 picker) and a client shim carries 2-versus-3 behaviour. The owner confirms or
   withdraws the emulation decision.
4. No in-process native plug-in loading.
5. A `commandPort`-compatible listener (decided allowed, 2026-11-05) carries only the typed
   subset. Recommended authentication: UNIX socket with 0600 mode where available; a localhost INET
   port requires a per-session token (never logged), since INET and Windows have no peer
   credentials; a non-localhost bind needs both the explicit user setting and the token.

Open questions for the owner: (a) is the recommended default accepted; (b) is an embedded
interpreter (C or D) ever in scope; (c) confirm the listener carries only the typed subset with
the authentication in item 5; (d) confirm or withdraw Python 2 emulation as reshaped in item 3;
(e) is native plug-in loading excluded for good (item 4). Child issues that say "gated on the
ADR" stay blocked until answered.

## 7. First slice proposal (provable without launching the app)

Slice 1, pure parser, no UI, no execution of expressions:

- A pure function `parse_command_line(&str) -> Result<ParsedCommand, ParseError>` in a new
  module, no I/O, no global state, no clock, no recursion. It returns a typed `ParsedCommand`
  (command, flag values, node names, bounded numbers), not an `Operation`: an `Operation` needs
  scene `NodeId`s (`src/editor_operations.rs` `Rename`, `Select`, `EditNode`), and turning `pCube1`
  into an id needs the scene. A separate resolve step (scene snapshot to `Operation`) belongs to
  slice 2.
- Whitelist of 3 to 4 commands whose Maya pages are public and whose flags map onto exactly one
  existing `Operation`: `select -replace` with one object, `select -clear` (`DeselectAll`),
  `rename` (`Rename`), `xform` translation/rotation/scale flags (`EditNode`, which takes
  optional channels; `SetTransform` needs all nine). Excluded from slice 1 because one command
  needs several operations: `move`/`rotate`/`scale` (Begin, Update and Finish manipulation) and
  multi-object `select`. Note that `Rename`, `SetTransform` and `EditNode` are gated
  `#[cfg(any(test, feature = "mcp"))]`; slice 1 tests build them under `cfg(test)`, and slice 2
  must ungate or otherwise make them available to the UI build before a Command Line can
  dispatch them. Each command's accepted flags come from its page under `/Commands/` [S1];
  unknown flag, unknown command, variable (`$x`), backtick, `;` sequence, expression, string
  escape beyond a documented set, or a length over a fixed cap is a `ParseError` that names the
  token class and offset, never the payload.
- Numbers are parsed as bounded finite f64 in the scene unit. `xform` rotation flags are degrees (the `EditNode` rule, regardless of Maya's angle unit; PROVISIONAL). `select -clear` maps to `DeselectAll`, which also returns to Object mode; the accepted-line table records that difference.
- Tests: table of accepted lines to exact `ParsedCommand` values; a table of rejected lines
  (empty, oversized, NUL, nested, `eval`, `system`, backticks, `source`, `loadPlugin`,
  `scriptNode`); non-UTF-8 input is rejected at the byte-to-`&str` boundary in slice 2; totality
  (never panics) checked by an exhaustive table of short inputs over a small alphabet plus a
  deterministic pseudo-random corpus, with no new dependency; a source-scan test that allowlists
  the parser file's `use` items (`core`, `alloc`, `std::str`, `std::num`, `std::fmt` and its own
  types) and fails on any other `crate::`, `super::` or `std::` path.
- Provable headless with `cargo test`; no display needed.
- Not in slice 1: Command Line UI (#63 stays disabled), MCP exposure, name resolution, dispatch.
  Slice 2 adds the resolve step and a typed MCP tool taking a structured command list (not a
  language string) with schema, protocol tests and a coverage-inventory row, then Command Line
  enablement with a native visual check.

## 8. Child issues

Tracked from #57; each follows the feature gates (visibly unsupported until done, MCP coverage,
protocol tests, docs, native check).

- #442 boundary ADR (owner decision)
- #443 typed command engine shared by UI and MCP
- #444 MEL command subset (slice 1 here)
- #445 Python command subset
- #446 Script Editor window (Command Line stays in #63)
- #447 plug-in loading and API/ABI policy
- #448 scene-embedded scripts as inert data
- #449 commandPort-compatible localhost listener

## Sources

- [S1] Command reference, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/index.html
- [S5] commandPort, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/commandPort.html
- [S7] scriptNode, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/scriptNode.html
- [S8] file, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/file.html
- [S9] Security preferences, 2026: https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Customizing/files/GUID-2176117C-6998-4802-9F9F-1417DF41C661.htm
- [S11] loadPlugin, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/loadPlugin.html
- [S12] What's new in the Maya 2027 devkit: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-DEVHELP/files/Whats-New-Whats-Changed/2027-Whats-New-in-API.html
- [S13] requires, 2027: https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/requires.html

The labels [Sn] match the source list on #57. Command pages were read only as quoted on #57; the
flag lists of the first-slice commands must be read from the pages when slice 1 is implemented.
