use super::NativeError;
use std::mem::size_of;

const MAX_BYTES: usize = 128 * 1024 * 1024;

pub(crate) fn validation_bytes(
    transforms: usize,
    identities: usize,
    dag: usize,
) -> Result<usize, NativeError> {
    transforms
        .checked_mul(3)
        .and_then(|transforms| {
            identities
                .checked_mul(3)
                .and_then(|identities| transforms.checked_add(identities))
        })
        .and_then(|tables| tables.checked_add(dag))
        .and_then(|entries| entries.checked_mul(256))
        .and_then(|bytes| bytes.checked_add(4096))
        .ok_or(NativeError::ResourceLimit)
}

pub(crate) struct Budget {
    used: usize,
}

impl Budget {
    pub fn snapshot(&self) -> Self {
        Self { used: self.used }
    }

    pub fn new(bytes: usize) -> Result<Self, NativeError> {
        let mut budget = Self { used: 0 };
        budget.charge(bytes)?;
        Ok(budget)
    }

    pub fn charge(&mut self, bytes: usize) -> Result<(), NativeError> {
        self.used = self
            .used
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or(NativeError::ResourceLimit)?;
        Ok(())
    }

    pub fn preflight<T>(&mut self, count: usize) -> Result<(), NativeError> {
        self.charge(
            count
                .checked_mul(size_of::<T>())
                .ok_or(NativeError::ResourceLimit)?,
        )
    }

    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, NativeError> {
        let bytes = count
            .checked_mul(size_of::<T>())
            .ok_or(NativeError::ResourceLimit)?;
        self.used
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or(NativeError::ResourceLimit)?;
        let mut vector = Vec::new();
        vector
            .try_reserve_exact(count)
            .map_err(|_| NativeError::ResourceLimit)?;
        self.preflight::<T>(vector.capacity())?;
        Ok(vector)
    }

    pub fn string(&mut self, text: &str) -> Result<String, NativeError> {
        self.used
            .checked_add(text.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or(NativeError::ResourceLimit)?;
        let mut value = String::new();
        value
            .try_reserve_exact(text.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        self.charge(value.capacity())?;
        value.push_str(text);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_validation_scratch_policy_preserves_exact_checked_formula() {
        assert_eq!(validation_bytes(0, 0, 0).unwrap(), 4096);
        assert_eq!(validation_bytes(2, 3, 3).unwrap(), 4096 + 256 * 18);
        for counts in [(usize::MAX, 0, 0), (0, usize::MAX, 0), (0, 0, usize::MAX)] {
            assert_eq!(
                validation_bytes(counts.0, counts.1, counts.2),
                Err(NativeError::ResourceLimit)
            );
        }
    }

    #[test]
    fn exact_byte_cap_and_overflow_preserve_the_meter() {
        let mut budget = Budget::new(MAX_BYTES - 1).unwrap();
        assert_eq!(budget.charge(2), Err(NativeError::ResourceLimit));
        budget.charge(1).unwrap();
        assert_eq!(budget.charge(usize::MAX), Err(NativeError::ResourceLimit));
        budget.charge(0).unwrap();
        assert!(Budget::new(MAX_BYTES + 1).is_err());
        assert!(Budget::new(0).unwrap().vector::<u64>(usize::MAX).is_err());
    }

    #[test]
    fn actual_vector_and_string_capacities_are_charged() {
        let mut budget = Budget::new(0).unwrap();
        let vector = budget.vector::<u64>(3).unwrap();
        let string = budget.string("capacity").unwrap();
        assert_eq!(
            budget.used,
            vector.capacity() * size_of::<u64>() + string.capacity()
        );
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.used, budget.used);
        budget.charge(MAX_BYTES - budget.used).unwrap();
        assert!(budget.vector::<u8>(1).is_err());
        assert!(budget.string("x").is_err());
        assert_eq!(budget.used, MAX_BYTES);
    }
}
