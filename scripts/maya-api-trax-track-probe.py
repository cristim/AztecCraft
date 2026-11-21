"""Opt-in Maya Trax track API probe; no scene files are written.

Run in a separate batch process with Maya's mayapy. The probe uses an unnamed,
unmodified blank scene and creates a unique repository-local ignored MAYA_APP_DIR
under target/maya-catalog/. It does not reset or clear scenes, save or open files,
or load or unload plugins. Only an empty track is inserted and removed.
"""

import json
import os
import sys
import uuid


def _capture(call):
    """Capture a documented API call without traversing or stringifying its result."""
    try:
        return {"value": call(), "exception_type": None}
    except Exception as error:
        return {"value": None, "exception_type": type(error).__name__}


def main():
    if any(name == "maya" or name.startswith("maya.") for name in sys.modules):
        raise RuntimeError("maya_already_imported")
    app_dir = os.path.abspath(
        os.path.join(
            os.path.dirname(__file__),
            "..",
            "target",
            "maya-catalog",
            "trax-track-appdir-" + uuid.uuid4().hex,
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

        batch = _capture(lambda: cmds.about(batch=True))
        if batch["exception_type"] or batch["value"] is not True:
            raise RuntimeError("batch_standalone_required")
        scene_name = _capture(lambda: cmds.file(query=True, sceneName=True))
        modified = _capture(lambda: cmds.file(query=True, modified=True))
        if (
            scene_name["exception_type"]
            or modified["exception_type"]
            or scene_name["value"]
            or modified["value"]
        ):
            raise RuntimeError("empty_unnamed_scene_required")

        result = {
            "maya": {
                field: _capture(lambda field=field: cmds.about(**{field: True}))
                for field in ("installedVersion", "apiVersion", "cutIdentifier")
            },
            "batch": batch,
            "steps": {},
        }
        initial = _capture(lambda: cmds.traxAudioTrack(query=True, numTracks=True))
        result["steps"]["initial_count"] = initial
        inserted = False
        if initial["exception_type"] is None:
            insertion = _capture(lambda: cmds.traxAudioTrack(insertTrack=1))
            result["steps"]["insert_track_1"] = insertion
            inserted = insertion["exception_type"] is None

        post_insert = None
        if inserted:
            post_insert = _capture(
                lambda: cmds.traxAudioTrack(query=True, numTracks=True)
            )
            result["steps"]["post_insert_count"] = post_insert
            if post_insert["exception_type"] is None:
                result["steps"]["remove_track_1"] = _capture(
                    lambda: cmds.traxAudioTrack(removeTrack=1)
                )

        result["steps"]["final_count"] = _capture(
            lambda: cmds.traxAudioTrack(query=True, numTracks=True)
        )
        print(json.dumps(result, sort_keys=True))
    finally:
        if initialized:
            maya.standalone.uninitialize()


if __name__ == "__main__":
    main()
