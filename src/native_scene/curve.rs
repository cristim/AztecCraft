use super::{GeometryViewFacts, NativeError, NativeId, SourceFormat, SourceRetention};
use glam::DVec3;
use std::{fmt, ops::Range};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveForm {
    Open,
    Closed,
    Periodic,
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
    pub(super) degree: u32,
    pub(super) form: CurveForm,
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
            && self.degree == other.degree
            && self.form == other.form
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
            .field("degree", &self.degree)
            .field("cv_count", &self.control_vertices.len())
            .field("knot_count", &self.knots.len())
            .finish()
    }
}

impl NativeCurveShape {
    /// This shape's own `.v`; `None` when the source did not prove it.
    pub fn local_visibility(&self) -> Option<bool> {
        self.view.local_visibility
    }
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
        self.degree
    }
    pub fn form(&self) -> CurveForm {
        self.form
    }
    pub fn control_vertices(&self) -> &[[f64; 3]] {
        &self.control_vertices
    }
    pub fn knots(&self) -> &[f64] {
        &self.knots
    }
    pub fn domain(&self) -> Result<[f64; 2], NativeError> {
        let degree = self.degree as usize;
        let end = self
            .knots
            .len()
            .checked_sub(degree)
            .ok_or(NativeError::InvalidCurve)?;
        Ok([
            *self
                .knots
                .get(degree.checked_sub(1).ok_or(NativeError::InvalidCurve)?)
                .ok_or(NativeError::InvalidCurve)?,
            *self.knots.get(end).ok_or(NativeError::InvalidCurve)?,
        ])
    }

    // Maya stores cv+degree-1 knots, omitting the two outer textbook knots. De Boor never
    // reads them (first low index is 1, last high index is len), so the end values returned
    // here are only placeholders and need no form-specific extrapolation.
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
        let curve_degree = self.degree as usize;
        let degree = if derivative {
            curve_degree
                .checked_sub(1)
                .ok_or(NativeError::InvalidCurve)?
        } else {
            curve_degree
        };
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
                    self.full_knot(
                        index
                            .checked_add(curve_degree + 1)
                            .ok_or(NativeError::ResourceLimit)?,
                    )? - self.full_knot(index.checked_add(1).ok_or(NativeError::ResourceLimit)?)?,
                )?;
                if denominator <= 0.0 {
                    return Err(NativeError::InvalidCurve);
                }
                finite_point(
                    finite_point(finite_point(next - p)? * curve_degree as f64)? / denominator,
                )?
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
        let span = index
            .checked_add(self.degree as usize)
            .ok_or(NativeError::ResourceLimit)?;
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
        let degree = self.degree as usize;
        let count = self.control_vertices.len();
        if !(1..=3).contains(&degree)
            || count <= degree
            || count.checked_add(degree - 1) != Some(self.knots.len())
        {
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
        if self.form != CurveForm::Periodic {
            self.validate_clamped(degree, start, end)?;
        }
        match self.form {
            CurveForm::Open => {}
            CurveForm::Closed => {
                // Documented: a closed curve has no gap between its first and last CVs.
                if self.control_vertices.first() != self.control_vertices.last() {
                    return Err(NativeError::UnsupportedView("closed curve with a CV gap"));
                }
            }
            CurveForm::Periodic => self.validate_periodic(degree, count, start, end)?,
        }
        self.view.visible()?;
        Ok(())
    }

    fn validate_clamped(&self, degree: usize, start: f64, end: f64) -> Result<(), NativeError> {
        if start >= end
            || !(end - start).is_finite()
            || self.knots[degree - 1..=self.knots.len() - degree]
                .windows(2)
                .any(|pair| pair[0] > pair[1] || !(pair[1] - pair[0]).is_finite())
        {
            return Err(NativeError::InvalidCurve);
        }
        if self.knots[..degree].iter().any(|v| *v != start)
            || self.knots[self.knots.len() - degree..]
                .iter()
                .any(|v| *v != end)
        {
            // An unclamped closed curve is a documented form we do not admit, not a malformed one.
            return Err(if self.form == CurveForm::Closed {
                NativeError::UnsupportedView("closed curve with unclamped knots")
            } else {
                NativeError::InvalidCurve
            });
        }
        if self.knots[degree - 1..=self.knots.len() - degree]
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(NativeError::UnsupportedView(
                "interior knot multiplicity (C0 or kinked curve)",
            ));
        }
        Ok(())
    }

    // Admitted: the one documented periodic shape (degree 3, uniform knots, the last `degree`
    // CVs overlapping the first). Other periodic shapes are refused, not guessed.
    fn validate_periodic(
        &self,
        degree: usize,
        count: usize,
        start: f64,
        end: f64,
    ) -> Result<(), NativeError> {
        if start >= end
            || !(end - start).is_finite()
            || self.knots.windows(2).any(|pair| pair[0] > pair[1])
        {
            return Err(NativeError::InvalidCurve);
        }
        if degree != 3 || count < 6 {
            return Err(NativeError::UnsupportedView("periodic curve shape"));
        }
        let step = self.knots[1] - self.knots[0];
        if self
            .knots
            .windows(2)
            .any(|pair| ((pair[1] - pair[0]) - step).abs() > 1e-9 * step)
        {
            return Err(NativeError::UnsupportedView("nonuniform periodic knots"));
        }
        if self.control_vertices[..degree] != self.control_vertices[count - degree..] {
            return Err(NativeError::UnsupportedView(
                "periodic curve whose last CVs do not overlap its first",
            ));
        }
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
mod periodic_tests;
#[cfg(test)]
mod tests;
