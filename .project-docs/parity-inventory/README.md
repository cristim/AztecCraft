# Maya parity inventory (framework)

Status: framework only. No family is expanded, so this inventory contains no rows, no evidence and no figure. Tracking issue: cristim/AztecCraft#6 (every slice says "Refs #6"; only the freeze slice closes it).

This inventory replaces the provisional [workflow seed](../inventory-seed.md) as the candidate denominator once all 14 family files are expanded and frozen. It never says AztecCraft matches Maya. Exact parity needs comparison against the pinned reference and remains an open acceptance requirement (see [parity.md](../parity.md)).

## Documentation pin

- Denominator source: the Maya 2027.2 online documentation ([Maya 2027 Help](https://help.autodesk.com/view/MAYAUL/2027/ENU/)), the pin recorded in [reference.md](../reference.md). No Maya installation is used; the maintainer's 2026-10-20 decision that the official online Maya documentation is the source of truth (recorded on #5).
- The wording "all Maya 2026 functionality" in #6 maps to this pin. Rows that exist only in the 2026 documentation are tagged `2026-only`; rows that the 2027 notes add are tagged `2027-new`. No claim is made that the 2027.2 set contains or equals the 2026 set.
- A row whose 2027 page could not be read cites a 2026 page and carries the tag `2026 FALLBACK`. Details the documentation does not state are tagged `PROVISIONAL`.
- Component versions (Arnold, Bifrost, MayaUSD, LookdevX, MASH, XGen and so on) are taken from the 2027 documentation, never from an install.

## Status vocabulary

Status describes documented behaviour only, never equivalence to Maya. The order is Missing < Visible-unsupported < Implemented-untested < Partial < Doc-conformant, and only Doc-conformant scores.

| Status | Meaning |
| --- | --- |
| Missing | Nothing is drawn or callable. The Evidence cell says what was checked. |
| Visible-unsupported | Visible but inert: a disabled or placeholder control is drawn and reports that the operation is unsupported. |
| Implemented-untested | Fully implemented, with no test evidence. |
| Partial | Partly implemented (tested or not), or fully implemented with only some dimensions evidenced. |
| Doc-conformant | The behaviour the cited page describes is fully implemented. The Evidence cell names every dimension (interaction, semantic output, persistence, undo, invalid input, interop) and gives for each either its evidence or `n/a: <reason>`. A dimension left unnamed counts as not checked, which makes the row Partial. A single happy-path test does not qualify. |
| Out-of-scope | Excluded with a recorded reason (see the list below). |

Rows needing a running Maya (numerics, pixels, binary acceptance by Maya) are tagged `runtime-only`. They stay in the denominator at 0 until a reference run exists (#5).

### Mapping between the status vocabularies

| UI inventory ([ui-parity-inventory.md](../ui-parity-inventory.md)) | Family rows (this inventory) |
| --- | --- |
| Supported | Implemented-untested, Partial or Doc-conformant, decided per row by completeness (for example the prototype-text Help menu is Partial) |
| Partial | Partial |
| Visible-unsupported | Visible-unsupported |
| Missing | Missing |
| not used | Out-of-scope |

The application policy constants table proposed in PR #294 (development.md once merged) uses the labels verified-by-docs, provisional and differs-from-docs. They classify evidence for constants and never map to row statuses.

## Row format

| Column | Content |
| --- | --- |
| ID | `<PREFIX>-NNN`, stable, never reused or renumbered. A retired row is marked `retired`. |
| Workflow | One user task in imperative form with a short, testable action and assertion. One command with option-box variants is one row; command families are grouped, never one row per command. |
| Doc | 2027 documentation URL, with `2026 FALLBACK` or `2026-only` / `2027-new` tags where they apply. |
| Component | Maya core, or the named component with its pinned version. |
| Status | One value from the vocabulary above, plus the tags `runtime-only` where they apply. |
| Evidence | Anchored `path "symbol"` code cite, test name and the dimensions checked. Missing rows say what was checked. |
| MCP | MCP tool name, or `none`. Feeds the MCP coverage inventory; does not define coverage. |
| Owner / out-of-scope | An open issue number, or `OOS: <reason>`. |

UI element rows stay in ui-parity-inventory.md; family rows link to them by anchor and do not copy them.

## Evidence record

Each family file carries a header with the fields below, filled when it is expanded. This README records the shared rules.

- Documentation pin: Maya 2027.2 help, with the fetch date per file.
- Repository SHA at which the Evidence column was checked.
- Test platform or CI job for each cited test.
- No Maya build, GPU, locale or OCIO is involved (documentation-only). AztecCraft test evidence records its platform or CI job (above).
- Fixtures: the repository's synthetic fixtures, by path and hash where a row cites one. None for Missing rows.
- Scope: the documented behaviour of the cited page. Nothing is generalized to other versions, options or Maya runtime behaviour.

## Measurement rule (PROPOSED, documentation-conformance)

This rule is PROPOSED and needs the user's approval, including the budgets. It measures documentation conformance. It is not a parity measure.

- Each family has a fixed budget; the budgets sum to 100 and are set at the freeze. A family's budget is split equally among its rows. Adding or splitting rows never changes a budget.
- A row scores its share only when its status is Doc-conformant. Implemented-untested, Partial, Visible-unsupported and Missing score 0.
- Out-of-scope and retired rows leave that family's divisor and are counted separately, so moving a row to Out-of-scope visibly changes the denominator.
- `runtime-only` rows stay in the divisor at 0 and are also counted separately, so the ceiling is visible.
- Report together: row counts per status, the Out-of-scope and runtime-only counts, and per-family results. No family may vanish in an average.
- A family with no in-scope rows contributes 0 to both numerator and denominator, its budget is reported as unallocated, and it is listed separately.
- No percentage is computed until all 14 files are expanded and the budgets are approved at the freeze.
- No figure from this inventory may be quoted as Maya parity or Maya coverage. It does not count toward the 80-90% target or toward publication eligibility; those need reference comparisons ([inventory-seed.md](../inventory-seed.md), "Measurement proposal").
- Deviation from inventory-seed.md: the seed gives credit only after reference comparisons. This inventory uses documentation as its truth, so its top status is documentation conformance, which the seed would not credit.

## Out-of-scope add-ons (pending maintainer confirmation)

Recorded as `OOS: third-party`, easy to revisit. The confirmation question is in the claim comment on issue #6.

- Golaem
- Flow services (Flow-connected cloud capabilities)
- Substance
- Unreal Live Link

## Families

Names and order match [parity.md](../parity.md). Owner issues are open feature, epic or verification issues; bug issues and #202 are not owners (#202 is a custom path tracer, not a Maya workflow). Each file lists its own owners.

| # | Family | File | ID prefix | Owner issues |
| --- | --- | --- | --- | --- |
| 1 | Application shell | [01-application-shell.md](01-application-shell.md) | SHL | #59, #60, #61, #63, #64, #65, #66, #67, #68, #69, #170, #178, #71 |
| 2 | Scene management | [02-scene-management.md](02-scene-management.md) | SCN | #42, #105, #156, #189 |
| 3 | Viewport | [03-viewport.md](03-viewport.md) | VPT | #29, #30, #32, #166, #167, #35, #36, #37, #38, #39, #40, #41, #48, #49, #292, #295 |
| 4 | Polygon modeling | [04-polygon-modeling.md](04-polygon-modeling.md) | MOD | #43, #44, #45, #46 |
| 5 | Curves and surfaces | [05-curves-and-surfaces.md](05-curves-and-surfaces.md) | CRV | #47 |
| 6 | UV editing | [06-uv-editing.md](06-uv-editing.md) | UVE | #53 |
| 7 | Animation | [07-animation.md](07-animation.md) | ANM | #52, #62 |
| 8 | Rigging | [08-rigging.md](08-rigging.md) | RIG | #54 |
| 9 | Materials and lighting | [09-materials-and-lighting.md](09-materials-and-lighting.md) | MAT | #51 |
| 10 | Rendering | [10-rendering.md](10-rendering.md) | RND | #55 |
| 11 | Interchange | [11-interchange.md](11-interchange.md) | INT | #9, #14, #15, #17, #18, #22, #23, #272 |
| 12 | Scripting and extensions | [12-scripting-and-extensions.md](12-scripting-and-extensions.md) | SCR | #57 |
| 13 | Dynamics and extended systems | [13-dynamics-and-extended-systems.md](13-dynamics-and-extended-systems.md) | DYN | #56 |
| 14 | Pipeline and robustness | [14-pipeline-and-robustness.md](14-pipeline-and-robustness.md) | PIP | #8, #21, #58 |
