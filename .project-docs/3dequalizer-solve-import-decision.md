# 3DEqualizer solve import without executing scripts (issue #464)

Decision record. Docs only: no code changes, no parity claim, and every Maya or 3DEqualizer
behaviour named here is PROVISIONAL or marked unverified. Written against main `508328e4`. This is
a recommendation recorded under the standing "most sensible non-deferring option" rule; the owner
can override it. It applies the boundary in
[scripting-boundary-decision.md](scripting-boundary-decision.md) (#442).

## Decision

AztecCraft imports a 3DEqualizer solve only from data-only exports, and never evaluates
3DEqualizer's MEL or Python export scripts.

1. **First route: interchange files.** Bring the solve in through the camera formats the interchange
   work adds (Alembic and FBX under #23, USD under #438), once camera fidelity is proven (#463).
   This is option 3 of #464 and its recommended default: it needs no 3DEqualizer-specific parser.
2. **Second route, only if interchange loses data: a fixed-grammar reader (option 1 of #464).** A
   reader of 3DEqualizer's generated Maya export that treats the file as inert text, accepts only
   the fixed command set the exporter emits (camera creation, film-back attributes, focal-length
   keys, locators, rotate order) and refuses anything else with a typed error. Nothing is
   evaluated. It is built only after a written audit of the exporter's output and a new decision
   that names the grammar; until then it does not exist.
3. **Not a route: running the export script.** Executing the `.mel` or `.py` is refused. A `.mel`
   is not executed: no MEL evaluator is decided (the ADR defers MEL to a later slice and a fresh decision). A `.py` could run only in the sandbox, but the sandbox may
   call just six typed calls (`ls`, `select`, `select_clear`, `rename`, `xform`,
   `undo_info_query`; `src/scripting/protocol.rs:105-130`, `:210-241`), none of which creates a
   camera, sets a film back or keys a focal length. A 3DEqualizer script would fail on the first
   unsupported call, and widening the call set to fit it is a boundary change that needs its own
   decision, not an import feature.
4. **Option 2 of #464 stays outside this repository**: a script the user runs inside 3DEqualizer
   (which ships a Python API, per the issue) that writes an interchange file or an
   AztecCraft-native file. AztecCraft only reads the output.

## Which 3DEqualizer outputs are data-only

Only what the issue text states is asserted. Everything else is unverified.

| Output | Class | Basis |
| --- | --- | --- |
| Maya export as `.mel` | Script | #464: 3DEqualizer hands its solve to Maya "as an executable `.mel` or `.py` script" |
| Maya export as `.py` | Script | same sentence |
| A 3DEqualizer-side export script that writes a file | Script, but it runs in 3DEqualizer, not here | #464 option 2 |
| Alembic export from 3DEqualizer | Data | #464 option 3 names it as an available route; camera fidelity in it is unproven (#463) |
| USD export from 3DEqualizer | Data | #464 option 3; same caveat |
| FBX export from 3DEqualizer | Unverified | #464 does not mention FBX; FBX is an AztecCraft interchange target under #23, not a stated 3DEqualizer output |
| 3DEqualizer's native project file, per-point track files, lens-distortion export files | Unverified | not described in #464; whether a given file is plain numbers or executable has not been checked against 3DEqualizer documentation |
| Plain numeric camera, point and lens-distortion data | Data by definition | the rule of this record: a file qualifies only if every field is a number or a name and nothing in it is evaluated |

Whether 3DEqualizer's Maya export is the full generated script the issue describes, and which
attributes it writes, is the audit item in #464's scope (public documentation or a synthetic
sample). This record does not claim a list.

## What AztecCraft can read today

Nothing in this chain exists yet. A search of `src/` for FBX and Alembic finds no reader, and
`known-issues.md:1630-1631` states "No USD import, `.usdc` or `.usdz`" (the USD code is export
only, `src/usd.rs`). The interchange camera work (#23, #438, #463), the camera film back (#458)
and locator transforms (#313) are the prerequisites. So this decision is "decided, not yet
implemented", and the shadow test of #464 (import the same solve through the chosen route, confirm
zero drift against reference camera values) cannot run until a camera-capable reader lands.

## Consequences

- No new execution surface: nothing here adds a call to the scripting set, an interpreter or a
  parser for script text.
- A user whose pipeline emits only the 3DEqualizer Maya script must re-export through Alembic or
  USD, or run the exporter inside 3DEqualizer. If that proves unacceptable, route 2 is the next
  step and needs the audit first.
- Typed refusals, not silent rounding, when a format cannot carry a value (#463), including a
  3DEqualizer value that has no field in the target schema.
- Synthetic fixtures only; no 3DEqualizer binary or customer data is needed to test the readers.

## Open items

- Audit the attributes of 3DEqualizer's Maya export from public documentation or a synthetic
  sample (#464 scope, first checkbox).
- Confirm the FBX and lens-distortion export facts marked unverified above.
- Owner may override the route order.
