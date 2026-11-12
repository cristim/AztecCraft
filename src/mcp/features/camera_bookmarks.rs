use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, schema, scoped, token},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListBookmarksArgs {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BookmarkArgs {
    scene_token: String,
    /// Bookmark name: 1 to 64 characters, no control or invisible formatting characters, no
    /// leading or trailing whitespace. Names are unique and case-sensitive.
    name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenameBookmarkArgs {
    scene_token: String,
    name: String,
    new_name: String,
}

fn parse_list(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let _: ListBookmarksArgs = decode(arguments)?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::ListCameraBookmarks,
    })
}

fn bookmark(
    arguments: JsonObject,
    make: fn(String) -> Operation,
) -> Result<RemoteRequest, ErrorData> {
    let args: BookmarkArgs = decode(arguments)?;
    Ok(scoped(token(&args.scene_token)?, make(args.name)))
}

fn parse_create(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    bookmark(arguments, |name| Operation::CreateCameraBookmark { name })
}

fn parse_delete(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    bookmark(arguments, |name| Operation::DeleteCameraBookmark { name })
}

fn parse_apply(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    bookmark(arguments, |name| Operation::ApplyCameraBookmark { name })
}

fn parse_rename(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: RenameBookmarkArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::RenameCameraBookmark {
            name: args.name,
            new_name: args.new_name,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "list_camera_bookmarks",
        description: "List the session's camera bookmarks in creation order: name, target, distance, yaw_degrees and pitch_degrees in the units inspect_scene reports for the camera. Needs no scene_token. Bookmarks are session-only: they are not saved in the scene, are cleared when New or Open replaces the document and are lost on close",
        schema: schema::<ListBookmarksArgs>,
        parse: parse_list,
        idempotent: true,
        read_only: true,
    },
    ToolSpec {
        name: "create_camera_bookmark",
        description: "Save the live camera pose (target, distance, yaw, pitch; not the field of view or clip planes) as a named bookmark. A name already in use is refused with bookmark_exists, an invalid name with invalid_bookmark_name and more than 64 bookmarks with bookmark_limit. Session-only: not an undo step, does not edit or dirty the scene",
        schema: schema::<BookmarkArgs>,
        parse: parse_create,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "rename_camera_bookmark",
        description: "Rename a bookmark, keeping its pose and position in the list. An unknown name returns bookmark_not_found, a new_name already in use bookmark_exists; renaming to the same name returns unchanged. Session-only; does not edit the scene",
        schema: schema::<RenameBookmarkArgs>,
        parse: parse_rename,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "delete_camera_bookmark",
        description: "Delete a bookmark by name; an unknown name returns bookmark_not_found. Session-only: not an undo step, does not edit the scene",
        schema: schema::<BookmarkArgs>,
        parse: parse_delete,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "apply_camera_bookmark",
        description: "Set the live camera to a bookmark's saved pose exactly (target, distance, yaw, pitch); the field of view and clip planes are untouched. The change is immediate, with no animation. An unknown name returns bookmark_not_found. Session-only; does not edit the scene",
        schema: schema::<BookmarkArgs>,
        parse: parse_apply,
        idempotent: true,
        read_only: false,
    },
];
