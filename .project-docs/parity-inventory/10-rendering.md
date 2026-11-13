# Rendering

Status: not expanded, no evidence. ID prefix `RND`. Framework and rules: [README.md](README.md).

Owner issues (open feature, epic or verification issues): #55

| ID | Workflow | Doc | Component | Status | Evidence | MCP | Owner / out-of-scope |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RND-001 | Render the scene to an image file with a path tracer | none read (PROVISIONAL: Maya render documentation not yet reviewed for this row) | core | Missing | `src/render/integrator.rs "render"` and `src/render/image.rs "Image::save_ppm"` are a library-only prototype (PR #356) with unit tests of ray hits, shadow occlusion, size limits and PPM bytes. No menu, shelf, panel or Render Settings control calls them. interaction: n/a, no UI entry point. semantic output: fixed point light and sky, diffuse only, no BVH, PROVISIONAL, no Maya or Arnold parity claimed. persistence: PPM only through `craft_atomic::write_new`. undo: n/a. invalid input: typed errors for size, sample and depth limits. interop: n/a | none | #55 |
