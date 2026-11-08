use super::{BlockStateDefinition, Variant, VariantSet, bind::SelectedModels};
use crate::json::{Result, error};
use vise_core::random::RandomSource;

#[derive(Clone, Debug)]
enum Choice<T> {
    Single(T),
    Weighted {
        variants: Vec<T>,
        ends: Vec<i32>,
        total: i32,
    },
}
impl Choice<Variant> {
    fn new(value: &VariantSet) -> Result<Self> {
        match value {
            VariantSet::Single(value) => Ok(Self::Single(value.clone())),
            VariantSet::Weighted(entries) => {
                let mut total = 0i32;
                let mut ends = Vec::with_capacity(entries.len());
                let mut variants = Vec::with_capacity(entries.len());
                for (variant, weight) in entries {
                    if *weight <= 0 {
                        return Err(error("nonpositive model weight"));
                    }
                    total = total
                        .checked_add(*weight)
                        .ok_or_else(|| error("model weight overflow"))?;
                    ends.push(total);
                    variants.push(variant.clone());
                }
                if total == 0 {
                    return Err(error("empty weighted model"));
                }
                Ok(Self::Weighted {
                    variants,
                    ends,
                    total,
                })
            }
        }
    }
}
impl<T> Choice<T> {
    fn map<U>(&self, f: &mut impl FnMut(&T) -> U) -> Choice<U> {
        match self {
            Self::Single(value) => Choice::Single(f(value)),
            Self::Weighted {
                variants,
                ends,
                total,
            } => Choice::Weighted {
                variants: variants.iter().map(&mut *f).collect(),
                ends: ends.clone(),
                total: *total,
            },
        }
    }
    fn select(&self, random: &mut impl RandomSource) -> &T {
        match self {
            Self::Single(value) => value,
            Self::Weighted {
                variants,
                ends,
                total,
            } => {
                let index = random
                    .next_int_bound(*total)
                    .expect("validated positive model weight");
                &variants[ends.partition_point(|end| *end <= index)]
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct CompiledChoices<T = Variant> {
    variants: Vec<Choice<T>>,
    multipart: Vec<Choice<T>>,
}
impl CompiledChoices {
    pub fn new(definition: &BlockStateDefinition) -> Result<Self> {
        Ok(Self {
            variants: definition
                .variants
                .iter()
                .flatten()
                .map(|(_, v)| Choice::new(v))
                .collect::<Result<_>>()?,
            multipart: definition
                .multipart
                .iter()
                .flatten()
                .map(|s| Choice::new(&s.variant))
                .collect::<Result<_>>()?,
        })
    }
}
impl<T> CompiledChoices<T> {
    pub fn map<U>(&self, mut f: impl FnMut(&T) -> U) -> CompiledChoices<U> {
        CompiledChoices {
            variants: self.variants.iter().map(|c| c.map(&mut f)).collect(),
            multipart: self.multipart.iter().map(|c| c.map(&mut f)).collect(),
        }
    }
    pub fn particle_variant(&self, selected: &SelectedModels) -> Option<&T> {
        let choice = match selected {
            SelectedModels::Variant(index) => self.variants.get(*index)?,
            SelectedModels::Multipart(_) => self.multipart.first()?,
        };
        Some(match choice {
            Choice::Single(value) => value,
            Choice::Weighted { variants, .. } => variants.first()?,
        })
    }
    pub fn collect<'a>(
        &'a self,
        selected: &SelectedModels,
        random: &mut impl RandomSource,
        output: &mut Vec<&'a T>,
    ) -> Result<()> {
        self.for_each(selected, random, |v| output.push(v))
    }
    pub fn for_each<'a>(
        &'a self,
        selected: &SelectedModels,
        random: &mut impl RandomSource,
        mut f: impl FnMut(&'a T),
    ) -> Result<()> {
        match selected {
            SelectedModels::Variant(index) => {
                let choice = self
                    .variants
                    .get(*index)
                    .ok_or_else(|| error("invalid variant selection"))?;
                f(choice.select(random));
            }
            SelectedModels::Multipart(indices) => {
                if indices.iter().any(|index| *index >= self.multipart.len()) {
                    return Err(error("invalid multipart selection"));
                }
                let seed = random.next_long();
                for index in indices {
                    random.set_seed(seed);
                    f(self.multipart[*index].select(random));
                }
            }
        }
        Ok(())
    }
}
