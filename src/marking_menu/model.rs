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

/// The state of an entry: what choosing it does, or why it cannot be chosen. The caller computes
/// it from a snapshot when the menu opens and the interaction checks it again at dispatch.
/// `Disabled` is a context refusal (the entry may work in another context); `Unsupported` is a
/// control that has no implementation yet. Neither dispatches; each carries the reason shown to
/// the user, so a control is visibly unfinished rather than a dummy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryState<A> {
    Enabled(A),
    Disabled(String),
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<A> {
    pub label: String,
    pub state: EntryState<A>,
    /// Drawn with a check mark, for the current selection mode.
    pub checked: bool,
}

impl<A> Entry<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self::with_state(label, EntryState::Enabled(action))
    }

    pub fn disabled(label: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::with_state(label, EntryState::Disabled(reason.into()))
    }

    pub fn unsupported(label: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::with_state(label, EntryState::Unsupported(reason.into()))
    }

    fn with_state(label: impl Into<String>, state: EntryState<A>) -> Self {
        Self {
            label: label.into(),
            state,
            checked: false,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Why the entry cannot be chosen, or `None` when it can.
    pub fn disabled_reason(&self) -> Option<&str> {
        match &self.state {
            EntryState::Enabled(_) => None,
            EntryState::Disabled(reason) | EntryState::Unsupported(reason) => Some(reason),
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
        match self.entry(target).map(|entry| &entry.state) {
            None => Resolved::Empty,
            Some(EntryState::Enabled(action)) => Resolved::Dispatch(action),
            Some(EntryState::Disabled(reason) | EntryState::Unsupported(reason)) => {
                Resolved::Disabled(reason)
            }
        }
    }

    pub fn is_enabled(&self, target: Target) -> bool {
        matches!(self.resolve(target), Resolved::Dispatch(_))
    }
}
