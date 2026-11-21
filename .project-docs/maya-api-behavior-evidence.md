# Maya API behavior evidence — bounded Maya 2027.3 pass

Status: first empirical behavior slice; not a complete API audit, UI check, or
Maya 2026 compatibility/parity claim. Checked on 2026-11-21 against one
installed Windows Maya 2027.3 batch process.

## Probe and environment

The opt-in probe is [`scripts/maya-api-behavior-probe.py`](../scripts/maya-api-behavior-probe.py)
(SHA-256 `E5E6003DE77D483F6E54A672F634AC7E5D298F1FB7665E545A4A5B2D48CC7B56`).
It creates a unique `MAYA_APP_DIR` for each run below the repository's ignored
`target/maya-catalog/behavior-probe-appdir-<id>` before importing Maya, refuses
an already-imported Maya runtime and a named/modified scene, and creates only
synthetic in-memory objects. It does not save scenes or load/unload plug-ins.
The per-run directories are retained locally and are not shared between runs.
The source is a reusable audit affordance, not an AztecCraft feature.

Two independent runs each used a unique scratch app directory, ran in the
installed `mayapy`, and exited `0`. The parsed JSON contents of
`target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-1.json` and
`target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-2.json` matched.
The first ignored capture alone has SHA-256
`584EFB14BBA3E42F33EBFFC61BD262946A6D3E7BAD0C18EFA18D172593B47E9F`. Focused
assertions confirmed the version, single- and multi-edge topology, short-edge
length and unchanged geometry, `shotLabel` defaults, and separate API 1.0/API
2.0 wrapper results. Maya reported:

For reproduction, run the script twice in separate processes and capture
stdout, for example in PowerShell:

```powershell
& 'C:\Program Files\Autodesk\Maya2027\bin\mayapy.exe' scripts/maya-api-behavior-probe.py |
  Set-Content target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-1.json -Encoding utf8
if ($LASTEXITCODE -ne 0) { throw "Maya probe failed: $LASTEXITCODE" }
& 'C:\Program Files\Autodesk\Maya2027\bin\mayapy.exe' scripts/maya-api-behavior-probe.py |
  Set-Content target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-2.json -Encoding utf8
if ($LASTEXITCODE -ne 0) { throw "Maya probe failed: $LASTEXITCODE" }
```

Each process creates a distinct app directory automatically. The probe's
directory names are intentionally not emitted in the JSON; the reports capture
Maya/runtime data only.

| Field | Observed value |
| --- | --- |
| Installed version | `Autodesk MAYA 2027.3` |
| API version | `20270300` |
| Cut identifier | `202611131425-e0ccee1081` |
| Platform / machine | `win32` / `AMD64` |
| Python | `3.13.9` |
| Linear unit | `cm` |
| Loaded plug-in names | `cacheEvaluator`, `renderSetup` |

The JSON reports remain local and ignored at
`target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-1.json` and
`target/maya-catalog/maya-api-behavior-probe-node-editor-toggle-2.json`. This is one Maya
build and one batch environment; it does not establish GUI behavior, unloaded
plug-in behavior, other platforms, ABI compatibility, or Maya 2026 behavior.

## Smart Bevel observations

Autodesk's [Maya 2027.3 What's New](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-WhatsNew/files/GUID-E30563A9-74A3-4E4B-A955-CDDD25A562FD.htm)
says Smart Bevel removes extremely short edges before beveling and describes
other output improvements. The [command reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/CommandsPython/polySmartBevel.html)
does not specify a numerical short-edge threshold, bevel width/segments, or an
exact expected output mesh. The probe therefore records measured inputs and
outputs without asserting a general algorithm or threshold.

Both cases created explicit 2×2×2 cubes with one subdivision per axis and
construction history enabled, then selected edge `.e[0]` and called
`polySmartBevel()`:

| Case | Input edge / topology | Result |
| --- | --- | --- |
| Ordinary edge | Length `2.0` object units (`cm`); `8` vertices, `12` edges, `6` faces | Returned a `polySmartBevel` node without a reported exception; topology became `10` vertices, `15` edges, `7` faces. The recorded output coordinates show the mesh changed. |
| Short edge | One endpoint was moved so the selected edge measured `1.0013580322265625e-05` object units; `8` vertices, `12` edges, `6` faces | Returned a `polySmartBevel` node without a reported exception; topology remained `8/12/6`, and all recorded vertex coordinates were unchanged. The input edit is represented by a `polyTweak` history node. |
| One explicit edge (control) | History-free cube; explicit component `.e[0]`; endpoint IDs `[0,1]`; `8/12/6` | Returned a node; topology became `10/15/7`. |
| Two explicit edges (treatment) | Fresh history-free cube; explicit components `.e[0]` and `.e[6]`; endpoint IDs `[0,1]` and `[2,4]`; `8/12/6` | Returned a node; topology became `12/18/8`. This observed topology delta is larger than the one-edge control; no fixed per-edge topology contract is inferred. |

The short-edge outcome is a single empirical observation consistent with
Autodesk's summary, not proof of the fix's full behavior or its cutoff. Multiple
edge configurations, cutback amounts, straight-edge preservation, visual
output, and other topology/selection cases remain untested. The explicit
two-edge experiment observes one cube/edge pair only; it is not a general
per-edge algorithm or parity claim. This standalone test also does not establish
where or how the artist-facing tool is exposed in Maya's UI.

## `shotLabel` and Python API observation

Autodesk's [`shotLabel` node page](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/shotLabel.html)
documents `color` as float3 `(0,0,0)`, the three float color children as `0`,
and `name` as string with a null default. A created `shotLabel` in Maya 2027.3
reported the same current values and defaults. `attributeQuery` reported all
five attributes as readable, writable, storable and connectable.

For `name`, Maya's generic `attributeQuery(attributeType=True)` result is
`typed`, while `getAttr(type=True)` reports the stored data type as `string`;
the current value and listed default are both `None`. These are different
queries, not a disagreement with the node page.

The C++ [`MFnAttribute` reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_attribute.html)
marks `isNodeEditorPrimary` and `setNodeEditorPrimary` as introduced in 2027.3
and says attributes are not Node Editor primary by default. On a synthetic
`shotLabel.color` attribute, Maya's Python API 1.0
(`maya.OpenMaya.MFnAttribute`) exposed both method names on the class and
instance; calling the getter returned `false`. The setter was not called.

The Python API 2.0
[`MFnAttribute` reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/class_open_maya_1_1_m_fn_attribute.html)
documents a `nodeEditorPrimary` property. Maya 2027.3 exposed that property on
the class and instance, and its value on `shotLabel.color` was `false`; the
method names `isNodeEditorPrimary` and `setNodeEditorPrimary` themselves were
not exposed on that wrapper. These are API-specific wrapper observations on
one Maya build and one attribute; no setter invocation, compiled C++ harness,
ABI check, or claim about every member or release is included.

### Synthetic dynamic-attribute toggle

Autodesk's C++ [`MFnAttribute` reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/cpp_ref/class_m_fn_attribute.html)
marks `isNodeEditorPrimary()` and `setNodeEditorPrimary(bool)` as introduced in
2027.3, states attributes are not Node Editor primary by default, and describes
marked attributes as visible by default when the Node Editor's “Show Primary
Attributes” option is enabled. The Python API 2.0
[`MFnAttribute` reference](https://help.autodesk.com/cloudhelp/2027/ENU/MAYA-API-REF/py_ref/class_open_maya_1_1_m_fn_attribute.html)
documents `nodeEditorPrimary` as a property.

The guarded probe creates a temporary `network` node and one named dynamic
`double` attribute in the blank in-memory scene (the name is unique within that
scene). It observes the
initial flag with API 1.0 and API 2.0, sets true through API 1.0, reads true
through both APIs, then sets false through the API 2.0 property and reads false
through both APIs. It invokes no setter on a built-in or class attribute, saves
no scene, and deletes only its synthetic node in `finally`.

Two independent Maya 2027.3 batch runs completed the full sequence: initial
state `false`, cross-API `true` after the API 1.0 setter, then cross-API
`false` after the API 2.0 property setter. Both setter calls returned Python
`None` without a reported exception; both cross-API reads showed the requested
state. Cleanup attempted to reset through both available wrappers, reported no
cleanup exceptions, deleted the synthetic node, and confirmed it was absent.
It does not re-read the flag after reset attempts before deleting the node.
The probe gates each step on the previous result and records incomplete steps
and all cleanup errors. These observations establish only flag mutability and
cross-wrapper observation on one Maya build and one dynamic attribute; they do
not establish actual Node Editor visibility, filtering, or UI behavior, nor
Maya 2026 behavior.

## `traxAudioTrack` empty-track batch observation

The [2027 command reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Commands/traxAudioTrack.html)
documents insertion of an empty track at a 1-based index, removal of an empty
track, and a `numTracks` query. The [audio node reference](https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Tech-Docs/Nodes/audio.html)
says audio nodes carry sound-file information and are created with the `sound`
command. The probe therefore avoids fabricated audio nodes, sound-file inputs,
and undocumented audio-clip setup; it exercises only empty-track operations.

The opt-in [`traxAudioTrack` probe](../scripts/maya-api-trax-track-probe.py)
(SHA-256 `913F85C90089A76E1B4B027C11F709F757C036B678EAF3B8296613BE79627D89`)
ran twice in Maya 2027.3 standalone, each time with a unique ignored
`MAYA_APP_DIR` and a blank unnamed scene. Both runs exited `0`; their parsed
JSON results matched. The ignored local capture
`target/maya-catalog/trax-empty-track-final-1.json` has SHA-256
`FCD6B79A97F68760B5997CF37EEA3F8D2811578A26FB70C3C697AF9A657E4DCF`.
Local runtime reports are not distributed.

| Operation | Batch observation |
| --- | --- |
| Initial `numTracks` query | `0` |
| `insertTrack=1` | Returned `None` without a reported exception; subsequent count remained `0` |
| `removeTrack=1` | Returned `None` without a reported exception; final count remained `0` |

No audio node or clip existed in either scene. This records what the installed
batch runtime did, not proof that the API is absent or that an empty track
cannot be created in a GUI/editor workflow. The command reference describes
tracks as related to audio clips displayed in Trax; the tested standalone
process does not exercise a Trax editor. Lock/mute/solo, track assignment,
playback, audio-node creation, and artist-facing GUI workflows remain
untested. Maya 2026 remains unverified.

## Evidence boundaries and next work

- Runtime command execution, node defaults, and API wrapper exposure are
  empirical observations, separate from Autodesk's documented semantics.
- The exact Maya 2026 acceptance build remains unavailable. Nothing in this
  pass establishes Maya 2026 compatibility.
- No user scenes, reference assets, or Maya GUI were used.
- Next Maya-dependent candidates: test audio-node track state only if an
  authorized synthetic audio input can be established; GUI-only Camera
  Sequencer and artist-facing placement still require a separately
  authorized/available Maya GUI evidence path.

See the [versioned leaf audit](maya-api-leaf-audit.md), [audit plan](maya-api-audit-plan.md),
and [runtime catalog inventory](maya-2027-api-inventory.md) for the broader
source and version boundaries.
