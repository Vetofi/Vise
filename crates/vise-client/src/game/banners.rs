use super::Game;
use vise_client::{
    banner::{self, BannerLayer},
    containers::BlockEntity,
};

impl Game<'_> {
    pub(super) fn banner_layers_at(&self, pos: [i32; 3]) -> Vec<(String, u32)> {
        let Some(BlockEntity::Banner(b)) = self.world.block_entities.get(&(pos[0], pos[1], pos[2]))
        else {
            return Vec::new();
        };
        layer_textures(&self.stations().banners, &b.layers)
    }
}

pub(super) fn layer_textures(
    banners: &banner::Banners,
    layers: &[BannerLayer],
) -> Vec<(String, u32)> {
    layers
        .iter()
        .map(|l| {
            (
                banners.banner_texture(&l.pattern),
                banner::dye_rgb(&l.color),
            )
        })
        .collect()
}
