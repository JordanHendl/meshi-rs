use glam::{Mat4, Vec2, Vec3};
use noren::rdb::terrain::{TerrainChunk, TerrainChunkArtifact, TerrainProjectSettings};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::render::environment::terrain::TerrainRenderObject;

#[derive(Clone, Debug)]
pub enum TerrainChunkRef {
    Coords { coords: [i32; 2], lod: u8 },
    ArtifactEntry(String),
    ChunkEntry(String),
}

impl TerrainChunkRef {
    pub fn coords(coords: [i32; 2], lod: u8) -> Self {
        Self::Coords { coords, lod }
    }

    pub fn artifact_entry(entry: impl Into<String>) -> Self {
        Self::ArtifactEntry(entry.into())
    }

    pub fn chunk_entry(entry: impl Into<String>) -> Self {
        Self::ChunkEntry(entry.into())
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
        settings.world_bounds_min[1],
        settings.world_bounds_min[2] + chunk_coords[1] as f32 * chunk_stride.y,
    );
    let _ = Vec3::from(bounds_min);
    Mat4::from_translation(Vec3::new(grid_origin.x, 0.0, grid_origin.z))
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

pub fn terrain_render_object_from_chunk(
    settings: &TerrainProjectSettings,
    project_key: &str,
    key: String,
    chunk: &TerrainChunk,
) -> TerrainRenderObject {
    let artifact = terrain_chunk_artifact_from_chunk(settings, project_key, chunk);
    let transform = terrain_chunk_transform(settings, artifact.chunk_coords, artifact.bounds_min);
    TerrainRenderObject {
        key,
        artifact,
        transform,
    }
}

pub fn terrain_chunk_artifact_from_chunk(
    settings: &TerrainProjectSettings,
    project_key: &str,
    chunk: &TerrainChunk,
) -> TerrainChunkArtifact {
    let content_hash = terrain_chunk_content_hash(chunk);
    let cache_key = TerrainArtifactCacheKey {
        project_key: project_key.to_string(),
        content_hash,
    };
    if let Some(cached) = terrain_artifact_cache()
        .lock()
        .expect("terrain artifact cache lock")
        .get(&cache_key)
        .cloned()
    {
        return cached;
    }

    let grid_x = chunk.tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = chunk.tiles_per_chunk[1].saturating_add(1).max(1);
    let mut normals = Vec::with_capacity((grid_x * grid_y) as usize);
    for y in 0..grid_y {
        for x in 0..grid_x {
            normals.push(estimate_chunk_normal(
                chunk,
                x,
                y,
                chunk.tile_size.max(0.0001),
            ));
        }
    }
    let hole_masks = vec![0u8; (grid_x * grid_y) as usize];
    let (material_ids, material_weights) = terrain_chunk_material_blends(chunk);
    let artifact = TerrainChunkArtifact {
        project_key: project_key.to_string(),
        chunk_coords: chunk.chunk_coords,
        lod: 0,
        bounds_min: chunk.bounds_min,
        bounds_max: chunk.bounds_max,
        grid_size: [grid_x, grid_y],
        sample_spacing: settings.tile_size.max(0.0001),
        heights: chunk.heights.clone(),
        normals,
        hole_masks,
        material_ids,
        material_weights,
        content_hash,
        material_blend_texture: Default::default(),
    };

    terrain_artifact_cache()
        .lock()
        .expect("terrain artifact cache lock")
        .insert(cache_key, artifact.clone());

    artifact
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TerrainArtifactCacheKey {
    project_key: String,
    content_hash: u64,
}

fn terrain_artifact_cache() -> &'static Mutex<HashMap<TerrainArtifactCacheKey, TerrainChunkArtifact>>
{
    static CACHE: OnceLock<Mutex<HashMap<TerrainArtifactCacheKey, TerrainChunkArtifact>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn chunk_height_sample(chunk: &TerrainChunk, x: u32, y: u32) -> f32 {
    let grid_x = chunk.tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = chunk.tiles_per_chunk[1].saturating_add(1).max(1);
    if x >= grid_x || y >= grid_y {
        return 0.0;
    }
    let idx = (y * grid_x + x) as usize;
    chunk.heights.get(idx).copied().unwrap_or(0.0)
}

fn estimate_chunk_normal(chunk: &TerrainChunk, x: u32, y: u32, tile_size: f32) -> [f32; 3] {
    let grid_x = chunk.tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = chunk.tiles_per_chunk[1].saturating_add(1).max(1);
    let left = x.saturating_sub(1);
    let right = (x + 1).min(grid_x.saturating_sub(1));
    let down = y.saturating_sub(1);
    let up = (y + 1).min(grid_y.saturating_sub(1));
    let h_l = chunk_height_sample(chunk, left, y);
    let h_r = chunk_height_sample(chunk, right, y);
    let h_d = chunk_height_sample(chunk, x, down);
    let h_u = chunk_height_sample(chunk, x, up);
    let h_c = chunk_height_sample(chunk, x, y);
    let dx = if x == 0 {
        (h_r - h_c) / tile_size
    } else if x == grid_x.saturating_sub(1) {
        (h_c - h_l) / tile_size
    } else {
        (h_r - h_l) / (2.0 * tile_size)
    };
    let dy = if y == 0 {
        (h_u - h_c) / tile_size
    } else if y == grid_y.saturating_sub(1) {
        (h_c - h_d) / tile_size
    } else {
        (h_u - h_d) / (2.0 * tile_size)
    };
    let normal = glam::Vec3::new(-dx, 1.0, -dy).normalize_or_zero();
    [normal.x, normal.y, normal.z]
}

pub fn terrain_chunk_content_hash(chunk: &TerrainChunk) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    chunk.chunk_coords.hash(&mut hasher);
    chunk.tiles_per_chunk.hash(&mut hasher);
    chunk.tile_size.to_bits().hash(&mut hasher);
    chunk.bounds_min.iter().for_each(|value| value.to_bits().hash(&mut hasher));
    chunk.bounds_max.iter().for_each(|value| value.to_bits().hash(&mut hasher));
    for tile in &chunk.tiles {
        tile.tile_id.hash(&mut hasher);
        tile.flags.hash(&mut hasher);
    }
    for height in &chunk.heights {
        height.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn terrain_chunk_material_blends(
    chunk: &TerrainChunk,
) -> (Option<Vec<[u32; 4]>>, Option<Vec<[f32; 4]>>) {
    let grid_x = chunk.tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = chunk.tiles_per_chunk[1].saturating_add(1).max(1);
    if grid_x == 0 || grid_y == 0 {
        return (None, None);
    }

    let mut material_ids = Vec::with_capacity((grid_x * grid_y) as usize);
    let mut material_weights = Vec::with_capacity((grid_x * grid_y) as usize);

    let max_tile_x = chunk.tiles_per_chunk[0].saturating_sub(1);
    let max_tile_y = chunk.tiles_per_chunk[1].saturating_sub(1);

    for y in 0..grid_y {
        for x in 0..grid_x {
            let tile_x = x.min(max_tile_x);
            let tile_y = y.min(max_tile_y);
            let tile_id = chunk
                .tile_at(tile_x as i32, tile_y as i32)
                .map(|tile| tile.tile_id)
                .unwrap_or(0);
            material_ids.push([tile_id, 0, 0, 0]);
            material_weights.push([1.0, 0.0, 0.0, 0.0]);
        }
    }

    (Some(material_ids), Some(material_weights))
}
