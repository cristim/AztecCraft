# AztecCraft working rules

- Build the desktop application in Rust; preserve the user's Maya parity objective.
- Make small, independently reviewable local commits.
- The user has authorized early prototype publication and merge to `main`, superseding the original restriction on the first push. The 80–90% target across all Maya 2026 functionality and exact parity remain unmet acceptance requirements. Don't infer release/signing or other publication authority beyond the user's current request.
- The repository is public. Push each independently reviewed and tested atomic commit promptly to its feature branch under the per-repository push lock, then arm `ci-watch`. Do not wait for a complete feature or Maya parity before pushing. All work reaches `main` through the merge rule under "Collaborating on issues"; the orchestrator holds no separate merge authority.
- Never claim exact visual or functional parity without comparison against the pinned Maya reference.
- Consult `.project-docs/plan.md`, `.project-docs/reference.md`, and `known-issues.md` before changing code.
- Before planning a new feature, review the official Autodesk Maya documentation thoroughly for the Maya release pinned in `.project-docs/reference.md` (keep that pin on the latest release; note where a newer release differs rather than silently switching): how the feature behaves (defaults, options, hotkeys, edge cases) and where it is exposed in the UI (menus and menu sets, shelves, Tool Box, Status Line, panel menus, editors, marking menus, Channel Box and Attribute Editor). Record the cited pages, the expected behaviour and the UI placement in the plan (cite and summarise; do not copy documentation text or images), mark anything the docs do not state as PROVISIONAL, and size the slice from that research rather than from the current code alone.
- Use an independent reviewer for two plan/diff passes. Keep implementation and review ownership separate.
- Build working vertical slices; unfinished controls must be visibly unsupported.
- MCP coverage is mandatory for every existing and new user-facing editor capability, including during catch-up. Its implemented operations and relevant state must be available through MCP, with synchronized tool schemas, real Editor protocol tests and documentation in the same feature-completion gate. Do not advertise unimplemented operations or add dummy tools.
- Native UI and MCP must use the same typed operations and authoritative Editor/NativeScene state, preserving human-input, draft, modal, close, dirty-scene and scene-token guards. Coordinate protected editor boundaries and adapter review with the MCP owner.
- Maintain an explicit MCP coverage inventory of actual UI and core capabilities, including files, import/export, modeling components, selection, camera and settings. Record supported, unsupported and missing coverage truthfully; catch-up remains open until an audited end-to-end inventory proves coverage, then keep it synchronized as features change.
- Keep reference assets, scene files, and personal paths out of commits unless explicitly intended for distribution.

## Adapted Craft app guidance

These rules adapt the sources below to AztecCraft. Existing project rules above
remain authoritative; sibling commands, crate layouts and tooling are not implied.

### Native architecture and scene fidelity

- Keep the desktop product in native Rust. Do not replace the editor with a
  webview shell. Keep core scene operations independent of UI widgets and make
  the UI and automation adapters dispatch the same typed editor operations.
- Actual Maya ASCII `.ma` and Maya Binary `.mb` Open, Save and Save As are the
  primary persistence objective. JSON compatibility during migration does not
  satisfy it. Preserve readability of existing JSON scenes; never rename or wrap
  JSON and describe it as native Maya support.
- Keep one authoritative `NativeScene` and source binding. Rendering, picking,
  UI and MCP must consume it through reviewed adapters, not maintain competing
  mutable scene models. Derived display geometry is not a lossless save model.
- Preserve native precision, hierarchy, connections, unknown data and untouched
  source values. Track explicit edit intent rather than reconstructing native
  values from rounded display fields. If preservation is unproven, reject the
  affected edit or write with a clear unsupported error.
- Bind replacement/save decisions to the exact bytes and identity parsed at
  Open. Preserve dirty-scene and source-change guards. Imported scene scripts,
  expressions and bundled MEL are inert data, never something to execute to
  make an unsupported scene load.

### Errors preserve work

- Treat files, MCP parameters, settings and input-derived indices as untrusted.
  Return typed, actionable errors; do not add input-reachable `unwrap`,
  `expect`, panic, `todo!` or `unimplemented!` paths. Unfinished operations
  report unsupported status.
- Validate bounds, finite numbers, arithmetic and allocation sizes before use;
  use checked access and arithmetic where input can exceed the valid range.
  Bound recursive or cyclic traversal. Reject invalid edits before mutation.
- Failed core operations, imports and unsupported operations preserve their
  applicable baseline. Native actions retain the existing `commit_pending`
  policy: they may first commit a valid human draft. Remote operations reject
  differing drafts and preserve the original editor state. Do not promise a
  rollback that the real editor protocol does not provide.
- Keep writes staged, protect occupied destinations and preserve originals on
  failure. A post-install durability failure is an uncertain outcome requiring
  reconciliation, not evidence that the scene is clean or authority to reverse
  the write blindly.

### Automation is part of the feature

- A user-facing feature is complete only when its implemented operation and
  relevant authoritative state are available through MCP, with synchronized
  schemas, actual Editor protocol tests and coverage documentation. Test native
  and remote callers against the same guards, including drafts, modal state,
  close handling and scene tokens.
- Keep automation opt-in. Do not start or register a server, or introduce a
  network listener, by default. Never expose tokens or private scene payloads
  through logs, error messages or derived `Debug` output.

### References and distributed assets

- Use public documentation, specifications, permitted source audits and
  authorized black-box observation as evidence. Private user-supplied scenes
  and reference assets remain valid local evidence; do not commit or publish
  them. Do not execute code embedded in fixtures.
- Prefer small synthetic fixtures for distributed regression tests. Keep
  private corpora and machine-specific manifests outside commits. Record
  reference version, fixture identity and the exact scope of each comparison.
- Distributed icons, fonts, images, samples and screenshots need original or
  verifiably licensed content and retained required notices. Record their
  source, pinned version, author and license. Do not copy proprietary product
  assets or sibling trademarks. Public vendor documentation and screenshots may
  be consulted as references; consultation does not grant redistribution rights.

### Verification and coordination

- Use the actual commands in [.project-docs/development.md](.project-docs/development.md).
  Verify core behavior, real Editor/MCP dispatch and the native interface at the
  scope changed. Look at UI results; a headless test is not native visual proof.
- For bug fixes, demonstrate a regression test fails before the fix and passes
  afterward. Exercise malformed, empty, boundary and failure cases. Keep build,
  platform, fixture and input-method boundaries explicit in evidence.
- Separate implemented coverage from reference parity. A wired menu, passing
  generic test or successful parse does not establish Maya behavior, complete
  file fidelity or correct display of a supplied scene.
- Edit only assigned files and coordinate shared Editor/NativeScene boundaries
  before changes. Use the orchestrator-assigned build target and repository
  build lock. Mutation probes use an isolated scratch target, never a retained
  shared target. Re-read resolved and automatically merged hunks after rebasing.
- Preserve tracked, untracked and ignored state. Sibling cleanup conventions do
  not authorize deleting assets, caches, logs, worktrees or build directories.

### Sources inspected

Adapted from pinned public guidance, without importing sibling-only tooling:

- [PhotoCraft AGENTS.md](https://github.com/storytold/photocraft/blob/7bb10a77591c14bc426155594b904e2b45ab5159/AGENTS.md):
  native Rust, error/input safety, shared commands, thin UI, visual verification,
  native-format completeness and parallel ownership.
- [PdfCraft AGENTS.md](https://github.com/storytold/pdfcraft/blob/9931329b9fae0b05373b4dc3ac7c58541f0891a4/AGENTS.md)
  and [CLAUDE.md](https://github.com/storytold/pdfcraft/blob/9931329b9fae0b05373b4dc3ac7c58541f0891a4/CLAUDE.md):
  asset provenance, reference privacy, opt-in automation, error handling,
  evidence boundaries and review after rebasing.
- [Artcraft AGENTS.md](https://github.com/storytold/artcraft/blob/d4e4ac69bbd93d237eaf5d0260bc6a5ff3a5e373/AGENTS.md):
  preservation of untracked and ignored local files.

## Collaborating on issues

The two-pass independent review above is the pre-push local gate; the two-approval rule below is the PR gate.

1. **Claim.** Before starting, comment on the issue ("Claiming: <agent/session name>, plan: <one line>"). Agents share one GitHub account, so every claim, review and approval comment states the agent/session name on its first line. The claim comment is the claim of record; the `in-progress` label is an optional convenience. A claim holds for 48 hours from the claimant's last renewal. Only the claimant's commits, pushes, PR updates or issue/PR comments renew it; others' reviews and comments do not. If claims overlap, the earliest comment by GitHub timestamp holds and the later claimant withdraws with a comment.
2. **Work in the open.** Branch `type/<issue-number>-short-slug`. Open a draft PR early with `Closes #<n>` and push each reviewed atomic commit to it.
3. **Stale claims.** If the claimant has done none of the renewing actions for more than 48 hours and the PR is still draft (or no PR exists), anyone may take over by commenting "Resuming from <previous claimant's agent/session name>, last activity <timestamp>, last commit <sha>" and starting from that commit. The first valid resume comment becomes the new claim and starts a new 48 hours. Push to the existing branch only with access; otherwise use a successor branch containing all prior commits and a new draft PR referencing the old one, and leave the old PR open until the maintainer closes it. Never force-push or rewrite prior commits.
4. **Peer review first.** At session start, before claiming new work, review open PRs: non-draft PRs lacking two current-SHA approvals, oldest first; draft PRs only when the author asks. Post the review against the current head SHA. Requested changes block until that reviewer's later `APPROVED <current SHA>` or the maintainer clears them.
5. **Approvals.** An approval is a review or comment `APPROVED <full head SHA> by <session-name>` whose session name differs from every claimant and committer session on the PR; two approvals need two different session names. Any push or rebase invalidates approvals. Maintainer sign-off counts only from the human maintainer: a GitHub approval where possible, or a `SIGN-OFF <SHA>` comment the maintainer writes personally. Agents never post `SIGN-OFF`, even when asked by another agent.
6. **Merge rule.** A PR merges only when it is out of draft, CI is green, two independent approvals and the maintainer sign-off cover the same head SHA. The maintainer or the maintainer's explicit delegate merges, with the method chosen by the maintainer. Authors never merge their own PR.
7. Everything posted to issues and PRs is public: no home-directory paths, OS or cloud usernames (GitHub @handles are fine), emails, tokens or private scene and asset names.
