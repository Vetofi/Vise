use super::{DecodeError, Object, int_range, required};
use vise_core::random::{InvalidBound, RandomSource};
use serde_json::value::RawValue;

#[derive(Clone, Debug)]
pub struct WeightedList<T> {
    entries: Vec<(T, i32)>,
    cumulative: Vec<i32>,
    total: i32,
}
impl<T> WeightedList<T> {
    pub fn new(entries: Vec<(T, i32)>) -> Result<Self, DecodeError> {
        Self::build(entries, false)
    }
    fn build(entries: Vec<(T, i32)>, allow_empty: bool) -> Result<Self, DecodeError> {
        if entries.len() > 8192 {
            return Err(DecodeError::Field("weighted list size"));
        }
        let mut total = 0_i32;
        let mut cumulative = Vec::with_capacity(entries.len());
        for (_, weight) in &entries {
            if *weight < 0 {
                return Err(DecodeError::Field("negative weight"));
            }
            total = total
                .checked_add(*weight)
                .ok_or(DecodeError::Field("total weight"))?;
            cumulative.push(total);
        }
        if total == 0 && !allow_empty {
            return Err(DecodeError::Field("empty weighted list"));
        }
        Ok(Self {
            entries,
            cumulative,
            total,
        })
    }
    pub fn entries(&self) -> &[(T, i32)] {
        &self.entries
    }
    pub fn sample(&self, random: &mut impl RandomSource) -> Result<&T, InvalidBound> {
        let index = random.next_int_bound(self.total)?;
        let entry = self.cumulative.partition_point(|end| *end <= index);
        Ok(&self.entries[entry].0)
    }
    pub(crate) fn decode(
        raw: &RawValue,
        decode: impl FnMut(&RawValue) -> Result<T, DecodeError>,
    ) -> Result<Self, DecodeError> {
        Self::decode_inner(raw, decode, false)
    }
    fn decode_inner(
        raw: &RawValue,
        mut decode: impl FnMut(&RawValue) -> Result<T, DecodeError>,
        allow_empty: bool,
    ) -> Result<Self, DecodeError> {
        let entries: Vec<Box<RawValue>> = serde_json::from_str(raw.get())?;
        if entries.len() > 8192 {
            return Err(DecodeError::Field("weighted list size"));
        }
        Self::build(
            entries
                .iter()
                .map(|raw| {
                    let object: Object = serde_json::from_str(raw.get())?;
                    let data = decode(required(&object, "data")?)?;
                    let weight = int_range(required(&object, "weight")?, "weight", 0, i32::MAX)?;
                    Ok((data, weight))
                })
                .collect::<Result<_, DecodeError>>()?,
            allow_empty,
        )
    }
}

#[derive(Clone, Debug)]
pub struct OptionalWeightedList<T>(WeightedList<T>);
impl<T> OptionalWeightedList<T> {
    pub fn new(entries: Vec<(T, i32)>) -> Result<Self, DecodeError> {
        WeightedList::build(entries, true).map(Self)
    }
    pub fn entries(&self) -> &[(T, i32)] {
        self.0.entries()
    }
    pub fn sample(&self, random: &mut impl RandomSource) -> Result<Option<&T>, InvalidBound> {
        if self.0.total == 0 {
            Ok(None)
        } else {
            self.0.sample(random).map(Some)
        }
    }
    pub(crate) fn decode(
        raw: &RawValue,
        decode: impl FnMut(&RawValue) -> Result<T, DecodeError>,
    ) -> Result<Self, DecodeError> {
        WeightedList::decode_inner(raw, decode, true).map(Self)
    }
}
