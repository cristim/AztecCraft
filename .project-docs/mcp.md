# Native editor MCP

The optional MCP server controls the new native Editor window launched by its client. Tools use that window's scene, history, selection and camera. It cannot attach to an already running instance. This prototype exposes twelve tools; it does not establish Maya scripting, plug-in or full-functionality compatibility.

## Build and launch

Install the prerequisites in [development instructions](development.md), then build explicitly:

```sh
cargo build --release --locked --features mcp
```

Configure the client to launch the resulting binary with `--mcp-stdio`. A native graphical session is required. The default desktop build and default CI package omit MCP; rebuilding without `--features mcp` replaces the binary at the same target path. Use the exact feature-enabled artifact.

An example subprocess configuration, subject to the client's own configuration format:

```json
{
  "mcpServers": {
    "azteccraft": {
      "command": "<absolute-path-to-mcp-enabled-azteccraft>",
      "args": ["--mcp-stdio"]
    }
  }
}
```

On Windows the executable is `azteccraft.exe`. No socket, HTTP service or file-path tool is exposed. The local subprocess launcher is the authority boundary. Stdout is reserved for MCP messages; diagnostics use stderr.

Pinned rmcp 3.5.1 handles protocol decoding, discovery, initialization and request metadata. Official SDK tests exercise MCP 2026-07-28 Discover and legacy 2025-11-25 initialization. Cancellation tests exercise the legacy request-cancellation path. These tests do not establish compatibility with every MCP client.

## Tools and state

Call `inspect_scene` first. Every other tool requires `scene_token`, a UUID string from that inspection. IDs are unsigned 64-bit decimal strings, including values beyond JavaScript's exact integer range. JSON numbers are rejected for IDs. Unknown argument fields, invalid types and unknown tools are protocol errors.

| Tool | Arguments besides `scene_token` | Result |
| --- | --- | --- |
| `inspect_scene` | No arguments; no token required | Committed nodes, selected ID, history availability, dirty/draft/focus/modal flags and camera |
| `create_primitive` | `primitive`: `cube` or `plane`; optional `name` | Created `node_id`; selects the new node |
| `select_node` | Optional `node_id`; null or omission clears selection | Changed or unchanged selection |
| `rename_node` | `node_id`, `name` | Changed or unchanged authoring state |
| `set_transform` | `node_id`, `translation`, `rotation_degrees`, `scale`, each an array of three finite numbers | Changed or unchanged authoring state |
| `delete_node` | `node_id` | Changed authoring state |
| `undo` | None | Changed or unchanged history state |
| `redo` | None | Changed or unchanged history state |
| `frame_selected` | None | Changed or unchanged camera; requires selection |
| `orbit_camera` | `radians`: two finite numbers, yaw and pitch deltas | Changed or unchanged camera; pitch clamps near the poles |
| `pan_camera` | `world_units`: two finite numbers, camera right and up displacement in scene world units | Changed or unchanged camera target and eye |
| `dolly_camera` | `factor`: positive finite distance multiplier | Changed or unchanged camera distance; below one moves closer |

Transforms use the [existing coordinate conventions](development.md): scale, fixed-axis XYZ Euler degrees, then translation. Core validation rejects invalid names, nonfinite values and coordinate overflow. Node inspection includes its name, transform and mesh vertices/faces.

Successful tool results provide `structuredContent` and equivalent text content. The structured result has `outcome`: `scene`, `created`, `changed` or `unchanged`. A scene result nests fields under `inspection`; a created result includes `node_id`. Authoring mutations share native undo history. Selection and camera operations do not create authoring undo entries or mark the document dirty. Navigation arguments use camera units, not pointer pixels. Invalid resulting camera ranges return `camera_error` without changing the camera; finite JSON numbers outside the camera's f32 range are also rejected by camera validation.

Example tool arguments after inspection:

```json
{
  "scene_token": "<scene-token-from-inspection>",
  "primitive": "cube",
  "name": "remote_cube"
}
```

Each new Editor and each successful native New/Open replaces the scene token. Save, normal edits, undo and redo retain it. The token identifies a document generation, not an edit revision or credential. Reinspect after a stale token, deleted ID, cancellation, timeout or lost acknowledgement; do not automatically retry a mutation.

## Human edits, errors and limits

Remote mutations return `busy_editing` while a native text field is focused, a differing draft exists, or a modal dialog is open. This includes valid uncommitted drafts. They do not commit unrelated human edits. Inspection remains available and reports these states without returning draft text. Native inputs are processed before queued remote requests at the end of each UI frame, and remote changes request repaint.

Hidden/minimized windows dispatch through eframe's UI-thread logic callback without painting or forcing foreground. Visible windows still dispatch only after that frame's native UI input. Interactive native input retained while hidden sets the inspection flag `native_input_pending` and blocks remote mutation until visible UI processes it. Harmless focus changes and unheld pointer motion do not block idle hidden requests. Native Close while input is pending is canceled; restore/process the input and retry Close.

Tool failures set `isError` and return `error.code` plus `error.message`. Codes include `busy_editing`, `stale_scene`, `too_large`, `no_selection`, `invalid_input`, `scene_error`, `camera_error`, `busy`, `unavailable` and `cancelled`. `busy` covers queue saturation and the five-second UI reply deadline. The SDK may suppress a cancelled response. Cancellation before application skips a queued request; cancellation after a synchronous operation starts cannot undo it. Inspect to reconcile the result.

Prototype bounds are 64 KiB per ingress JSON line, 4096 UTF-8 bytes per input name, at most 16 retained RPC slots and 16 pending supported notification callbacks, 16 queued editor requests, and eight requests dispatched per frame. RPC admission remains held until reply write or connection close; callback permits remain held until completion/drop or close, including reused request IDs. A cancelled request whose reply the SDK suppresses retains its RPC slot until connection close. Enough abandoned calls can exhaust admission and close the session. A new connection launches a new Editor window; it cannot attach to the old window.

Unsupported and repeated notifications are filtered before SDK dispatch. Oversized ingress, invalid JSON, fatal SDK decoding errors, duplicate outstanding IDs or exhausted protocol admission close the connection. SDK compatibility handling can skip nonstandard messages; the adapter consumes those ignored lines without stalling later buffered requests.

Tool results are limited to 512 KiB and framed replies to at most 1 MiB, including the terminating newline. Inspection preflights source size before cloning nodes, then bounds serialized content. Large scenes can return `too_large` rather than partial inspection. These bounds do not promise inspection of arbitrary large scenes.

The [known first-click text defect](../known-issues.md#first-click-text-input-can-be-lost) remains in pinned egui. Tests prove the bridge adds no input loss compared with the same native baseline and preserves text actually inserted in the following frame; they do not fix that defect.

## Shutdown and verification

Ingress EOF or protocol failure closes the bridge immediately, skips queued mutations and invokes the existing native close guard before SDK handler draining. A clean editor closes. Unsaved committed scenes, including valid drafts committed by native close handling, prompt the existing Save/Discard/Cancel confirmation without autosave. An invalid draft keeps the GUI open with a validation error; the user must fix or cancel that edit before Close/Save can reach confirmation. This deliberately departs from MCP's recommendation to exit promptly for a dirty document. A client can forcibly terminate its subprocess and lose unsaved work; universal shutdown compatibility is not claimed. Cancelling native close does not revive an EOF-closed protocol connection.

Approved native close cancels protocol work and uses a one-second bounded runtime shutdown rather than waiting indefinitely on silent stdin. The subprocess regression holds stdin open and checks bounded cleanup. Tests also cover actual Editor input/order/history, official SDK tool journeys, both lifecycles, cancellation, deadlines, floods, framing and stalled output. They use real Editor/egui state without an OS window. Native protocol/window verification and cross-platform graphical interaction remain separate evidence requirements.

EOF automatic close is deferred while native input remains unprocessed; its close request is retained until visible consumption. Native verification of the initial server reproduced background/minimized timeouts because its queue drained only in the UI callback. The revised actual `App::logic` path passes minimized/occluded-state tests, pending-input baseline comparisons and hidden close/cancellation tests. [Scoped macOS native verification](mcp-native-verification.md) confirmed minimized requests after the correction, while a later idle deadline and recovery leave broader responsiveness unverified.

Run the official client suite explicitly so it is not omitted by feature gating:

```sh
cargo test --locked --features mcp-test-client
cargo clippy --locked --all-targets --features mcp-test-client -- -D warnings
cargo clippy --locked --all-targets --features mcp -- -D warnings
cargo build --locked --features mcp
```

No-default core builds remain independent of desktop and MCP dependencies. See [build checks](builds.md) for CI and the exact dependency license references.

## Capability synchronization

Every existing and new user-facing capability and its relevant state must be accessible through MCP, including while catch-up is in progress. Each applicable feature slice includes its typed operation, MCP schema/handler and inspection changes, real Editor protocol tests and documentation before the completed feature gate. The feature owner coordinates protected editor boundaries and adapter review with the MCP owner. Do not advertise a tool before the operation exists or leave MCP support as a permanent later backlog.

The current catalog omits component selection/extrusion, file actions, GUI Export, materials, animation, simulation and scripting. Native face selection/extrusion support must include its MCP capability updates in that same completed feature slice. Core-only APIs and unsupported controls do not imply an advertised MCP tool.

Nine verified tools establish the initial server foundation, not catch-up with every existing operation. Existing capabilities without tools need reviewed follow-up slices; their absence does not defer MCP support for new applicable features, including the native face slice.

| Existing capability | Current MCP exposure |
| --- | --- |
| Object create/select/rename/transform/delete, history and framing | Nine tools listed above, including inspection |
| Native New/Open/Save As | No tool; document lifecycle, human-edit guards and file bounds need a dedicated slice |
| Core OBJ export | No tool; file authority and create-new/no-clobber behavior need a dedicated slice |
| Core mesh extrusion | No tool; native face selection/extrusion, component inspection and protocol support belong in the completed native face slice |
| Interactive camera orbit/pan/dolly | Three navigation tools use typed editor operations in MCP builds; live camera state is inspectable. Native pointer gestures still call the camera directly until the shared native producer integration is verified |

Initial catch-up remains incomplete while these implemented capabilities lack their reviewed MCP support. Full Maya parity remains a separate unmet objective.

## Protocol references

- [MCP stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)
- [MCP tool results](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [Pinned official Rust SDK](https://docs.rs/rmcp/3.5.1/rmcp/)
