/// One of the eight compass zones around the dial centre, clockwise from north.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Zone {
    pub const ALL: [Zone; 8] = [
        Zone::N,
        Zone::NE,
        Zone::E,
        Zone::SE,
        Zone::S,
        Zone::SW,
        Zone::W,
        Zone::NW,
    ];

    pub fn index(self) -> usize {
        match self {
            Zone::N => 0,
            Zone::NE => 1,
            Zone::E => 2,
            Zone::SE => 3,
            Zone::S => 4,
            Zone::SW => 5,
            Zone::W => 6,
            Zone::NW => 7,
        }
    }
}

/// The zone a pointer displacement points at, in screen coordinates (y grows downwards). Sectors
/// are 45 degrees wide and centred on the compass points; a sector edge belongs to the sector
/// clockwise of it. Returns `None` inside the dead zone (distance below `dead_zone`, and always
/// for a zero displacement) and for any non-finite input.
pub fn zone_at(delta: [f32; 2], dead_zone: f32) -> Option<Zone> {
    let [dx, dy] = delta;
    if !(dx.is_finite() && dy.is_finite() && dead_zone.is_finite()) {
        return None;
    }
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    let len = dx.hypot(dy);
    if len == 0.0 || len < f64::from(dead_zone) {
        return None;
    }
    let degrees = dx.atan2(-dy).to_degrees();
    let sector = ((degrees + 22.5) / 45.0).floor().rem_euclid(8.0);
    Zone::ALL.get(sector as usize).copied()
}

/// An entry on the dial or in the overflow list: the dial zone, or the row of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Zone(Zone),
    List(usize),
}

/// What choosing an entry does. `Unsupported` entries are drawn but never dispatch; the reason
/// says why, so the control is visibly unfinished rather than a dummy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action<A> {
    Dispatch(A),
    Unsupported(String),
}

/// Whether an entry can be chosen in the current context, computed by the caller from a snapshot
/// when the menu opens and checked again at dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Enabled,
    Disabled(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<A> {
    pub label: String,
    pub action: Action<A>,
    pub availability: Availability,
    /// Drawn with a check mark, for the current selection mode.
    pub checked: bool,
}

impl<A> Entry<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self {
            label: label.into(),
            action: Action::Dispatch(action),
            availability: Availability::Enabled,
            checked: false,
        }
    }

    pub fn unsupported(label: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: Action::Unsupported(reason.into()),
            availability: Availability::Enabled,
            checked: false,
        }
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.availability = Availability::Disabled(reason.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Why the entry cannot be chosen, or `None` when it can.
    pub fn disabled_reason(&self) -> Option<&str> {
        match (&self.action, &self.availability) {
            (Action::Unsupported(reason), _) => Some(reason),
            (_, Availability::Disabled(reason)) => Some(reason),
            _ => None,
        }
    }
}

/// What choosing a target does.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolved<'a, A> {
    Dispatch(&'a A),
    Disabled(&'a str),
    /// No entry at the target: an empty zone or a list row past the end.
    Empty,
}

/// Eight dial zones plus an overflow list; the order of the list is the order of its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkingMenu<A> {
    pub zones: [Option<Entry<A>>; 8],
    pub list: Vec<Entry<A>>,
}

impl<A> Default for MarkingMenu<A> {
    fn default() -> Self {
        Self {
            zones: std::array::from_fn(|_| None),
            list: Vec::new(),
        }
    }
}

impl<A> MarkingMenu<A> {
    pub fn entry(&self, target: Target) -> Option<&Entry<A>> {
        match target {
            Target::Zone(zone) => self.zones[zone.index()].as_ref(),
            Target::List(row) => self.list.get(row),
        }
    }

    pub fn resolve(&self, target: Target) -> Resolved<'_, A> {
        let Some(entry) = self.entry(target) else {
            return Resolved::Empty;
        };
        match (&entry.action, entry.disabled_reason()) {
            (_, Some(reason)) => Resolved::Disabled(reason),
            (Action::Dispatch(action), None) => Resolved::Dispatch(action),
            (Action::Unsupported(reason), None) => Resolved::Disabled(reason),
        }
    }

    pub fn is_enabled(&self, target: Target) -> bool {
        matches!(self.resolve(target), Resolved::Dispatch(_))
    }
}
