//! The scenarios the gate runs, and the ratchet that compares their violations with `gaps/`.
//! A scenario other than the default only reports nodes that its base frame does not contain.
//! Add a scenario by adding a line to `all`.

pub(super) mod panels;

use super::{Violation, gaps, rig::Rig, violations};
use crate::editor_operations::{ActiveTool, MenuSet, Operation};
use eframe::egui::{
    self,
    accesskit::{NodeId, TreeUpdate},
};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(super) struct Captured {
    /// Frame before the scenario's action; its nodes are not the scenario's to report.
    pub(super) base: Option<TreeUpdate>,
    pub(super) update: TreeUpdate,
    /// Area ids of the windows the scenario opens (their chrome is exempt, see `chrome_ids`).
    pub(super) windows: Vec<egui::Id>,
}

pub(super) struct Scenario {
    pub(super) name: String,
    pub(super) area: &'static str,
    pub(super) capture: Box<dyn Fn() -> Captured>,
}

impl Scenario {
    fn new(name: &str, area: &'static str, capture: impl Fn() -> Captured + 'static) -> Self {
        Scenario {
            name: name.to_owned(),
            area,
            capture: Box::new(capture),
        }
    }

    pub(super) fn violations(&self) -> Vec<Violation> {
        let captured = (self.capture)();
        let skip: HashSet<NodeId> = captured
            .base
            .iter()
            .flat_map(|base| base.nodes.iter().map(|(id, _)| *id))
            .collect();
        violations(&self.name, &captured.update, &captured.windows)
            .into_iter()
            .filter(|v| !skip.contains(&v.node))
            .collect()
    }
}

fn plain(rig: &Rig, base: Option<TreeUpdate>) -> Captured {
    Captured {
        base,
        update: rig.update(),
        windows: Vec::new(),
    }
}

pub(super) fn default_frame() -> TreeUpdate {
    Rig::new(false).update()
}

fn tool_scenario(name: &str, tool: ActiveTool) -> Scenario {
    Scenario::new(name, "shell", move || {
        let mut rig = Rig::new(true);
        rig.tool(tool);
        plain(&rig, Some(default_frame()))
    })
}

/// Opens `path` (menu button, then submenus by hover) on a cube scene and reports the new nodes.
fn menu_scenario(name: &str, set: MenuSet, path: &'static [&'static str]) -> Scenario {
    Scenario::new(name, "menus", move || {
        let mut rig = Rig::new(true);
        if set != MenuSet::Modeling {
            rig.editor.native_operation(Operation::SetMenuSet { set });
            rig.settle();
        }
        let base = rig.update();
        rig.click(path[0]);
        for label in &path[1..] {
            rig.hover(label);
        }
        rig.settle();
        plain(&rig, Some(base))
    })
}

pub(super) fn all() -> Vec<Scenario> {
    let mut all = vec![
        Scenario::new("default", "shell", || plain(&Rig::new(false), None)),
        tool_scenario("tool-move", ActiveTool::Move),
        tool_scenario("tool-rotate", ActiveTool::Rotate),
        tool_scenario("tool-scale", ActiveTool::Scale),
    ];
    let modeling = MenuSet::Modeling;
    for (name, set, path) in [
        ("menu-file", modeling, &["File"][..]),
        ("menu-edit", modeling, &["Edit"]),
        (
            "menu-create-polygon",
            modeling,
            &["Create", "Polygon Primitives"],
        ),
        (
            "menu-select-components",
            modeling,
            &["Select", "Components"],
        ),
        ("menu-view", modeling, &["View"]),
        ("menu-shading", modeling, &["Shading"]),
        ("menu-modify", modeling, &["Modify"]),
        ("menu-display", modeling, &["Display"]),
        ("menu-uv", modeling, &["UV"]),
        ("menu-skeleton", MenuSet::Rigging, &["Skeleton"]),
        ("menu-help", modeling, &["Help"]),
    ] {
        all.push(menu_scenario(name, set, path));
    }
    all.extend(panels::scenarios());
    all
}

/// Failure lines: new violations (fix the control, or add its key to the area file) and recorded
/// keys no longer violated (delete the line).
pub(super) fn ratchet_failures(
    scenarios: &[Scenario],
    recorded: &[(&str, &[&str])],
) -> Vec<String> {
    let mut found: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for scenario in scenarios {
        let area = found.entry(scenario.area).or_default();
        for violation in scenario.violations() {
            area.insert(violation.key(), violation.message());
        }
    }
    let mut failures = Vec::new();
    for (area, keys) in &found {
        let known: BTreeSet<&str> = recorded
            .iter()
            .filter(|(name, _)| name == area)
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect();
        for (key, message) in keys {
            if !known.contains(key.as_str()) {
                failures.push(format!("new in gaps/{area}.rs: {key:?}\n    {message}"));
            }
        }
        for key in &known {
            if !keys.contains_key(*key) {
                failures.push(format!("fixed, delete from gaps/{area}.rs: {key:?}"));
            }
        }
    }
    for (area, keys) in recorded {
        if !found.contains_key(area) {
            failures.push(format!("gaps/{area}.rs has no scenario"));
        }
        if !keys.windows(2).all(|pair| pair[0] < pair[1]) {
            failures.push(format!("gaps/{area}.rs must be sorted without duplicates"));
        }
    }
    failures
}

#[test]
fn gate_matches_the_recorded_gaps() {
    let failures = ratchet_failures(&all(), &gaps::AREAS);
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}
