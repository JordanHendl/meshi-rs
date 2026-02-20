#[derive(Clone, Copy, Debug)]
pub struct TerrainClipmapResourceSettings {
    pub tile_resolution: [u32; 2],
}

impl Default for TerrainClipmapResourceSettings {
    fn default() -> Self {
        Self {
            tile_resolution: [65, 65],
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerrainClipmapSettings {
    pub surface: TerrainClipmapResourceSettings,
    pub material: TerrainClipmapResourceSettings,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainRenderSettings {
    pub enabled: bool,
    pub patch_size: f32,
    pub lod_levels: u32,
    pub clipmap_resolution: u32,
    pub max_tiles: u32,
    pub clipmap: TerrainClipmapSettings,
}

impl Default for TerrainRenderSettings {
    fn default() -> Self {
        let clipmap_resolution = 8;
        Self {
            enabled: false,
            patch_size: 64.0,
            lod_levels: 4,
            clipmap_resolution,
            max_tiles: clipmap_resolution * clipmap_resolution,
            clipmap: TerrainClipmapSettings::default(),
        }
    }
}
