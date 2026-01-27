use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Vec2, Vec3};
use noren::rdb::terrain::{TerrainChunkArtifact, TerrainProjectSettings};

use crate::render::environment::terrain::TerrainRenderObject;

#[derive(Clone, Debug)]
pub enum TerrainChunkRef {
    Coords { coords: [i32; 2], lod: u8 },
    ArtifactEntry(String),
}

impl TerrainChunkRef {
    pub fn coords(coords: [i32; 2], lod: u8) -> Self {
        Self::Coords { coords, lod }
    }

    pub fn artifact_entry(entry: impl Into<String>) -> Self {
        Self::ArtifactEntry(entry.into())
    }
}

pub fn terrain_chunk_transform(
    settings: &TerrainProjectSettings,
    chunk_coords: [i32; 2],
    bounds_min: [f32; 3],
) -> Mat4 {
    let chunk_stride = Vec2::new(
        settings.tile_size * settings.tiles_per_chunk[0] as f32,
        settings.tile_size * settings.tiles_per_chunk[1] as f32,
    );
    let grid_origin = Vec3::new(
        settings.world_bounds_min[0] + chunk_coords[0] as f32 * chunk_stride.x,
        settings.world_bounds_min[1] + chunk_coords[1] as f32 * chunk_stride.y,
        settings.world_bounds_min[2],
    );
    let terrain_rotation = Mat4::from_rotation_x(-FRAC_PI_2);
    let bounds_min = Vec3::from(bounds_min);
    Mat4::from_translation(grid_origin) * terrain_rotation * Mat4::from_translation(-bounds_min)
}

pub fn terrain_render_object_from_artifact(
    settings: &TerrainProjectSettings,
    key: String,
    artifact: TerrainChunkArtifact,
) -> TerrainRenderObject {
    let transform = terrain_chunk_transform(settings, artifact.chunk_coords, artifact.bounds_min);
    TerrainRenderObject {
        key,
        artifact,
        transform,
    }
}
