mod config;
mod noise_based;
use crate::registry::BlockStateId;
pub use config::{AquiferConfig, AquiferField, AquiferSamplers};
pub use noise_based::{
    AquiferBlockStates, AquiferError, NoiseBasedAquifer, RegistryAquiferBlockStates,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidStatus {
    pub level: i32,
    pub state: BlockStateId,
}
impl FluidStatus {
    pub const fn at(self, y: i32, air: BlockStateId) -> BlockStateId {
        if y < self.level { self.state } else { air }
    }
}
pub trait FluidPicker: std::fmt::Debug + Send + Sync {
    fn compute(&self, position: [i32; 3]) -> FluidStatus;
}
impl<P: FluidPicker + ?Sized> FluidPicker for &P {
    fn compute(&self, position: [i32; 3]) -> FluidStatus {
        P::compute(self, position)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct GlobalFluidPicker {
    sea_level: i32,
    default_fluid: BlockStateId,
    lava: BlockStateId,
    air: BlockStateId,
    disabled: bool,
}
impl GlobalFluidPicker {
    pub const fn new(
        sea_level: i32,
        default_fluid: BlockStateId,
        lava: BlockStateId,
        air: BlockStateId,
    ) -> Self {
        Self {
            sea_level,
            default_fluid,
            lava,
            air,
            disabled: false,
        }
    }
}
impl FluidPicker for GlobalFluidPicker {
    fn compute(&self, [_, y, _]: [i32; 3]) -> FluidStatus {
        if self.disabled {
            FluidStatus {
                level: -4064,
                state: self.air,
            }
        } else if y < (-54).min(self.sea_level) {
            FluidStatus {
                level: -54,
                state: self.lava,
            }
        } else {
            FluidStatus {
                level: self.sea_level,
                state: self.default_fluid,
            }
        }
    }
}
#[derive(Debug)]
pub struct DisabledAquifer<P> {
    picker: P,
    air: BlockStateId,
}
impl<P: FluidPicker> DisabledAquifer<P> {
    pub const fn new(picker: P, air: BlockStateId) -> Self {
        Self { picker, air }
    }
    pub fn compute_substance(&self, position: [i32; 3], density: f64) -> Option<BlockStateId> {
        if density > 0. {
            None
        } else {
            Some(self.picker.compute(position).at(position[1], self.air))
        }
    }
    pub const fn should_schedule_fluid_update(&self) -> bool {
        false
    }
}
