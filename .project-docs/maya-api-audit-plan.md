# Maya API audit plan

Status: in progress under Issue #6. This is a source-backed inventory and
runtime-evidence effort, not an implementation or parity claim.

## Reference boundary

- Current documentation/runtime reference: Maya 2027.3. Autodesk's 2027.3
  What's New page, release notes and C++ API changelog were checked on
  2026-11-20. The installed Maya executable and `mayapy.exe` report file version
  `27.3.0.2355`; a bounded standalone runtime `about` and catalog observation
  was also captured (see [the API inventory](maya-2027-api-inventory.md)).
- Acceptance remains Maya 2026. The official 2026.3 update release notes exist,
  but no 2026.3 C++ API changelog page was found (the expected endpoint returned
  404). The exact installed 2026 acceptance build remains unavailable and
  unverified. Do not infer 2026 compatibility from 2027.3.
- Existing inventories retain the release actually inspected in their evidence
  records. In particular, a Maya 2027.2 citation is not silently relabeled as
  2027.3; affected workflows must be checked against the 2027.3 update before
  the inventory is frozen.

## What the project will contain

1. **Source registry and audit procedure.** Keep the full API inventory in
   [maya-2027-api-inventory.md](maya-2027-api-inventory.md), with each source
   labelled as a mutable year-level catalog, a per-update changelog, a release
   note, a devkit artifact or a runtime observation.
2. **Local catalog snapshots.** Extend
   `scripts/maya-catalog-crawl.sh` to capture raw Autodesk year-page responses,
   extracted name lists and a manifest in ignored `target/maya-catalog/`
   snapshots. Record retrieval time, exact URL, source class, raw/extracted
    SHA-256, counts and per-page structural validation status. Never commit raw
    pages or bulk symbol lists. A count tolerance is only a structural
    tripwire, not proof of completeness. A failed crawl emits an error and
    removes its staging directory; it does not publish a failed snapshot or
    preserve a failure manifest.
3. **Release-specific API deltas.** Inventory Autodesk's C++ changelog
   entries for 2026.0/.1/.2 and 2027.0/.1/.2/.3 separately. The 2027.3 page
   identifies `MFnAttribute::isNodeEditorPrimary`,
   `MFnAttribute::setNodeEditorPrimary` and `MPxTexture2dEvaluator` as
   introduced in 2027.3. Commands, nodes and Python catalogs currently expose
   year-level endpoints only; compare those as dated snapshots, not as
   update-specific changes. Label update provenance unknown when no dated
   source establishes it. Do not treat absence from a catalog as an API
   removal.
4. **Runtime evidence from the installed build.** Add an opt-in, read-only Maya
   probe that records `about` version/build/API/environment facts, command
   names via `help("*", list=True)`, registered node types via
   `ls(nodeTypes=True)`, and currently loaded plug-in registrations via
   `pluginInfo`. The first opt-in probe is implemented in
   `scripts/maya-api-runtime-probe.py` and captures the observed process
   metadata and registrations. Query node attributes only for explicitly
   scoped types using documented facilities such as `attributeInfo`. Do not
   open user scenes, load/unload plug-ins, change preferences, or write outside
   ignored target output. Record that the results describe one build, process
   and loaded plug-in state; they cannot enumerate unloaded plug-ins or
   establish entitlement, ABI compatibility or behavior.
5. **Surface-by-surface detail audit.** Expand the name lists into documented
   semantics for MEL/`maya.cmds` (flags, modes, defaults, results), dependency
   nodes (attributes, types, defaults and constraints), C++ (classes,
   namespaces, modules, functions, members and signatures), Python API 2.0,
   and Python API 1.0 as its own surface. Do not infer Python API 1.0 coverage
   from API 2.0. Keep installed runtime presence, documented semantics,
   behavioral tests and ABI compatibility as separate evidence.
6. **Integrate new capabilities into parity work.** Cross-link relevant
   commands and nodes to the 14 workflow families. Add user-facing Maya 2027.3
   workflows to the correct family with versioned citations and truthful
   AztecCraft/MCP status; API symbol counts are never the workflow denominator.
   Record 2027.3 release-note features outside the core API separately, with
   component version and entitlement boundaries.

## Evidence and completion rules

Each snapshot or claim records:

- surface and symbol/workflow;
- exact source URL, locale, release/update label, retrieval time and source
  category (versioned update page or mutable year-level page);
- page/artifact identity, extraction tool version, raw/extracted hash, count,
  and successful structural-validation status; failed crawls are not retained
  as snapshots;
- documented semantics and the exact leaf-page review scope;
- separate status for documentation coverage, runtime enumeration,
  behavioral testing and ABI testing;
- for runtime evidence: exact Maya product/API/build version, platform,
  entitlement, loaded plug-ins, probe command, inputs and result identity;
- explicit exclusions, provisional details and unverified differences.

Declare a catalog surface complete only after every declared source page and
extraction path has been accounted for, failures and exclusions are visible,
and an independent reviewer checks the resulting manifest and representative
entries. A complete 2027.3 catalog does not establish Maya 2026 parity. Issue
#6 remains open until the workflow inventory is complete and frozen.

## Initial version sources

- [Maya 2027.3 What's New](https://help.autodesk.com/view/MAYAUL/2027/ENU/?guid=GUID-E30563A9-74A3-4E4B-A955-CDDD25A562FD)
- [Maya 2027.3 Release Notes](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2027_3_HTML.html)
- [Maya 2027.3 C++ API changes](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_3.html)
- [Maya 2026.3 Update Release Notes](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2026_3_RELEASE_NOTES_HTML.html)
- Runtime probe references: [about](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/about.html),
  [help](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/help.html),
  [ls](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/ls.html),
  [pluginInfo](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/pluginInfo.html),
  [Python standalone](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-DEVHELP/files/Maya-Python-API/Maya_DEVHELP_Maya_Python_API_PythonOutsideMayaSession_html.html).
- Year-level commands, nodes and Python/C++ catalogs and update-specific C++
  changelogs are registered in
  [maya-2027-api-inventory.md](maya-2027-api-inventory.md).
