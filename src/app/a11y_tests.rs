//! Accessibility gate (#465): walks the AccessKit tree egui emits and reports controls a screen
//! reader cannot use. Reads the raw `TreeUpdate` from `FullOutput`, like the older per-control tests.
//!
//! Rules: R1 name, R2 role, R3 disabled reason, R3b stale reason, R4 state, R5 name quality.
//! Fix a failure on the control (`accessible_name`, `labelled_by`, `widget_info`); see
//! `.project-docs/accessibility.md`. There is no per-control allow-list.

use eframe::egui::{
    self,
    accesskit::{Action, Node, NodeId, Role, Toggled, TreeUpdate},
};
use std::collections::{HashMap, HashSet};

/// Prefix of the description a disabled control carries: its reason, in the form AXHelp speaks.
pub(super) const DISABLED_PREFIX: &str = "Unavailable: ";

const ACTIONS: [Action; 6] = [
    Action::Click,
    Action::Focus,
    Action::Increment,
    Action::Decrement,
    Action::SetValue,
    Action::Expand,
];
const ACTIONABLE_ROLES: [Role; 11] = [
    Role::Button,
    Role::CheckBox,
    Role::RadioButton,
    Role::ComboBox,
    Role::TextInput,
    Role::SpinButton,
    Role::Slider,
    Role::MenuItem,
    Role::Tab,
    Role::Link,
    Role::Switch,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Rule {
    Name,
    Role,
    Reason,
    StaleReason,
    State,
    NameQuality,
}

impl Rule {
    pub(super) fn code(self) -> &'static str {
        match self {
            Rule::Name => "R1",
            Rule::Role => "R2",
            Rule::Reason => "R3",
            Rule::StaleReason => "R3b",
            Rule::State => "R4",
            Rule::NameQuality => "R5",
        }
    }

    fn fix(self) -> &'static str {
        match self {
            Rule::Name => {
                "no accessible name: call `.accessible_name(..)`, `.labelled_by(..)` or `widget_info`"
            }
            Rule::Role => {
                "generic role: give the widget a role with `widget_info` (custom widgets)"
            }
            Rule::Reason => {
                "disabled without a reason: the description must start with `Unavailable: `"
            }
            Rule::StaleReason => "enabled but the description still starts with `Unavailable: `",
            Rule::State => "no checked state: use `WidgetInfo::selected` or set `toggled`",
            Rule::NameQuality => "the name is a glyph or too short to be spoken: name it in words",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Violation {
    pub(super) scenario: String,
    pub(super) rule: Rule,
    pub(super) role: Role,
    pub(super) node: NodeId,
    pub(super) name: String,
    pub(super) bounds: Option<(f64, f64, f64, f64)>,
    pub(super) ancestor: String,
    pub(super) anchor: String,
    pub(super) in_window: bool,
}

impl Violation {
    /// `scenario|rule|role|anchor`: ids when the widget has a known one, else its nearest named
    /// ancestor plus the index among same-role nodes under it, so a fix never shifts other keys.
    pub(super) fn key(&self) -> String {
        format!(
            "{}|{}|{:?}|{}",
            self.scenario,
            self.rule.code(),
            self.role,
            self.anchor
        )
    }

    pub(super) fn message(&self) -> String {
        let window = if self.in_window && self.rule == Rule::Role {
            " (inside a window: if this is egui window chrome, register the window in `Scenario::windows`)"
        } else {
            ""
        };
        format!(
            "[{}] {} {:?} {:?} bounds {:?} under {:?}: {}{}; see .project-docs/accessibility.md",
            self.scenario,
            self.rule.code(),
            self.role,
            self.name,
            self.bounds,
            self.ancestor,
            self.rule.fix(),
            window
        )
    }
}

/// Stable names for widgets that have an explicit egui id; their keys never shift.
pub(super) fn known_ids() -> HashMap<NodeId, String> {
    super::text_field_ids()
        .enumerate()
        .map(|(index, id)| (id.accesskit_id(), format!("@field{index}")))
        .collect()
}

/// Area id of `egui::Window::new(title)` without an explicit id (egui hashes the title text as an
/// `Option<Cow<str>>`).
pub(super) fn titled_window_id(title: &str) -> egui::Id {
    egui::Id::unique(Some(std::borrow::Cow::Borrowed(title)))
}

/// Ids of the unnamed chrome egui adds to a window of area id `area`: the title-bar click area
/// (every window) and the resize corner (resizable windows).
fn chrome_ids(windows: &[egui::Id]) -> HashSet<NodeId> {
    windows
        .iter()
        .flat_map(|area| {
            [
                area.with("__title_click").accesskit_id(),
                area.with("resize").with("__resize_corner").accesskit_id(),
            ]
        })
        .collect()
}

/// Text labels are not controls even though egui makes them selectable (Click and Focus).
fn is_actionable(node: &Node) -> bool {
    node.role() != Role::Label
        && (ACTIONS.iter().any(|action| node.supports_action(*action))
            || ACTIONABLE_ROLES.contains(&node.role()))
}

/// The label (a Label node carries its text in the value), else what the first `labelled_by`
/// target that has a name speaks.
fn name_of(nodes: &HashMap<NodeId, &Node>, node: &Node) -> Option<String> {
    let spoken = |n: &Node| {
        n.label()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                n.value()
                    .filter(|s| !s.is_empty() && n.role() == Role::Label)
            })
            .map(str::to_owned)
    };
    spoken(node).or_else(|| {
        node.labelled_by()
            .iter()
            .find_map(|id| nodes.get(id).and_then(|target| spoken(target)))
    })
}

/// One node with everything the rules and the keys need, so the rules read plain fields.
pub(super) struct Entry {
    pub(super) id: NodeId,
    pub(super) role: Role,
    pub(super) name: Option<String>,
    bounds: Option<(f64, f64, f64, f64)>,
    ancestor: String,
    pub(super) anchor: String,
    in_window: bool,
    /// Egui window chrome of a registered window: the rules skip it, and so does the keyboard walk.
    pub(super) exempt: bool,
    pub(super) actionable: bool,
    pub(super) disabled: bool,
    description: String,
    toggled: Option<Toggled>,
}

impl Entry {
    pub(super) fn violation(&self, scenario: &str, rule: Rule) -> Violation {
        Violation {
            scenario: scenario.to_owned(),
            rule,
            role: self.role,
            node: self.id,
            name: self.name.clone().unwrap_or_default(),
            bounds: self.bounds,
            ancestor: self.ancestor.clone(),
            anchor: self.anchor.clone(),
            in_window: self.in_window,
        }
    }
}

/// Depth-first in child order, so the per-ancestor sibling indices are deterministic.
pub(super) fn entries(update: &TreeUpdate, windows: &[egui::Id]) -> Vec<Entry> {
    let Some(tree) = update.tree.as_ref() else {
        return Vec::new();
    };
    let nodes: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let chrome = chrome_ids(windows);
    let known = known_ids();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    let mut counts: HashMap<(NodeId, Role), usize> = HashMap::new();
    let mut stack = vec![(tree.root, (tree.root, String::from("root")), false)];
    while let Some((id, ancestor, in_window)) = stack.pop() {
        let Some(node) = nodes.get(&id).copied() else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let role = node.role();
        let index = counts.entry((ancestor.0, role)).or_default();
        let anchor = known
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("{}#{}", ancestor.1, index));
        *index += 1;
        order.push(Entry {
            id,
            role,
            name: name_of(&nodes, node),
            bounds: node.bounds().map(|b| (b.x0, b.y0, b.x1, b.y1)),
            ancestor: ancestor.1.clone(),
            anchor,
            in_window,
            exempt: role == Role::Unknown && in_window && chrome.contains(&id),
            actionable: is_actionable(node),
            disabled: node.is_disabled(),
            description: node.description().unwrap_or("").to_owned(),
            toggled: node.toggled(),
        });
        let next_ancestor = match node.label().filter(|label| !label.is_empty()) {
            Some(label) => (id, label.to_owned()),
            None => ancestor,
        };
        let next_window = in_window || (role == Role::Window && id != tree.root);
        for child in node.children().iter().rev() {
            stack.push((*child, next_ancestor.clone(), next_window));
        }
    }
    order
}

/// Violations in `update`. `windows` are the area ids of the windows the scenario opens.
pub(super) fn violations(
    scenario: &str,
    update: &TreeUpdate,
    windows: &[egui::Id],
) -> Vec<Violation> {
    let mut found = Vec::new();
    for entry in entries(update, windows) {
        let mut fail = |rule: Rule| found.push(entry.violation(scenario, rule));
        let reason = entry.description.starts_with(DISABLED_PREFIX);
        if !entry.disabled && reason {
            fail(Rule::StaleReason);
        }
        if entry.exempt || !entry.actionable {
            continue;
        }
        if entry.name.is_none() {
            fail(Rule::Name);
        }
        if matches!(entry.role, Role::Unknown | Role::GenericContainer) {
            fail(Rule::Role);
        }
        if entry.disabled && !reason {
            fail(Rule::Reason);
        }
        if matches!(
            entry.role,
            Role::CheckBox | Role::RadioButton | Role::Switch
        ) && !matches!(entry.toggled, Some(Toggled::True | Toggled::False))
        {
            fail(Rule::State);
        }
        if let Some(name) = &entry.name
            && name.chars().filter(|c| c.is_alphabetic()).count() < 2
        {
            fail(Rule::NameQuality);
        }
    }
    found
}

mod gaps;
mod rig;
mod scenarios;
mod self_tests;
