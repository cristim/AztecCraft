//! Maya release years AztecCraft can emulate, and which documented behaviours each one has.
//!
//! The ranges are bracketed by pages that state the behaviour (`absent_in`, `present_in`); the
//! year it appeared lies between them and `since` takes the later, conservative bound.

use std::fmt;

/// One release year in `OLDEST..=PINNED`; construct it with `try_from` so no caller holds a year
/// the table does not cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MayaVersion(u16);

impl MayaVersion {
    pub const OLDEST: MayaVersion = MayaVersion(2011);
    /// The release the documentation pin in `reference.md` names.
    pub const PINNED: MayaVersion = MayaVersion(2027);

    pub fn year(self) -> u16 {
        self.0
    }
}

impl Default for MayaVersion {
    fn default() -> Self {
        Self::PINNED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionOutOfRange;

impl fmt::Display for VersionOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Maya version must be a year from {} to {}",
            MayaVersion::OLDEST.0,
            MayaVersion::PINNED.0
        )
    }
}

impl std::error::Error for VersionOutOfRange {}

impl TryFrom<u64> for MayaVersion {
    type Error = VersionOutOfRange;

    fn try_from(year: u64) -> Result<Self, Self::Error> {
        u16::try_from(year)
            .ok()
            .filter(|y| (Self::OLDEST.0..=Self::PINNED.0).contains(y))
            .map(MayaVersion)
            .ok_or(VersionOutOfRange)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureId {
    FrameSelectionChildrenSplit,
}

impl FeatureId {
    /// The stable name used in inspection output.
    pub fn id(self) -> &'static str {
        match self {
            Self::FrameSelectionChildrenSplit => "frame_selection_children_split",
        }
    }
}

/// When a documented behaviour exists. `absent_in` and `present_in` are the nearest releases
/// whose pages show it missing and present; `since` is the first year this table treats it as
/// available.
#[derive(Debug, Clone, Copy)]
pub struct FeatureRange {
    pub feature: FeatureId,
    pub since: u16,
    pub absent_in: u16,
    pub present_in: u16,
    pub urls: &'static [&'static str],
}

pub const FEATURE_RANGES: [FeatureRange; 1] = [FeatureRange {
    feature: FeatureId::FrameSelectionChildrenSplit,
    since: 2016,
    absent_in: 2014,
    present_in: 2016,
    urls: &[
        "https://download.autodesk.com/global/docs/maya2014/en_us/files/PC_Maya_Hotkeys.htm",
        "https://help.autodesk.com/cloudhelp/2016/ENU/Maya/files/GUID-6C1FACC3-8B01-45CB-8458-581DFAFC2A2F.htm",
    ],
}];

pub fn is_available(feature: FeatureId, version: MayaVersion) -> bool {
    FEATURE_RANGES
        .iter()
        .find(|row| row.feature == feature)
        .is_some_and(|row| version.0 >= row.since)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_FEATURES: [FeatureId; 1] = [FeatureId::FrameSelectionChildrenSplit];

    fn v(year: u64) -> MayaVersion {
        MayaVersion::try_from(year).unwrap()
    }

    #[test]
    fn every_feature_has_exactly_one_consistent_row() {
        for feature in ALL_FEATURES {
            let rows: Vec<_> = FEATURE_RANGES
                .iter()
                .filter(|r| r.feature == feature)
                .collect();
            assert_eq!(rows.len(), 1, "{}", feature.id());
            let row = rows[0];
            assert!(row.absent_in < row.present_in);
            assert_eq!(row.present_in, row.since);
            for year in [row.since, row.absent_in, row.present_in] {
                assert!((MayaVersion::OLDEST.year()..=MayaVersion::PINNED.year()).contains(&year));
            }
            assert!(!row.urls.is_empty());
            for url in row.urls {
                assert!(
                    url.starts_with("https://help.autodesk.com/")
                        || url.starts_with("https://download.autodesk.com/"),
                    "{url}"
                );
            }
        }
    }

    #[test]
    fn availability_flips_at_the_since_year() {
        let f = FeatureId::FrameSelectionChildrenSplit;
        for year in [2011, 2014, 2015] {
            assert!(!is_available(f, v(year)), "{year}");
        }
        for year in [2016, 2017, 2027] {
            assert!(is_available(f, v(year)), "{year}");
        }
    }

    #[test]
    fn parse_accepts_the_bounds_and_rejects_everything_else() {
        assert_eq!(v(2011), MayaVersion::OLDEST);
        assert_eq!(v(2027), MayaVersion::PINNED);
        assert_eq!(MayaVersion::default(), MayaVersion::PINNED);
        for bad in [0, 2010, 2028, 65_536 + 2020, u64::MAX] {
            assert_eq!(MayaVersion::try_from(bad), Err(VersionOutOfRange), "{bad}");
        }
    }

    #[test]
    fn feature_ids_are_stable() {
        assert_eq!(
            FeatureId::FrameSelectionChildrenSplit.id(),
            "frame_selection_children_split"
        );
    }
}
