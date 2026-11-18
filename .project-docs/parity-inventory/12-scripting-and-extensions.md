# Scripting and extensions

Status: not expanded; the three rows below are the #57 slice-1 starting rows (T0) and are no denominator. No evidence yet. ID prefix `SCR`. Framework and rules: [README.md](README.md).

Owner issues (open feature, epic or verification issues): #57

| ID | Workflow | Doc | Component | Status | Evidence | MCP | Owner / out-of-scope |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SCR-001 | Run a Python statement from the Script Editor and see its result echoed | [Script Editor, 2024 page](https://help.autodesk.com/cloudhelp/2024/ENU/Maya-Scripting/files/GUID-B66020CC-1BBE-4EA1-B1DE-A288B044DD80.htm), `PROVISIONAL` (no 2027 page read) | Maya core | Missing | Checked 2026-11-15: no Script Editor window and no interpreter in the repository. Design: [scripting-adr.md](../scripting-adr.md) (bundled CPython in WASM in a helper process, human-submitted or human-approved only, `scripting` feature off by default) | none | #57 |
| SCR-002 | Call `maya.cmds` commands from Python | [Using Python, 2026](https://help.autodesk.com/cloudhelp/2026/ENU/Maya-Scripting/files/GUID-55B63946-CDC9-42E5-9B6E-45EE45CFC7FC.htm), `2026 FALLBACK` | Maya core | Missing | Checked 2026-11-15: no `maya.cmds` module. Slice 1 would cover only `ls`, `select`, `rename`, `xform` and `undoInfo` query; every other command raises `NotImplementedError` | none (planned: `run_script` with per-run native approval, default off) | #57 |
| SCR-003 | Run a MEL statement from the Script Editor or Command Line | [Command line, 2024 page](https://help.autodesk.com/cloudhelp/2024/ENU/Maya-Basics/files/GUID-4981A0A5-9418-4274-918C-561E33B0102D.htm), `PROVISIONAL` | Maya core | Missing | Checked 2026-11-15: no MEL evaluator; the Command Line is a disabled row (#63). MEL is slice 2 | none | #57, #63 |
