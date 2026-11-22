"""Opt-in, read-only Maya runtime catalog probe.

Run with an installed Maya ``mayapy`` executable (replace ``<stamp>`` with a
timestamp):

    mayapy scripts/maya-api-runtime-probe.py > target/maya-catalog/runtime-<stamp>.json

This captures one initialized standalone process and its currently loaded
plugins. It does not enumerate unloaded plugins and does not establish ABI
compatibility or command/node behavior.

API snapshots list module-level exported names only; they do not cover class
members, behavior, or ABI.
"""

import json
import platform
import sys


def _scalars(value):
    """Normalize JSON-safe scalar/list/None results and sort their values."""
    if value is None:
        values = []
    elif isinstance(value, (list, tuple, set)):
        values = list(value)
    else:
        values = [value]
    values = [
        item
        for item in values
        if isinstance(item, (str, int, float, bool)) or item is None
    ]

    def sort_key(item):
        if item is None:
            return (4, "")
        if isinstance(item, bool):
            return (0, int(item))
        if isinstance(item, (int, float)):
            return (1, item)
        return (2, item)

    return sorted(values, key=sort_key)


def _strings(value):
    """Normalize name results, preserving only strings."""
    return sorted(item for item in _scalars(value) if isinstance(item, str))


def _query(call):
    try:
        return call()
    except Exception as error:
        return {"error": type(error).__name__}


def _maya_field(cmds, name):
    return _query(lambda: cmds.about(**{name: True}))


def _plugin_field(cmds, plugin, flag):
    result = _query(lambda: cmds.pluginInfo(plugin, query=True, **{flag: True}))
    if isinstance(result, dict):
        return result
    return _scalars(result)


def _api_module_snapshot(module_name):
    """Import one specified Maya API module and capture its public names."""
    try:
        module = __import__(module_name, fromlist=["*"])
    except Exception as error:
        return {
            "module": module_name,
            "import_status": "error",
            "error_type": type(error).__name__,
            "exported_names": [],
            "count": 0,
        }

    names = sorted(name for name in dir(module) if not name.startswith("_"))
    return {
        "module": module_name,
        "import_status": "ok",
        "error_type": None,
        "exported_names": names,
        "count": len(names),
    }


def main():
    import maya.standalone

    initialized = False
    try:
        maya.standalone.initialize(name="python")
        initialized = True
        import maya.cmds as cmds

        about_fields = (
            "version",
            "installedVersion",
            "apiVersion",
            "date",
            "cutIdentifier",
            "buildVariant",
            "batch",
        )
        about = {field: _maya_field(cmds, field) for field in about_fields}

        commands = _query(lambda: cmds.help("*", list=True))
        node_types = _query(lambda: cmds.ls(nodeTypes=True))
        if not isinstance(commands, dict):
            commands = _strings(commands)
        if not isinstance(node_types, dict):
            node_types = _strings(node_types)

        plugins_result = _query(lambda: cmds.pluginInfo(query=True, listPlugins=True))
        plugins = []
        if isinstance(plugins_result, dict):
            plugins = plugins_result
        else:
            for plugin in _strings(plugins_result):
                record = {"name": plugin}
                for flag in (
                    "version",
                    "vendor",
                    "apiVersion",
                    "command",
                    "constraintCommand",
                    "controlCommand",
                    "modelEditorCommand",
                    "dependNode",
                    "dependNodeId",
                    "translator",
                ):
                    record[flag] = _plugin_field(cmds, plugin, flag)
                plugins.append(record)

        api_modules = [
            _api_module_snapshot(module_name)
            for module_name in (
                "maya.OpenMaya",
                "maya.api.OpenMaya",
                "maya.api.OpenMayaAnim",
                "maya.api.OpenMayaRender",
                "maya.api.OpenMayaUI",
            )
        ]

        result = {
            "schema_version": 1,
            "capture_mode": "mayapy-standalone",
            "environment": {
                "python": sys.version,
                "sys.platform": sys.platform,
                "machine": platform.machine(),
            },
            "maya": about,
            "commands": commands,
            "node_types": node_types,
            "plugins": plugins,
            "api_modules": api_modules,
        }
        print(json.dumps(result, sort_keys=True))
    finally:
        if initialized:
            maya.standalone.uninitialize()


if __name__ == "__main__":
    main()
