use super::{BinaryError, resource};
const MAX_WORK: usize = 50_000_000;

#[derive(Default)]
pub(super) struct Counts {
    pub transforms: usize,
    pub shapes: usize,
    pub dag: usize,
    pub atoms: usize,
    pub leaves: usize,
    pub points: usize,
    pub edges: usize,
    pub faces: usize,
    pub corners: usize,
    pub offsets: usize,
    pub chunks: usize,
    pub records: usize,
}

fn sum(values: impl IntoIterator<Item = usize>) -> Result<usize, BinaryError> {
    values.into_iter().try_fold(0usize, |total, value| {
        total.checked_add(value).ok_or_else(resource)
    })
}

fn product(a: usize, b: usize) -> Result<usize, BinaryError> {
    a.checked_mul(b).ok_or_else(resource)
}

impl Counts {
    pub fn admit_work(&self) -> Result<(), BinaryError> {
        let v = sum([self.transforms, self.shapes])?;
        let pairs = if self.leaves == 0 {
            0
        } else {
            product(self.leaves, self.leaves - 1)? / 2
        };
        let linear = sum([
            self.chunks,
            self.records,
            self.transforms,
            self.shapes,
            self.dag,
            self.atoms,
            self.leaves,
            self.points,
            self.edges,
            self.faces,
            self.corners,
            self.offsets,
        ])?;
        let work = sum([
            product(product(2, v)?, self.dag)?,
            product(self.transforms, self.leaves)?,
            pairs,
            product(32, linear)?,
        ])?;
        if work > MAX_WORK {
            return Err(resource());
        }
        Ok(())
    }

    pub fn validation_bytes(&self) -> Result<usize, BinaryError> {
        let v = sum([self.transforms, self.shapes])?;
        super::super::admission::validation_bytes(self.transforms, v, self.dag)
            .map_err(|_| resource())
    }
}

pub(super) struct Budget {
    inner: super::super::admission::Budget,
}

impl Budget {
    pub fn snapshot(&self) -> Self {
        Self {
            inner: self.inner.snapshot(),
        }
    }
    pub fn new(bytes: usize) -> Result<Self, BinaryError> {
        Ok(Self {
            inner: super::super::admission::Budget::new(bytes).map_err(|_| resource())?,
        })
    }

    pub fn charge(&mut self, bytes: usize) -> Result<(), BinaryError> {
        self.inner.charge(bytes).map_err(|_| resource())
    }

    pub fn preflight<T>(&mut self, count: usize) -> Result<(), BinaryError> {
        self.inner.preflight::<T>(count).map_err(|_| resource())
    }

    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, BinaryError> {
        self.inner.vector(count).map_err(|_| resource())
    }

    pub fn string(&mut self, text: &str) -> Result<String, BinaryError> {
        self.inner.string(text).map_err(|_| resource())
    }
}
