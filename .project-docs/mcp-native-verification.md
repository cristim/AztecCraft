# Scoped native MCP verification

The macOS packaged-debug application was exercised through its actual stdio
connection and native window. These observations cover the initial nine-tool
foundation, not full MCP capability catch-up, other platforms or Maya parity.

## Hidden-window correction

At source `36dffdf`, executable SHA256
`a28d6d7a24e0108f26ecc1a5a25cc6a89c4b653cbb4c3b7cb0a9cdb7558d013a`,
native testing exercised the nine-tool journey, minimized-window requests,
human-draft rejection and EOF close confirmation. Cancel kept the dirty window
available; Discard of the synthetic test scene exited successfully. Independent
verification checked the retained screenshots and protocol logs.

This corrects the earlier `95150af` minimized inspection deadline caused by
dispatch existing only at visible UI end. Actual trait-callback regressions also
cover minimized and occluded state, pending native input, cancellation and close.
The native result does not establish every idle or background condition.

## Combined icons and MCP build

Source `541c72c` rebases the reviewed MCP foundation onto the quad-mesh,
extrusion and colorful-icon work. Its executable SHA256 was
`a19166f94b38fe47e7507b51422b86695be8d856a81b70c4511f6beae5d75091`.
The native window showed the colorful tool symbols. Protocol inspection and
selection clearing followed by selecting a node changed the same live Editor's
selection; this was an actual mutation rather than a selected-node no-op.

After a long idle interval, request 8 reached the UI reply deadline. A main-thread
sample taken during investigation included Metal drawable acquisition. Neither
overlap with request 8 nor a five-second acquisition wait was established. Request 9 subsequently
succeeded in the same process with the same scene token. The underlying cause
remains unproven; the sample does not establish a Metal defect or unconditional
idle/background responsiveness. Further native window and menu investigation is
pending.

The combined source passed 99 MCP/client/core tests, 66 default desktop/core
tests and 50 headless tests, plus formatting, strict lint and builds. These checks
do not replace native coverage. macOS application Quit interception, Windows/Linux
native behavior and complete document/menu journeys remain unverified. This
record makes no Maya ASCII/Binary load, display or save claim.

## Linux X11 visible idle and fully covered window (#13)

Ran the Linux x86_64 debug build at `2d65489c1fb3a595e9cd658555bcbfa813f2ae06` with `--features mcp` and opt-in `--mcp-stdio` on Omarchy 4.5.0. Used one MCP session and an isolated 2560 x 1800 Xvfb display. The editor window was 1440 x 900. A second 1700 x 1050 native window at the same origin covered it and took focus. Verified X11 geometry and stacking order.

Sent five `inspect_scene` calls per state after idle gaps of 0, 1, 5, 15 and 30 seconds. Measured request-to-reply with a monotonic clock. The visible window replied in 5.6 to 35.4 ms and the covered window in 1.8 to 13.0 ms. All ten replies were successful and below the 5-second editor deadline. This covers an X11 covered window and idle gaps up to 30 seconds. The macOS drawable-acquisition timeout remains unresolved.
