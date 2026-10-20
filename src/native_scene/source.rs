use super::{AngularUnit, NativeError, NativeId, Units};
use std::{fmt, ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceFormat {
    Ascii,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TransformField {
    Translation,
    Rotation,
    Scale,
    RotatePivot,
    ScalePivot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Storage {
    Centimeters,
    Radians,
    Unitless,
}

/// Fields whose absent leaf may be inserted when an opened Maya Binary is saved. Rotation and scale
/// need their own guard work first and keep their refusal. PROVISIONAL.
pub(super) const INSERTABLE: &[TransformField] = &[TransformField::Translation];

impl TransformField {
    pub(super) fn storage(self) -> Storage {
        match self {
            Self::Translation | Self::RotatePivot | Self::ScalePivot => Storage::Centimeters,
            Self::Rotation => Storage::Radians,
            Self::Scale => Storage::Unitless,
        }
    }
}

impl Storage {
    fn factor(self, units: &Units) -> Result<f64, NativeError> {
        match self {
            Self::Centimeters => units.linear_factor(),
            Self::Radians => match &units.angular {
                AngularUnit::Degree => Ok(std::f64::consts::PI / 180.0),
                AngularUnit::Radian => Ok(1.0),
                AngularUnit::Unsupported(_) => Err(NativeError::UnsupportedView("angular unit")),
            },
            Self::Unitless => Ok(1.0),
        }
    }
    pub(super) fn internal(self, value: f64, units: &Units) -> Result<f64, NativeError> {
        finite(value * self.factor(units)?)
    }
    pub(super) fn declared(self, value: f64, units: &Units) -> Result<f64, NativeError> {
        finite(value / self.factor(units)?)
    }
}

fn finite(value: f64) -> Result<f64, NativeError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(NativeError::NonFinite)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceLeaf {
    pub(super) payload: Range<usize>,
    pub(super) numeric: Range<usize>,
    pub(super) owner: NativeId,
    pub(super) field: TransformField,
    pub(super) storage: Storage,
}

/// Where an absent default leaf of `owner` may be spliced into the source bytes.
///
/// `at` is the byte offset of the new record and `aux` the header word every `DBL3` of the file
/// shares, which the record copies. `ancestors` holds the header start of every
/// enclosing group, innermost first and the root last, whose size words grow with the record.
/// PROVISIONAL: see `.project-docs/binary-leaf-insertion.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InsertSite {
    pub(super) owner: NativeId,
    pub(super) field: TransformField,
    pub(super) at: usize,
    pub(super) aux: u32,
    pub(super) ancestors: Vec<usize>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SourceRetention {
    pub(super) bytes: Arc<[u8]>,
    pub(super) format: SourceFormat,
    pub(super) records: Vec<Range<usize>>,
    pub(super) leaves: Vec<SourceLeaf>,
    pub(super) sites: Vec<InsertSite>,
    pub(super) appearance: Option<Arc<super::appearance::SourceAppearance>>,
}

impl fmt::Debug for SourceRetention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceRetention")
            .field("format", &self.format)
            .field("byte_count", &self.bytes.len())
            .field("atom_count", &self.records.len())
            .field("leaf_count", &self.leaves.len())
            .finish()
    }
}

impl SourceRetention {
    pub(super) fn validate(&self) -> Result<(), NativeError> {
        let mut end = 0;
        for range in &self.records {
            if range.start != end || range.end <= range.start || range.end > self.bytes.len() {
                return Err(NativeError::InvalidRetention);
            }
            end = range.end;
        }
        if end != self.bytes.len()
            || (self.format == SourceFormat::Ascii && !self.leaves.is_empty())
            || (self.format != SourceFormat::Binary && self.appearance.is_some())
        {
            return Err(NativeError::InvalidRetention);
        }
        for (index, leaf) in self.leaves.iter().enumerate() {
            if leaf.payload.start > leaf.numeric.start
                || leaf.numeric.end > leaf.payload.end
                || leaf.payload.end > self.bytes.len()
                || leaf.numeric.end.checked_sub(leaf.numeric.start) != Some(24)
                || leaf.storage != leaf.field.storage()
                || self.leaves[..index].iter().any(|other| {
                    leaf.numeric.start < other.numeric.end && other.numeric.start < leaf.numeric.end
                })
            {
                return Err(NativeError::InvalidRetention);
            }
        }
        self.validate_sites()
    }

    fn validate_sites(&self) -> Result<(), NativeError> {
        let group_end = |start: usize| -> Option<usize> {
            let header = self.bytes.get(start..start.checked_add(16)?)?;
            if !matches!(&header[..4], b"FOR8" | b"LIS8") {
                return None;
            }
            let size = u64::from_be_bytes(header[8..].try_into().ok()?);
            start
                .checked_add(16)?
                .checked_add(usize::try_from(size).ok()?)
        };
        let is_atom = |offset: usize| {
            offset == self.bytes.len()
                || self
                    .records
                    .binary_search_by_key(&offset, |atom| atom.start)
                    .is_ok()
        };
        for (index, site) in self.sites.iter().enumerate() {
            let mut inside = site.at;
            let mut limit = None;
            let valid = site.field == TransformField::Translation
                && self.format == SourceFormat::Binary
                && is_atom(site.at)
                && (index == 0 || self.sites[index - 1].owner < site.owner)
                && site.ancestors.last() == Some(&0)
                && site.ancestors.iter().all(|&start| {
                    let Some(end) = group_end(start) else {
                        return false;
                    };
                    let nested = start.checked_add(20).is_some_and(|body| body <= inside)
                        && inside <= end
                        && limit.is_none_or(|inner| end >= inner)
                        && is_atom(start);
                    inside = start;
                    limit = Some(end);
                    nested
                });
            if !valid {
                return Err(NativeError::InvalidRetention);
            }
        }
        Ok(())
    }
    /// The recorded site for `owner`'s absent `field` leaf, if the grammar allowed one.
    pub(super) fn insert_site(
        &self,
        owner: NativeId,
        field: TransformField,
    ) -> Option<&InsertSite> {
        let index = self
            .sites
            .binary_search_by_key(&owner, |site| site.owner)
            .ok()?;
        self.sites.get(index).filter(|site| site.field == field)
    }
    /// Whether an edit of `owner`'s absent `field` leaf can be saved by inserting a record.
    pub(super) fn can_insert(&self, owner: NativeId, field: TransformField) -> bool {
        INSERTABLE.contains(&field) && self.insert_site(owner, field).is_some()
    }
    pub(super) fn binary_leaf(
        &self,
        origin: &ScalarOrigin,
        owner: NativeId,
        field: TransformField,
        component: usize,
    ) -> Result<&SourceLeaf, NativeError> {
        let (leaf, bound) = match origin {
            ScalarOrigin::Binary { leaf, component } => (*leaf, *component),
            #[cfg(test)]
            ScalarOrigin::Ascii { .. } => return Err(NativeError::InvalidRetention),
        };
        let leaf = self.leaves.get(leaf).ok_or(NativeError::InvalidRetention)?;
        if self.format != SourceFormat::Binary
            || leaf.owner != owner
            || leaf.field != field
            || bound != component
            || component >= 3
            || leaf.storage != field.storage()
        {
            return Err(NativeError::InvalidRetention);
        }
        Ok(leaf)
    }
    pub(super) fn original(
        &self,
        origin: &ScalarOrigin,
        owner: NativeId,
        field: TransformField,
        component: usize,
    ) -> Result<f64, NativeError> {
        let leaf = self.binary_leaf(origin, owner, field, component)?;
        let start = component
            .checked_mul(8)
            .and_then(|offset| leaf.numeric.start.checked_add(offset))
            .ok_or(NativeError::InvalidRetention)?;
        let end = start.checked_add(8).ok_or(NativeError::InvalidRetention)?;
        if end > leaf.numeric.end {
            return Err(NativeError::InvalidRetention);
        }
        let word: [u8; 8] = self
            .bytes
            .get(start..end)
            .ok_or(NativeError::InvalidRetention)?
            .try_into()
            .map_err(|_| NativeError::InvalidRetention)?;
        finite(f64::from_be_bytes(word))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScalarOrigin {
    #[cfg(test)]
    Ascii {
        literal: Range<usize>,
    },
    Binary {
        leaf: usize,
        component: usize,
    },
}

#[cfg(test)]
mod tests;
