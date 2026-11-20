# ADR: sandboxed user scripting (#57, #442)

Status: accepted. The owner decided the sandbox (bundled CPython in WASM) and answered Q-approval (A), Q-publish (A) and Q-size (A) on 2026-11-15; the lane decisions below stay overridable. Nothing here is a Maya parity claim (#5). Supersedes the "Recommended default (proposed)" of [scripting-threat-model.md](scripting-threat-model.md) section 6. Slice 1 is a Python Script Editor window; later slices are listed at the end. The full security-reviewed plan is kept in the lane's plan file, not copied here.

## Decision

Owner, 2026-11-15, on #57: "B but try to mitigate it somehow maybe by running python in a sandbox of some sorts". Reading: "B" is the answer to "may AztecCraft ever execute user-supplied script text": yes (the letter is the owner's choice on #57, not the threat model's option table). Later the same day the owner chose the sandbox: a bundled CPython compiled to WASI, run in wasmtime on every platform. Python 3 only; the earlier Python 2 emulation decision (2026-11-05) is withdrawn.

AztecCraft runs user-supplied Python only as follows:

- Bundled runtime. CPython 3.13.x built to `wasm32-wasip1` runs inside wasmtime (pinned LTS line, `=48.0.9`, features limited to the core engine: no component model, wasi-http or GC). There is no user-chosen interpreter and no interpreter validation.
- Helper process. wasmtime runs in a separate `script-host` helper process, never in the editor. The WASM sandbox is the policy boundary (what a script may touch: no preopened directories, no sockets, no environment, no processes; only stdio pipes, coarsened clocks, random and the closed `azteccraft.*` call set). The helper process is the blast-radius boundary: a wasmtime or Cranelift bug, an abort or an out-of-memory reaches the helper's address space, not the scene, MCP state or tokens in the editor.
- Minimal fail-closed OS layer on the helper, as defence in depth (Linux seccomp and Landlock read-only, macOS Seatbelt deny network, file-write, fork, Apple Events and most Mach lookups, Windows AppContainer without capabilities plus a kill-on-close Job). If it cannot be applied, scripting is unavailable and says why; there is no unsandboxed run and no switch to skip it.
- Typed calls only. The helper and the editor speak length-prefixed, bounded frames over the helper's stdin and stdout. The editor accepts a closed set of typed calls (`ScriptCall`), re-validates every one and maps each to one existing editor operation through the same guards as MCP plus a dedicated `Scripting` provenance. The helper is not trusted.
- Python-level restrictions are never counted as a boundary.
- Text runs only on an explicit human action: Script Editor Run, or an MCP `run_script` request that a human approves per run (below). Scenes never trigger execution: `scriptNode`, expressions, bundled MEL, file-embedded Python and imported shelf or preference MEL stay inert data (AGENTS.md).

## Why a helper process (recorded reasoning)

CVE-2023-26489 (a Cranelift address-mode bug that let a guest reach memory well outside its linear memory, including the embedder's) and the Critical Cranelift advisories published 2026-10-19 show that WASM isolation depends on the compiler being correct. In the editor process such a bug would expose the open scene, MCP state, tokens and unsaved work. In the helper it exposes only the helper. What the helper alone does not do: it runs as the same OS user, hence the minimal OS layer above. Residual risk: reads stay allowed on Linux and macOS so the helper can load itself and python.wasm, so an escaped helper could read user files and leak their contents through scripting calls (for example as node names an MCP client then reads). Landlock read scoping is a possible follow-up.

## MCP run (owner answer Q-approval: A)

MCP gets `run_script`, `inspect_scripting` and `cancel_script`. `run_script` never executes by itself: it creates one pending approval, and a native modal shows the exact code (hidden and bidirectional characters made visible) bound to its SHA-256. No tool approves, resolves or toggles anything. The tool is always registered but refused with `scripting_mcp_disabled` until a human enables scripting and, separately, the session-only MCP request opt-in. Recorded residuals: an agent with desktop control can synthesise OS input, click Run and flip the opt-in itself; only an OS-trusted prompt or not granting that agent desktop control defends (Q-UIAuto, still open, lane default A: keep the AccessKit refusal and the default-off opt-in, document the residual). The approval Run button deliberately ignores assistive-technology press actions (an accessibility gap recorded in known-issues, conflict with #465 noted). A prompt-injected script that a hurried human approves is bounded by the sandbox, one undo step and the rule that nothing is saved without a normal Save.

## Owner answers and lane decisions (recorded, overridable)

| Id | Decision |
| --- | --- |
| Sandbox | Bundled CPython 3.13.x in wasmtime, all platforms (owner). |
| Q-approval | A: MCP may run scripts only after per-run native approval (owner). |
| Q-publish | A: python.wasm is built from a pinned source recipe in CI and packaging, verified against a SHA-256 lock, cached; no hosted or released prebuilt artifact (owner). |
| Q-size | A: accept installer growth up to about 80 MB for the helper, python.wasm and notices, adjusted after the T2a and T8b measurements (owner). |
| Helper | Helper process only, no in-process runner; one helper and one fresh Store per run. |
| Limits | 30 s wall clock, fuel sized from measured throughput, 256 MiB guest memory, 768 MiB helper RSS backstop, 256 KiB output, 10,000 calls, 20,000 frames per run, 64 KiB per helper frame, 1 MiB per editor frame. Not user-tunable in slice 1. |
| Undo | One undo chunk per Run ("Script", "Script (MCP)" for approved MCP runs, "Script (stopped)" when cancelled). Maya does not document this grouping: PROVISIONAL. No rollback is promised. |
| Opt-in | Per session, re-confirmed each launch; separate second opt-in for MCP requests. |
| Feature | A `scripting` cargo feature, off by default like `mcp`. Enabling it in a release package is owner-gated. |
| Deps | Pinned `wasmtime` and its closure, plus `landlock`/`seccompiler` and `windows-sys` for the OS layer, are routine and flagged in review. |
| Stdlib | Prefer freezing a curated stdlib into python.wasm; a read-only preopen of the app's checksum-verified stdlib is accepted only if freezing fails. |

## Remaining questions of #442

(a), (b) and (d) are answered (execution is in scope, a sandboxed runtime replaces the typed-subset-only default, Python 3 only). (c), the `commandPort`-compatible listener and its authentication, stays a separate decision and slice: the sandbox boundary is host functions and pipes, never a socket. (e), in-process native plug-in loading, stays excluded. Open owner item: Q-UIAuto.

## Slices

Slice 1: bounded typed protocol, broker and `Scripting` provenance, helper binary with the wasmtime engine and in-house WASI preview1 linker, OS layers, python.wasm build and licence notices, Python shim, Script Editor window, opt-in, MCP tools with the approval modal. Slice 2: file calls behind a native confirmation, Command Line Python mode (#63), MEL, a larger `cmds` set. Slice 1 leaves the Command Line to #63.

## Residual risk

A script can read the whole scene through its typed calls and can ruin the open document (undoable, never saved unless the human saves). The WASM layer depends on wasmtime; the pin follows the advisories and a monthly update task. Out of scope: kernel exploits, side channels, a user who approves a malicious script after reading it.

Evidence plan: WASI-level escape tests with hand-written WAT guests in every CI job, the same cases against the real python.wasm once the CI build exists, a test-only control config proving each case can fail, and one small OS-layer test per platform. Until that gate exists no parity-inventory row claims anything for scripting.
