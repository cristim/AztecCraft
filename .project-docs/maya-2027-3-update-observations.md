# Maya 2027.3 update observations

This is a source-backed candidate register for update-specific API and workflow
follow-up, not a complete feature inventory or Maya-parity claim. Autodesk
separates the What's New summaries from release-note fixes. A summary is only
the behavior stated there; it does not establish all options, defaults, UI
placement, installed component availability, or AztecCraft/MCP coverage.
Candidate family IDs do not create or score parity rows. Maya 2026 remains the
acceptance target; none of these Maya 2027.3 observations establishes 2026
compatibility.

Sources read on 2026-11-20:

- [What's New in Maya 2027.3](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-E30563A9-74A3-4E4B-A955-CDDD25A562FD.htm)
- [Maya 2027.3 Release Notes](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-ReleaseNotes/files/MAYA_RELEASENOTES_2027_3_HTML.html)
- [2027.3 C++ API changelog](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/changelog_2027_3.html)

## Workflow and component candidates

| Autodesk item / source label | Source-backed summary | Candidate family | Still unverified before a parity row |
| --- | --- | --- | --- |
| Autodesk Maya MCP server (Tech Preview), Maya | External MCP clients can read and change a Maya scene. | SCR; possibly SHL for setup | Maya-specific setup/UI entry point, actual tool list and schemas, guards, supported operations, protocol behavior and parity. Do not infer from the generic Autodesk Assistant MCP docs. |
| Autodesk Assistant updates, Maya | The page says Assistant helps users do work in Maya; it does not enumerate Maya-specific new operations. | SHL; possibly SCR | Maya-specific capabilities, entry point, privacy/operation boundaries and update delta. |
| Dynamic Geometry Attributes (DGA), Maya | `pointValues` enables DGA values in viewport shading; a Dynamic Geometry Attributes Options window can create the required graph. | MAT and/or RND | Node attributes/defaults, graph construction and failures, window controls, viewport/renderer scope, and whether one or both families own the workflow. Maya 2026.3 `dgaToArray` material is not evidence for this 2027.3 workflow. |
| USD for Maya 0.38.0, component | Summary describes improved inspection, debugging and organization of layered USD data and expanded component-authoring/material workflows. | INT/PIP; MAT for material workflows | Individual 0.38.0 workflows and UI surfaces. Do not re-label USD 0.37.0 / Maya 2027.2 features as 2027.3 changes. |
| LookdevX for Maya 2.3.0, component | Adds a Material Browser providing access to a material library. | MAT | Browser entry point, library formats/content, browse/apply behavior and component availability. |
| Bifrost 3.3.0.16, component | The summary reports a variety of new features and improvements but names no specific workflow. | DYN | Obtain the versioned Bifrost feature documentation before adding individual workflows. |
| Substance 3.1.1, component | Summary reports OpenPBR support and other fixes. | MAT | Maya integration, import/assignment behavior, UI and exact supported OpenPBR scope. |
| MtoA 5.6.4, component | Summary reports a faster/more responsive Accelerated Viewport, faster playback and motion blur in Arnold Viewport. | RND; ANM only if a distinct playback workflow is documented | Controls, requirements, renderer behavior and whether playback is a separate user workflow. |
| Color-space assignment rules, Maya | Regex may be used instead of glob syntax to specify rules assigning color spaces to input files. | MAT | Exact regex dialect, precedence/escaping, defaults, configuration UI and 2026 difference. |
| MotionMaker Deep Phase, Maya | Updated model uses additional phase information to improve generated character-motion quality; the existing generation workflow is unchanged. | ANM | Exact output/quality assertions, controls, supported inputs and 2026 comparison. |
| Smart Bevel, Maya | Removes extremely short edges before beveling; avoids cutback crossing its boundary as amount increases; processes all specified edges; keeps straight input edges from bulging or shifting. | MOD | Exact option-dependent cases, topology/output assertions, UI placement and Maya comparison. Avoid duplicating the existing Smart Bevel workflow row if one is later added. |

The source page presents these as part of What's New in Maya 2027.3. Component
versions are retained as stated; page presentation is not a substitute for
checking detailed component documentation or establishing Maya 2026 behavior.
Detailed feature pages were not reliably retrievable for several JS-rendered
sections during this audit. Those gaps remain open rather than being filled
from older year-level pages.

## Release-note fixes and known issue

These are update-specific fixes or a known issue, not automatically new
workflows. Consider them as regression assertions under an existing family
after checking the relevant leaf documentation and Maya 2026 behavior.

| Maya 2027.3 note | Candidate family | Follow-up |
| --- | --- | --- |
| UV Toolkit Set Texel Density uses current scene units (`MAYA-127760`) | MOD | Confirm the control/options and compare numerical behavior. |
| File Texture `neverIgnoreClamping=True` enforces selected Preview Quality resolution clamping for non-square textures, including UDIM workflows (`MAYA-144445`) | MAT/RND | Check attribute presence/default and viewport behavior on the pinned Maya 2026 target. |
| DGA `dgaSampler` UV Set changes do not automatically update UV linking; the documented workaround is manual UV linking (`MAYA-144888`, known issue) | MAT/RND | Keep as a known limitation; determine whether it affects any planned workflow and verify version scope. |
| OCIO regex rules receive full support (`MAYA-143775`) | MAT | See the workflow candidate above; inspect the detailed rule page before specifying syntax tests. |

The release notes contain additional animation, Outliner, GPU evaluation,
Foundation, modeling display, rendering and rigging fixes. This register is
not a complete regression-test transcription; each should be added only when
relevant to a documented AztecCraft workflow.

## API-only update evidence

The C++ changelog explicitly introduces `MFnAttribute::isNodeEditorPrimary`,
`MFnAttribute::setNodeEditorPrimary`, and `MPxTexture2dEvaluator` in 2027.3.
See the full update-by-update C++ list and limitations in
[Maya API catalog sources](maya-2027-api-inventory.md#c-changelogs), and
candidate family traceability in the
[non-scoring crosswalk](maya-api-workflow-crosswalk.md). These API additions do
not alone establish an artist-facing workflow or behavioral/ABI compatibility.

## Evidence boundary and next work

- Source summaries establish only what the cited Autodesk pages state. Exact
  leaf semantics, defaults, hotkeys, menus, shelves, menu sets, panel/editor
  surfaces and edge cases still require feature-specific research.
- Candidate families are provisional traceability, not assertions that the
  corresponding family inventory row exists.
- Keep Maya core, Maya USD, LookdevX, Bifrost, Substance and MtoA versions and
  availability distinct.
- Retrieve official detailed pages and compare release-specific behavior to
  Maya 2026 before freezing workflow rows. Record runtime presence,
  behavioral tests and ABI evidence independently.
