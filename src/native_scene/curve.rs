use super::{GeometryViewFacts, NativeError, NativeId, SourceFormat, SourceRetention};
use glam::DVec3;
use std::{fmt, ops::Range};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveForm {
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CurveSource {
    pub(super) owner: NativeId,
    pub(super) statement: Range<usize>,
}

#[derive(Clone)]
pub struct NativeCurveShape {
    pub(super) id: NativeId,
    pub(super) uuid: Option<Uuid>,
    pub(super) name: String,
    pub(super) control_vertices: Vec<[f64; 3]>,
    pub(super) knots: Vec<f64>,
    pub(super) source: CurveSource,
    pub(super) view: GeometryViewFacts,
}

impl PartialEq for NativeCurveShape {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.uuid == other.uuid
            && self.name == other.name
            && self.source == other.source
            && self.view == other.view
            && self.control_vertices.len() == other.control_vertices.len()
            && self.knots.len() == other.knots.len()
            && self
                .control_vertices
                .iter()
                .zip(&other.control_vertices)
                .all(|(a, b)| a.map(f64::to_bits) == b.map(f64::to_bits))
            && self
                .knots
                .iter()
                .zip(&other.knots)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

impl fmt::Debug for NativeCurveShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeCurveShape")
            .field("id", &self.id)
            .field("cv_count", &self.control_vertices.len())
            .field("knot_count", &self.knots.len())
            .finish()
    }
}

impl NativeCurveShape {
    pub fn id(&self) -> NativeId {
        self.id
    }
    pub fn uuid(&self) -> Option<Uuid> {
        self.uuid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn degree(&self) -> u32 {
        3
    }
    pub fn form(&self) -> CurveForm {
        CurveForm::Open
    }
    pub fn control_vertices(&self) -> &[[f64; 3]] {
        &self.control_vertices
    }
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }
    pub fn domain(&self) -> Result<[f64; 2], NativeError> {
        let end = self
            .knots
            .len()
            .checked_sub(3)
            .ok_or(NativeError::InvalidCurve)?;
        Ok([
            *self.knots.get(2).ok_or(NativeError::InvalidCurve)?,
            *self.knots.get(end).ok_or(NativeError::InvalidCurve)?,
        ])
    }

    fn full_knot(&self, index: usize) -> Result<f64, NativeError> {
        let value = if index == 0 {
            self.knots.first()
        } else if index
            == self
                .knots
                .len()
                .checked_add(1)
                .ok_or(NativeError::ResourceLimit)?
        {
            self.knots.last()
        } else {
            self.knots.get(index - 1)
        };
        value
            .copied()
            .filter(|value| value.is_finite())
            .ok_or(NativeError::InvalidCurve)
    }

    pub(crate) fn evaluate(&self, parameter: f64) -> Result<DVec3, NativeError> {
        let [start, end] = self.domain()?;
        if !parameter.is_finite() || parameter < start || parameter > end {
            return Err(NativeError::InvalidCurve);
        }
        let span = if parameter == end {
            self.control_vertices
                .len()
                .checked_sub(1)
                .ok_or(NativeError::InvalidCurve)?
        } else {
            self.knots.partition_point(|knot| *knot <= parameter)
        };
        self.de_boor(span, parameter, false)
    }

    fn de_boor(&self, span: usize, parameter: f64, derivative: bool) -> Result<DVec3, NativeError> {
        let degree = if derivative { 2usize } else { 3usize };
        let knot_offset = usize::from(derivative);
        let span = span
            .checked_sub(knot_offset)
            .ok_or(NativeError::InvalidCurve)?;
        let first = span.checked_sub(degree).ok_or(NativeError::InvalidCurve)?;
        let mut values = [DVec3::ZERO; 4];
        for (j, value) in values.iter_mut().enumerate().take(degree + 1) {
            let index = first.checked_add(j).ok_or(NativeError::ResourceLimit)?;
            let p = DVec3::from_array(
                *self
                    .control_vertices
                    .get(index)
                    .ok_or(NativeError::InvalidCurve)?,
            );
            *value = if derivative {
                let next = DVec3::from_array(
                    *self
                        .control_vertices
                        .get(index.checked_add(1).ok_or(NativeError::ResourceLimit)?)
                        .ok_or(NativeError::InvalidCurve)?,
                );
                let denominator = finite_scalar(
                    self.full_knot(index.checked_add(4).ok_or(NativeError::ResourceLimit)?)?
                        - self
                            .full_knot(index.checked_add(1).ok_or(NativeError::ResourceLimit)?)?,
                )?;
                if denominator <= 0.0 {
                    return Err(NativeError::InvalidCurve);
                }
                finite_point(finite_point(finite_point(next - p)? * 3.0)? / denominator)?
            } else {
                finite_point(p)?
            };
        }
        for round in 1..=degree {
            for j in (round..=degree).rev() {
                let low_index = first
                    .checked_add(j)
                    .and_then(|i| i.checked_add(knot_offset))
                    .ok_or(NativeError::ResourceLimit)?;
                let high_index = span
                    .checked_add(1)
                    .and_then(|i| i.checked_add(j))
                    .and_then(|i| i.checked_sub(round))
                    .and_then(|i| i.checked_add(knot_offset))
                    .ok_or(NativeError::ResourceLimit)?;
                let low = self.full_knot(low_index)?;
                let denominator = finite_scalar(self.full_knot(high_index)? - low)?;
                if denominator <= 0.0 {
                    return Err(NativeError::InvalidCurve);
                }
                let alpha = finite_scalar(finite_scalar(parameter - low)? / denominator)?;
                if !(0.0..=1.0).contains(&alpha) {
                    return Err(NativeError::InvalidCurve);
                }
                values[j] = finite_point(
                    values[j - 1] + finite_point(finite_point(values[j] - values[j - 1])? * alpha)?,
                )?;
            }
        }
        finite_point(values[degree])
    }

    pub(crate) fn bezier_span(&self, index: usize) -> Result<[DVec3; 4], NativeError> {
        let span = index.checked_add(3).ok_or(NativeError::ResourceLimit)?;
        if span >= self.control_vertices.len() {
            return Err(NativeError::InvalidCurve);
        }
        let a = self.full_knot(span)?;
        let b = self.full_knot(span.checked_add(1).ok_or(NativeError::ResourceLimit)?)?;
        let width = finite_scalar(b - a)?;
        if width <= 0.0 {
            return Err(NativeError::InvalidCurve);
        }
        let start = self.evaluate(a)?;
        let end = self.evaluate(b)?;
        let left = finite_point(self.de_boor(span, a, true)? * finite_scalar(width / 3.0)?)?;
        let right = finite_point(self.de_boor(span, b, true)? * finite_scalar(width / 3.0)?)?;
        Ok([
            start,
            finite_point(start + left)?,
            finite_point(end - right)?,
            end,
        ])
    }

    pub(super) fn validate(&self, source: Option<&SourceRetention>) -> Result<(), NativeError> {
        let source = source
            .filter(|s| s.format == SourceFormat::Ascii)
            .ok_or(NativeError::InvalidRetention)?;
        if self.source.owner != self.id
            || self.source.statement.start >= self.source.statement.end
            || source.bytes.get(self.source.statement.clone()).is_none()
            || !source.records.contains(&self.source.statement)
        {
            return Err(NativeError::InvalidRetention);
        }
        let count = self.control_vertices.len();
        if count < 4 || count.checked_add(2) != Some(self.knots.len()) {
            return Err(NativeError::InvalidCurve);
        }
        if self
            .control_vertices
            .iter()
            .flatten()
            .chain(&self.knots)
            .any(|v| !v.is_finite())
        {
            return Err(NativeError::NonFinite);
        }
        let [start, end] = self.domain()?;
        if start >= end
            || !(end - start).is_finite()
            || self.knots[..3].iter().any(|v| *v != start)
            || self.knots[self.knots.len() - 3..].iter().any(|v| *v != end)
            || self.knots[2..=self.knots.len() - 3]
                .windows(2)
                .any(|pair| pair[0] >= pair[1] || !(pair[1] - pair[0]).is_finite())
        {
            return Err(NativeError::InvalidCurve);
        }
        self.view.visible()?;
        Ok(())
    }
}

fn finite_scalar(value: f64) -> Result<f64, NativeError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(NativeError::UnsupportedView("nonfinite cubic evaluation"))
}

fn finite_point(value: DVec3) -> Result<DVec3, NativeError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(NativeError::UnsupportedView("nonfinite cubic evaluation"))
}

#[cfg(test)]
mod tests;
