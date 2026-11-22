use crate::{
    editor_operations::{NameScope, Operation, RemoteRequest},
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

fn on_by_default() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PrefixArgs {
    scene_token: String,
    /// Text prepended verbatim (no separator) to the name of every selected object and every
    /// transform below it. Must be non-empty and keep each name a valid identifier (letters,
    /// digits and underscore, not starting with a digit).
    prefix: String,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

/// Which names a search and replace edits.
#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Scope {
    /// The selected objects only (default).
    #[default]
    Selected,
    /// The selected objects and every transform below them.
    Hierarchy,
    /// Every object in the scene; needs no selection.
    All,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchReplaceArgs {
    scene_token: String,
    /// Plain text to look for (not a pattern); must be non-empty.
    search: String,
    /// Text that replaces every occurrence; may be empty, but each resulting name must stay valid.
    replace: String,
    /// Which names to edit (default `selected`).
    #[serde(default)]
    scope: Scope,
    /// Match upper and lower case exactly (default true).
    #[serde(default = "on_by_default")]
    match_case: bool,
    /// The `selection_generation` that `inspect_scene` reported. Used by the `selected` and
    /// `hierarchy` scopes: with several objects selected the value is required, and a value that
    /// no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

fn parse_prefix(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: PrefixArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::PrefixHierarchyNames {
            prefix: args.prefix,
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

fn parse_search_replace(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SearchReplaceArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SearchReplaceNames {
            scope: match args.scope {
                Scope::Selected => NameScope::Selected,
                Scope::Hierarchy => NameScope::Hierarchy,
                Scope::All => NameScope::All,
            },
            search: args.search,
            replace: args.replace,
            match_case: args.match_case,
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "prefix_hierarchy_names",
        description: "Modify > Prefix Hierarchy Names on the object selection, in one undo entry named Prefix Hierarchy Names: prefix is prepended verbatim (no separator) to the name of every selected object and of every transform below it, each node once (an instanced node is renamed once; its name is shared by all its paths). Derived shape names follow. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_name_edit for an empty prefix, invalid_name when a resulting name would not be a valid identifier (letters, digits and underscore, not starting with a digit), name_collision when a resulting name is already taken or two resulting names would be equal (clashes are refused, never made unique); a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit). A restricted node (one the scene cannot edit) among the targets, or anywhere in an ancestor or descendant chain of a target (every scope, so also all), refuses the whole edit as unsupported_edit. Derived shape names take part in the uniqueness check, independent of order; a resulting name or derived shape name over 4096 bytes is invalid_name. All or nothing, with the scene and history untouched on any refusal. Not idempotent: calling it twice prefixes twice. PROVISIONAL, not compared with Maya: its pages do not state the separator or clash handling",
        schema: schema::<PrefixArgs>,
        parse: parse_prefix,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "search_replace_names",
        description: "Modify > Search and Replace Names, in one undo entry named Search and Replace Names: every non-overlapping occurrence of search (plain text, not a pattern; case-sensitive unless match_case is false, which compares ASCII letters only) in the names of the scope's objects becomes replace. scope is selected (default), hierarchy (the selection and every transform below it) or all (every object; needs no selection). Names that do not contain search are kept. Pass expected_selection_generation from inspect_scene for the selected and hierarchy scopes; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection (not for scope all). Refused with invalid_name_edit for an empty search or when no name changes (nothing contains search, or replace leaves every name as it was), invalid_name when a resulting name would be empty or not a valid identifier, name_collision when a resulting name is already taken or two resulting names would be equal; a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit). A restricted node (one the scene cannot edit) among the targets, or anywhere in an ancestor or descendant chain of a target (every scope, so also all), refuses the whole edit as unsupported_edit. Derived shape names take part in the uniqueness check, independent of order; a resulting name or derived shape name over 4096 bytes is invalid_name. All or nothing, with the scene and history untouched on any refusal. Regular expressions are unsupported. PROVISIONAL, not compared with Maya: its pages do not state case sensitivity, pattern support, defaults or clash handling",
        schema: schema::<SearchReplaceArgs>,
        parse: parse_search_replace,
        idempotent: false,
        read_only: false,
    },
];
