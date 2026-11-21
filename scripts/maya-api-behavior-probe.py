"""Opt-in synthetic Maya behavior probe (no scene files are written).

Run in a separate batch process with Maya's mayapy:
    mayapy scripts/maya-api-behavior-probe.py
The probe creates a unique repository-local ignored MAYA_APP_DIR under
target/maya-catalog/ before importing Maya. Scenes are synthetic, in memory,
and never saved; an initial named or modified scene is refused.
"""

import json
import itertools
import math
import os
import platform
import re
import sys
import uuid


def _safe(value, depth=0):
    """Normalize only bounded scalar/array results; never stringify Maya objects."""
    if depth > 8:
        return {"normalization": "depth_limit"}
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else {"normalization": "non_finite_float"}
    if isinstance(value, dict):
        entries = itertools.islice(value.items(), 10000)
        return {
            key: _safe(item, depth + 1)
            for key, item in entries
            if isinstance(key, str)
        }
    if isinstance(value, (list, tuple)):
        return [_safe(item, depth + 1) for item in value[:10000]]
    try:
        return [_safe(item, depth + 1) for item in itertools.islice(iter(value), 10000)]
    except Exception:
        return {"normalization": "unsupported_type"}


def _failure(call):
    try:
        return {"value": _safe(call()), "exception_type": None}
    except Exception as error:
        return {"value": None, "exception_type": type(error).__name__}


def _bevel_scenario(cmds, short_edge):
    cube = cmds.polyCube(
        width=2.0,
        height=2.0,
        depth=2.0,
        subdivisionsX=1,
        subdivisionsY=1,
        subdivisionsZ=1,
        constructionHistory=True,
    )[0]
    mesh = cmds.listRelatives(cube, shapes=True, fullPath=True)[0]
    edge = mesh + ".e[0]"
    edge_info = cmds.polyInfo(edge, edgeToVertex=True)[0]
    match = re.search(r":\s*(\d+)\s+(\d+)(?:\s|$)", edge_info)
    if match is None:
        raise ValueError("edge_endpoint_parse_failed")
    endpoint_ids = [int(match.group(1)), int(match.group(2))]
    if short_edge:
        # Collapse one endpoint onto the other, then offset it by 1e-5 in object X.
        # Vertex IDs come from Maya's own edge-to-vertex query for edge .e[0].
        first = cmds.pointPosition("{}.vtx[{}]".format(mesh, endpoint_ids[0]), local=True)
        second_component = "{}.vtx[{}]".format(mesh, endpoint_ids[1])
        cmds.xform(second_component, objectSpace=True, translation=first)
        cmds.move(1e-5, 0, 0, second_component, relative=True, objectSpace=True)
    endpoints = [
        cmds.pointPosition("{}.vtx[{}]".format(mesh, vertex_id), local=True)
        for vertex_id in endpoint_ids
    ]
    edge_length = sum((endpoints[0][axis] - endpoints[1][axis]) ** 2 for axis in range(3)) ** 0.5
    input_counts = {
        "vertices": len(cmds.ls(mesh + ".vtx[*]", flatten=True) or []),
        "edges": len(cmds.ls(mesh + ".e[*]", flatten=True) or []),
        "faces": len(cmds.ls(mesh + ".f[*]", flatten=True) or []),
    }
    vertex_components = cmds.ls(mesh + ".vtx[*]", flatten=True) or []
    pre_vertex_coordinates = {
        "object_units": [
            _safe(cmds.pointPosition(component, local=True))
            for component in vertex_components
        ],
        "world_units": [
            _safe(cmds.pointPosition(component, world=True))
            for component in vertex_components
        ],
    }
    cmds.select(edge, replace=True)
    selected_before = cmds.ls(selection=True, flatten=True) or []
    result = _failure(lambda: cmds.polySmartBevel())
    node = result["value"]
    if isinstance(node, list):
        node = node[0] if node else None
    output_counts = {
        "vertices": len(cmds.ls(mesh + ".vtx[*]", flatten=True) or []),
        "edges": len(cmds.ls(mesh + ".e[*]", flatten=True) or []),
        "faces": len(cmds.ls(mesh + ".f[*]", flatten=True) or []),
    }
    history = cmds.listHistory(mesh) or []
    coordinates = []
    for component in cmds.ls(mesh + ".vtx[*]", flatten=True) or []:
        coordinates.append(_safe(cmds.pointPosition(component, world=True)))
    return {
        "input_topology": input_counts,
        "output_topology": output_counts,
        "selected_component_before": selected_before,
        "explicitly_selected_edge": edge,
        "edge_endpoint_vertex_ids": endpoint_ids,
        "edge_endpoint_object_coordinates": _safe(endpoints),
        "edge_length_object_units": edge_length,
        "pre_bevel_vertex_coordinates": pre_vertex_coordinates,
        "selected_component_after": cmds.ls(selection=True, flatten=True) or [],
        "returned_node": node,
        "history": history,
        "post_bevel_coordinate_mode": "world_units",
        "vertex_coordinates": coordinates,
        "exception_type": result["exception_type"],
    }


def _shot_label(cmds):
    node = cmds.createNode("shotLabel", name="aztecBehaviorProbeShotLabel")
    attributes = {}
    for name in ("color", "colorR", "colorG", "colorB", "name"):
        plug = node + "." + name
        query = {}
        query["declared_type"] = _failure(
            lambda name=name: cmds.attributeQuery(name, node=node, attributeType=True)
        )
        query["current_data_type"] = _failure(lambda plug=plug: cmds.getAttr(plug, type=True))
        query["current_value"] = _failure(lambda plug=plug: cmds.getAttr(plug))
        query["default"] = _failure(
            lambda name=name: cmds.attributeQuery(name, node=node, listDefault=True)
        )
        for label in ("readable", "writable", "storable", "connectable"):
            query[label] = _failure(
                lambda name=name, label=label: cmds.attributeQuery(name, node=node, **{label: True})
            )
        attributes[name] = query
    api_result = _failure(lambda: _attribute_api(node))
    return {"node": node, "attributes": attributes, "api_attribute": api_result}


def _explicit_smart_bevel_scenario(cmds):
    """Compare one explicit edge with two explicit edges on fresh, history-free cubes."""
    cases = {}
    for name, edge_ids in (("control", (0,)), ("treatment", (0, 6))):
        cube = cmds.polyCube(
            width=2.0,
            height=2.0,
            depth=2.0,
            subdivisionsX=1,
            subdivisionsY=1,
            subdivisionsZ=1,
            constructionHistory=False,
        )[0]
        shape = cmds.listRelatives(cube, shapes=True, fullPath=True)[0]
        components = ["{}.e[{}]".format(shape, edge_id) for edge_id in edge_ids]
        endpoints = {}
        for component in components:
            edge_info = cmds.polyInfo(component, edgeToVertex=True)[0]
            match = re.search(r":\s*(\d+)\s+(\d+)(?:\s|$)", edge_info)
            if match is None:
                raise ValueError("edge_endpoint_parse_failed")
            endpoints[component] = [int(match.group(1)), int(match.group(2))]
        before = {
            "vertices": len(cmds.ls(shape + ".vtx[*]", flatten=True) or []),
            "edges": len(cmds.ls(shape + ".e[*]", flatten=True) or []),
            "faces": len(cmds.ls(shape + ".f[*]", flatten=True) or []),
        }
        outcome = _failure(lambda components=components: cmds.polySmartBevel(*components))
        node = outcome["value"]
        if isinstance(node, list):
            node = node[0] if node else None
        after = {
            "vertices": len(cmds.ls(shape + ".vtx[*]", flatten=True) or []),
            "edges": len(cmds.ls(shape + ".e[*]", flatten=True) or []),
            "faces": len(cmds.ls(shape + ".f[*]", flatten=True) or []),
        }
        cases[name] = {
            "explicit_components": components,
            "edge_endpoint_vertex_ids": endpoints,
            "pre_topology": before,
            "post_topology": after,
            "returned_node": node,
            "history": cmds.listHistory(shape) or [],
            "exception_type": outcome["exception_type"],
        }
    return cases


def _attribute_api(node):
    from maya.api import OpenMaya as om

    selection = om.MSelectionList()
    selection.add(node + ".color")
    plug = selection.getPlug(0)
    attribute = om.MFnAttribute(plug.attribute())
    return {
        "isNodeEditorPrimary_exposed": hasattr(attribute, "isNodeEditorPrimary"),
        "setNodeEditorPrimary_exposed": hasattr(attribute, "setNodeEditorPrimary"),
    }


def main():
    if any(name == "maya" or name.startswith("maya.") for name in sys.modules):
        raise RuntimeError("maya_already_imported")
    app_dir = os.path.abspath(
        os.path.join(
            os.path.dirname(__file__),
            "..",
            "target",
            "maya-catalog",
            "behavior-probe-appdir-" + uuid.uuid4().hex,
        )
    )
    os.makedirs(app_dir, exist_ok=False)
    os.environ["MAYA_APP_DIR"] = app_dir
    import maya.standalone

    initialized = False
    try:
        maya.standalone.initialize(name="python")
        initialized = True
        import maya.cmds as cmds

        if not cmds.about(batch=True):
            raise RuntimeError("batch_standalone_required")
        scene_name = cmds.file(query=True, sceneName=True)
        modified = cmds.file(query=True, modified=True)
        if scene_name or modified:
            raise RuntimeError("empty_unnamed_scene_required")
        result = {
            "runtime": {
                "python": sys.version,
                "platform": sys.platform,
                "machine": platform.machine(),
                "loaded_plugins": _failure(
                    lambda: sorted(cmds.pluginInfo(query=True, listPlugins=True) or [])
                )["value"],
            },
            "maya": {
                field: _failure(lambda field=field: cmds.about(**{field: True}))["value"]
                for field in ("installedVersion", "apiVersion", "cutIdentifier")
            },
            "linear_unit": _failure(lambda: cmds.currentUnit(query=True, linear=True))["value"],
            "scenarios": {},
        }
        for key, short in (("cube_edge", False), ("short_edge", True)):
            try:
                result["scenarios"][key] = _bevel_scenario(cmds, short)
            except Exception as error:
                result["scenarios"][key] = {"exception_type": type(error).__name__}
        try:
            result["scenarios"]["explicit_smart_bevel"] = _explicit_smart_bevel_scenario(cmds)
        except Exception as error:
            result["scenarios"]["explicit_smart_bevel"] = {"exception_type": type(error).__name__}
        try:
            result["scenarios"]["shotLabel"] = _shot_label(cmds)
        except Exception as error:
            result["scenarios"]["shotLabel"] = {"exception_type": type(error).__name__}
        print(json.dumps(result, sort_keys=True))
    finally:
        if initialized:
            maya.standalone.uninitialize()


if __name__ == "__main__":
    main()
