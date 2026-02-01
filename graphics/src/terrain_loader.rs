use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Vec2, Vec3};
use noren::rdb::primitives::Vertex;
use noren::rdb::terrain::{TerrainChunk, TerrainChunkArtifact, TerrainProjectSettings};

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
    let (vertices, indices, bounds_min, bounds_max) = terrain_chunk_mesh(chunk);
    let content_hash = terrain_chunk_content_hash(chunk);
    TerrainChunkArtifact {
        project_key: project_key.to_string(),
        chunk_coords: chunk.chunk_coords,
        lod: 0,
        bounds_min,
        bounds_max,
        vertex_layout: settings.vertex_layout.clone(),
        vertices,
        indices,
        material_ids: None,
        material_weights: None,
        content_hash,
        mesh_entry: chunk.mesh_entry.clone(),
    }
}

fn terrain_chunk_mesh(chunk: &TerrainChunk) -> (Vec<Vertex>, Vec<u32>, [f32; 3], [f32; 3]) {
    let grid_x = chunk.tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = chunk.tiles_per_chunk[1].saturating_add(1).max(1);
    let tile_size = chunk.tile_size.max(0.0001);
    let mut vertices = Vec::with_capacity((grid_x * grid_y) as usize);
    let mut min_bounds = [f32::MAX; 3];
    let mut max_bounds = [f32::MIN; 3];

    for y in 0..grid_y {
        for x in 0..grid_x {
            let position = [
                x as f32 * tile_size,
                y as f32 * tile_size,
                chunk_height_sample(chunk, x, y),
            ];
            let normal = estimate_chunk_normal(chunk, x, y, tile_size);
            let uv = [
                x as f32 / grid_x.saturating_sub(1).max(1) as f32,
                y as f32 / grid_y.saturating_sub(1).max(1) as f32,
            ];
            update_bounds(&mut min_bounds, &mut max_bounds, &position);
            vertices.push(Vertex {
                position,
                normal,
                tangent: [1.0, 0.0, 0.0, 1.0],
                uv,
                color: [1.0, 1.0, 1.0, 1.0],
                joint_indices: [0; 4],
                joint_weights: [0.0; 4],
            });
        }
    }

    let mut indices = Vec::new();
    for y in 0..grid_y.saturating_sub(1) {
        for x in 0..grid_x.saturating_sub(1) {
            let i0 = (y * grid_x + x) as u32;
            let i1 = (y * grid_x + x + 1) as u32;
            let i2 = ((y + 1) * grid_x + x) as u32;
            let i3 = ((y + 1) * grid_x + x + 1) as u32;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    (vertices, indices, min_bounds, max_bounds)
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
    let normal = glam::Vec3::new(-dx, -dy, 1.0).normalize_or_zero();
    [normal.x, normal.y, normal.z]
}

fn update_bounds(min_bounds: &mut [f32; 3], max_bounds: &mut [f32; 3], position: &[f32; 3]) {
    for i in 0..3 {
        min_bounds[i] = min_bounds[i].min(position[i]);
        max_bounds[i] = max_bounds[i].max(position[i]);
    }
}

pub fn terrain_chunk_content_hash(chunk: &TerrainChunk) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    chunk.chunk_coords.hash(&mut hasher);
    chunk.tiles_per_chunk.hash(&mut hasher);
    chunk.tile_size.to_bits().hash(&mut hasher);
    for height in &chunk.heights {
        height.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}
