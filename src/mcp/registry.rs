use super::features::{
    animation, extrude, file, history, inspect, manipulation, material, node_edit, selection,
    status_line, view,
};
use crate::editor_operations::{MAX_NAME_BYTES, RemoteRequest};
use rmcp::model::{ErrorData, JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

/// One advertised tool: its catalogue entry and the decoder for its arguments.
pub(super) struct ToolSpec {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) schema: fn() -> JsonObject,
    pub(super) parse: fn(JsonObject) -> Result<RemoteRequest, ErrorData>,
    /// Repeating the call with the same arguments returns `unchanged`.
    pub(super) idempotent: bool,
    pub(super) read_only: bool,
}

/// Catalogue order is the order of this list, then each feature's own order.
const FEATURES: &[&[ToolSpec]] = &[
    inspect::TOOLS,
    manipulation::TOOLS,
    selection::TOOLS,
    node_edit::TOOLS,
    extrude::TOOLS,
    animation::TOOLS,
    material::TOOLS,
    history::TOOLS,
    view::TOOLS,
    status_line::TOOLS,
    file::TOOLS,
];

pub(super) fn specs() -> impl Iterator<Item = &'static ToolSpec> {
    FEATURES.iter().flat_map(|tools| tools.iter())
}

#[cfg(test)]
pub(crate) fn tool_names() -> impl Iterator<Item = &'static str> {
    specs().map(|spec| spec.name)
}

pub(super) fn catalog() -> Vec<Tool> {
    specs()
        .map(|spec| {
            Tool::new(spec.name, spec.description, (spec.schema)()).with_annotations(
                ToolAnnotations::new()
                    .read_only(spec.read_only)
                    .idempotent(spec.idempotent)
                    .open_world(false),
            )
        })
        .collect()
}

pub(super) fn parse_request(name: &str, arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    if arguments
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.len() > MAX_NAME_BYTES)
    {
        return Err(ErrorData::invalid_params(
            "name exceeds 4096 UTF-8 bytes",
            None,
        ));
    }
    match specs().find(|spec| spec.name == name) {
        Some(spec) => (spec.parse)(arguments),
        None => Err(ErrorData::new(
            rmcp::model::ErrorCode::METHOD_NOT_FOUND,
            "Unknown AztecCraft tool",
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.project-docs/mcp-tools.md");

    fn render() -> String {
        let mut doc = String::from(
            "# MCP tool catalogue\n\nGenerated from the tool registry in `src/mcp/registry.rs`; do not edit by hand. \
Regenerate with `UPDATE_DOCS=1 cargo test --features mcp registry::tests::tool_doc`. \
Argument schemas are served by `tools/list`; see [mcp.md](mcp.md) for behaviour and limits.\n\n",
        );
        doc.push_str(&format!("{} tools.\n\n", specs().count()));
        doc.push_str("| Tool | Access | Idempotent | Description |\n|---|---|---|---|\n");
        for spec in specs() {
            doc.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                spec.name,
                if spec.read_only {
                    "read-only"
                } else {
                    "mutating"
                },
                if spec.idempotent { "yes" } else { "no" },
                spec.description.replace('|', "\\|"),
            ));
        }
        doc
    }

    #[test]
    fn tool_doc_matches_registry() {
        crate::app::snapshots::assert_text_file(
            std::path::Path::new(DOC_PATH),
            ".project-docs/mcp-tools.md",
            &render(),
            "UPDATE_DOCS",
        );
    }

    const INVENTORY_DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.project-docs/mcp.md");

    /// Rows of the `## Coverage inventory` table as six trimmed cells.
    fn inventory_rows(doc: &str) -> Vec<Vec<String>> {
        let doc = doc.replace("\r\n", "\n");
        let section = doc
            .split("\n## Coverage inventory\n")
            .nth(1)
            .expect("mcp.md has a Coverage inventory section");
        let section = section.split("\n## ").next().unwrap();
        let mut table = section.lines().filter(|line| line.starts_with('|'));
        let header = table.next().expect("inventory table header");
        assert_eq!(
            header,
            "| Area | Capability | Status | Tools | State | Issue |"
        );
        assert!(table.next().expect("separator").contains("---"));
        table
            .map(|line| {
                let cells = line
                    .trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().to_owned())
                    .collect::<Vec<_>>();
                assert_eq!(cells.len(), 6, "inventory row has six cells: {line}");
                cells
            })
            .collect()
    }

    /// Backticked names in a cell; anything else in the cell is a malformed row.
    fn backticked(cell: &str) -> Vec<String> {
        cell.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.strip_prefix('`')
                    .and_then(|part| part.strip_suffix('`'))
                    .unwrap_or_else(|| panic!("`{part}` is not a backticked name"))
                    .to_owned()
            })
            .collect()
    }

    /// Dotted paths of every object key in the committed `inspect_scene` snapshots, so State
    /// names are checked against the serialized projection instead of a second hand-kept list.
    fn inspection_paths() -> std::collections::BTreeSet<String> {
        fn walk(prefix: &str, value: &Value, out: &mut std::collections::BTreeSet<String>) {
            if let Value::Object(map) = value {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, child, out);
                    out.insert(path);
                }
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
        let mut paths = std::collections::BTreeSet::new();
        let mut files = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with("inspection_") && name.ends_with(".json") {
                let snapshot: Value = serde_json::from_slice(&std::fs::read(&path).unwrap())
                    .unwrap_or_else(|error| panic!("{name} is not JSON: {error}"));
                walk("", &snapshot["inspection"], &mut paths);
                files += 1;
            }
        }
        assert!(files > 0, "no inspection snapshots found");
        // Snapshots omit optional fields, so add the keys of a fully populated node, both as
        // `nodes.<key>` and bare (the inventory names node fields without the prefix).
        let node = crate::editor_operations::NodeInspection {
            id: String::new(),
            name: String::new(),
            transform_id: String::new(),
            visible: None,
            transform: crate::editor_operations::TransformInput {
                translation: [0.0; 3],
                rotation_degrees: [0.0; 3],
                scale: [1.0; 3],
            },
            authoring_transform: crate::editor_operations::AuthoringTransformInspection {
                translation: [0.0; 3],
                rotation: [0.0; 3],
                scale: [1.0; 3],
            },
            transform_attributes: Some(azteccraft::native_scene::TransformAttributes {
                rotate_order: "xyz",
                rotate_axis_degrees: [0.0; 3],
                shear: [0.0; 3],
                rotate_pivot_translate: [0.0; 3],
                scale_pivot_translate: [0.0; 3],
                non_default: vec!["shear"],
            }),
            instance_paths: Vec::new(),
            shape_ids: Vec::new(),
        };
        let mut node_paths = std::collections::BTreeSet::new();
        walk("", &serde_json::to_value(&node).unwrap(), &mut node_paths);
        for path in node_paths {
            paths.insert(format!("nodes.{path}"));
            paths.insert(path);
        }
        paths
    }

    #[test]
    fn create_primitive_description_states_the_face_budget() {
        let digits = azteccraft::primitive_options::MAX_FACES.to_string();
        let mut grouped = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let spec = specs().find(|s| s.name == "create_primitive").unwrap();
        assert!(
            spec.description.contains(&format!("{grouped}-face budget")),
            "description must state the {grouped}-face budget"
        );
    }

    #[test]
    fn inventory_tools_match_registry() {
        let inspection = inspection_paths();
        let doc = std::fs::read_to_string(INVENTORY_DOC_PATH).unwrap();
        let rows = inventory_rows(&doc);
        assert!(!rows.is_empty());
        let registered = tool_names()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        let mut documented = std::collections::BTreeSet::new();
        for row in &rows {
            let (capability, status, tools, state, issue) =
                (&row[1], row[2].as_str(), &row[3], &row[4], &row[5]);
            let tools = backticked(tools);
            let state = backticked(state);
            for field in &state {
                assert!(
                    inspection.contains(field),
                    "inventory row `{capability}` names State `{field}`, which no committed \
                     inspection snapshot serializes"
                );
            }
            for tool in &tools {
                assert!(
                    registered.contains(tool),
                    "inventory row `{capability}` names unregistered tool `{tool}`"
                );
                documented.insert(tool.clone());
            }
            match status {
                "supported" => assert!(
                    !tools.is_empty() || !state.is_empty(),
                    "supported row `{capability}` names no tool or state field"
                ),
                "unsupported" => assert!(
                    tools.is_empty() && issue.is_empty(),
                    "unsupported row `{capability}` must name no tool and no issue"
                ),
                "missing" => {
                    assert!(tools.is_empty(), "missing row `{capability}` names a tool");
                    let numbered = issue
                        .strip_prefix('#')
                        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                    assert!(
                        numbered || issue == "unfiled",
                        "missing row `{capability}` needs an issue like #123 or `unfiled`"
                    );
                }
                other => panic!("row `{capability}` has unknown status `{other}`"),
            }
        }
        let absent = registered.difference(&documented).collect::<Vec<_>>();
        assert!(
            absent.is_empty(),
            "registered tools missing from the mcp.md coverage inventory: {absent:?}"
        );
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names = tool_names().collect::<Vec<_>>();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
