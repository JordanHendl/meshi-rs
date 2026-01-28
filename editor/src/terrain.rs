use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const N_LAYERS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord2 {
    pub cx: i32,
    pub cz: i32,
    pub lod: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ChunkCoord3 {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub lod: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeightChunk {
    pub size: u32,
    pub heights: Vec<f32>,
    pub material_weights: Vec<[u8; N_LAYERS]>,
    pub slope: Option<Vec<f32>>,
    pub curvature: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min[0] <= other.max[0]
            && self.max[0] >= other.min[0]
            && self.min[1] <= other.max[1]
            && self.max[1] >= other.min[1]
            && self.min[2] <= other.max[2]
            && self.max[2] >= other.min[2]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FeatureKind {
    Cave,
    Overhang,
    Cliff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureParams {
    Cave(CaveParams),
    Overhang(OverhangParams),
    Cliff(CliffParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVolume {
    pub id: u64,
    pub shape: Aabb,
    pub kind: FeatureKind,
    pub params: FeatureParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DensityChunk {
    pub dims: [u32; 3],
    pub density: Vec<f32>,
    pub material_id: Option<Vec<u16>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshChunk {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainGenSettings {
    pub world_scale: f32,
    pub cache_root: PathBuf,
    pub height: HeightSettings,
    pub density: DensitySettings,
    pub materials: MaterialSettings,
    pub determinism_epsilon: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeightSettings {
    pub chunk_samples: u32,
    pub border_samples: u32,
    pub sample_spacing: f32,
    pub height_scale: f32,
    pub noise: NoiseSettings,
    pub warp: Option<WarpSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DensitySettings {
    pub chunk_dims: [u32; 3],
    pub border_samples: u32,
    pub voxel_size: f32,
    pub iso_level: f32,
    pub cave: CaveParams,
    pub overhang: OverhangParams,
    pub cliff: CliffParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialSettings {
    pub layers: [MaterialLayer; N_LAYERS],
    pub slope_rock_threshold: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MaterialLayer {
    pub min_height: f32,
    pub max_height: f32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseSettings {
    pub frequency: f32,
    pub octaves: u32,
    pub lacunarity: f32,
    pub gain: f32,
    pub ridged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarpSettings {
    pub frequency: f32,
    pub amplitude: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaveParams {
    pub frequency: f32,
    pub threshold: f32,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverhangParams {
    pub frequency: f32,
    pub strength: f32,
    pub max_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliffParams {
    pub frequency: f32,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionManifest {
    pub region_bounds: Aabb,
    pub height_chunks: Vec<ChunkCoord2>,
    pub density_chunks: Vec<ChunkCoord3>,
    pub mesh_chunks: Vec<ChunkCoord3>,
    pub chunk_bounds: HashMap<String, Aabb>,
}

#[derive(Debug, Clone)]
pub struct BuildArtifact {
    pub output_root: PathBuf,
    pub manifest: RegionManifest,
}

#[derive(Clone)]
pub struct HeightSampler {
    seed: WorldSeed,
    settings: HeightSettings,
}

impl HeightSampler {
    pub fn new(seed: WorldSeed, settings: HeightSettings) -> Self {
        Self { seed, settings }
    }

    pub fn height_at_world(&self, x: f32, z: f32) -> f32 {
        let mut sample_x = x / self.settings.sample_spacing;
        let mut sample_z = z / self.settings.sample_spacing;
        if let Some(warp) = &self.settings.warp {
            let warp_val_x = fbm2(
                self.seed,
                sample_x * warp.frequency,
                sample_z * warp.frequency,
                3,
                2.0,
                0.5,
                false,
            );
            let warp_val_z = fbm2(
                self.seed,
                (sample_x + 13.7) * warp.frequency,
                (sample_z - 9.2) * warp.frequency,
                3,
                2.0,
                0.5,
                false,
            );
            sample_x += warp_val_x * warp.amplitude;
            sample_z += warp_val_z * warp.amplitude;
        }
        let mut height = fbm2(
            self.seed,
            sample_x * self.settings.noise.frequency,
            sample_z * self.settings.noise.frequency,
            self.settings.noise.octaves,
            self.settings.noise.lacunarity,
            self.settings.noise.gain,
            self.settings.noise.ridged,
        );
        height = height.clamp(-1.0, 1.0);
        height * self.settings.height_scale
    }
}

pub fn generate_height_chunk(
    seed: WorldSeed,
    coord: ChunkCoord2,
    settings: &TerrainGenSettings,
) -> HeightChunk {
    let sampler = HeightSampler::new(seed, settings.height.clone());
    let border = settings.height.border_samples as i32;
    let size = settings.height.chunk_samples as i32 + border * 2;
    let stride = (settings.height.chunk_samples - 1) as f32 * settings.height.sample_spacing;
    let origin_x = coord.cx as f32 * stride;
    let origin_z = coord.cz as f32 * stride;

    let mut heights = vec![0.0; (size * size) as usize];
    for z in 0..size {
        for x in 0..size {
            let world_x = origin_x + (x - border) as f32 * settings.height.sample_spacing;
            let world_z = origin_z + (z - border) as f32 * settings.height.sample_spacing;
            let h = sampler.height_at_world(world_x, world_z);
            heights[(z * size + x) as usize] = h;
        }
    }

    let slope = compute_slope_map(size as u32, &heights, settings.height.sample_spacing);
    let material_weights = compute_material_weights(
        size as u32,
        &heights,
        &slope,
        &settings.materials,
    );

    HeightChunk {
        size: size as u32,
        heights,
        material_weights,
        slope: Some(slope),
        curvature: None,
    }
}

pub fn detect_feature_volumes(
    _seed: WorldSeed,
    _region_bounds: Aabb,
    _settings: &TerrainGenSettings,
) -> Vec<FeatureVolume> {
    Vec::new()
}

pub fn volumes_overlapping_aabb(
    volumes: &[FeatureVolume],
    aabb_world: Aabb,
) -> Vec<FeatureVolume> {
    volumes
        .iter()
        .filter(|volume| volume.shape.intersects(&aabb_world))
        .cloned()
        .collect()
}

pub fn generate_density_chunk(
    seed: WorldSeed,
    coord: ChunkCoord3,
    volumes_in_range: &[FeatureVolume],
    settings: &TerrainGenSettings,
    height_sampler: &HeightSampler,
) -> DensityChunk {
    let border = settings.density.border_samples as i32;
    let dims = settings.density.chunk_dims;
    let dims_with_border = [
        dims[0] + settings.density.border_samples * 2,
        dims[1] + settings.density.border_samples * 2,
        dims[2] + settings.density.border_samples * 2,
    ];
    let stride_x = (dims[0] - 1) as f32 * settings.density.voxel_size;
    let stride_y = (dims[1] - 1) as f32 * settings.density.voxel_size;
    let stride_z = (dims[2] - 1) as f32 * settings.density.voxel_size;
    let origin = [
        coord.cx as f32 * stride_x,
        coord.cy as f32 * stride_y,
        coord.cz as f32 * stride_z,
    ];

    let mut density =
        vec![0.0; (dims_with_border[0] * dims_with_border[1] * dims_with_border[2]) as usize];
    for z in 0..dims_with_border[2] as i32 {
        for y in 0..dims_with_border[1] as i32 {
            for x in 0..dims_with_border[0] as i32 {
                let world_x =
                    origin[0] + (x - border) as f32 * settings.density.voxel_size;
                let world_y =
                    origin[1] + (y - border) as f32 * settings.density.voxel_size;
                let world_z =
                    origin[2] + (z - border) as f32 * settings.density.voxel_size;
                let height = height_sampler.height_at_world(world_x, world_z);
                let mut d = height - world_y;
                for volume in volumes_in_range {
                    if point_in_aabb([world_x, world_y, world_z], volume.shape) {
                        d = apply_feature_density(seed, volume, d, [world_x, world_y, world_z]);
                    }
                }
                density[index_3d(
                    x as u32,
                    y as u32,
                    z as u32,
                    dims_with_border,
                )] = d;
            }
        }
    }

    DensityChunk {
        dims: dims_with_border,
        density,
        material_id: None,
    }
}

pub fn mesh_density_chunk(density_chunk: &DensityChunk, settings: &TerrainGenSettings) -> MeshChunk {
    let dims = density_chunk.dims;
    let border = settings.density.border_samples as i32;
    let voxel = settings.density.voxel_size;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let mut next_index = 0u32;

    let max_x = dims[0] as i32 - 1 - border;
    let max_y = dims[1] as i32 - 1 - border;
    let max_z = dims[2] as i32 - 1 - border;

    for z in border..max_z {
        for y in border..max_y {
            for x in border..max_x {
                let cube = sample_cube(density_chunk, x, y, z);
                let cube_index = cube_index(&cube, settings.density.iso_level);
                if EDGE_TABLE[cube_index] == 0 {
                    continue;
                }
                let mut vert_list = [[0.0; 3]; 12];
                if EDGE_TABLE[cube_index] & 1 != 0 {
                    vert_list[0] = vertex_interp(
                        settings.density.iso_level,
                        cube[0],
                        cube[1],
                        [x, y, z],
                        [x + 1, y, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 2 != 0 {
                    vert_list[1] = vertex_interp(
                        settings.density.iso_level,
                        cube[1],
                        cube[2],
                        [x + 1, y, z],
                        [x + 1, y, z + 1],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 4 != 0 {
                    vert_list[2] = vertex_interp(
                        settings.density.iso_level,
                        cube[2],
                        cube[3],
                        [x + 1, y, z + 1],
                        [x, y, z + 1],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 8 != 0 {
                    vert_list[3] = vertex_interp(
                        settings.density.iso_level,
                        cube[3],
                        cube[0],
                        [x, y, z + 1],
                        [x, y, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 16 != 0 {
                    vert_list[4] = vertex_interp(
                        settings.density.iso_level,
                        cube[4],
                        cube[5],
                        [x, y + 1, z],
                        [x + 1, y + 1, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 32 != 0 {
                    vert_list[5] = vertex_interp(
                        settings.density.iso_level,
                        cube[5],
                        cube[6],
                        [x + 1, y + 1, z],
                        [x + 1, y + 1, z + 1],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 64 != 0 {
                    vert_list[6] = vertex_interp(
                        settings.density.iso_level,
                        cube[6],
                        cube[7],
                        [x + 1, y + 1, z + 1],
                        [x, y + 1, z + 1],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 128 != 0 {
                    vert_list[7] = vertex_interp(
                        settings.density.iso_level,
                        cube[7],
                        cube[4],
                        [x, y + 1, z + 1],
                        [x, y + 1, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 256 != 0 {
                    vert_list[8] = vertex_interp(
                        settings.density.iso_level,
                        cube[0],
                        cube[4],
                        [x, y, z],
                        [x, y + 1, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 512 != 0 {
                    vert_list[9] = vertex_interp(
                        settings.density.iso_level,
                        cube[1],
                        cube[5],
                        [x + 1, y, z],
                        [x + 1, y + 1, z],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 1024 != 0 {
                    vert_list[10] = vertex_interp(
                        settings.density.iso_level,
                        cube[2],
                        cube[6],
                        [x + 1, y, z + 1],
                        [x + 1, y + 1, z + 1],
                        voxel,
                    );
                }
                if EDGE_TABLE[cube_index] & 2048 != 0 {
                    vert_list[11] = vertex_interp(
                        settings.density.iso_level,
                        cube[3],
                        cube[7],
                        [x, y, z + 1],
                        [x, y + 1, z + 1],
                        voxel,
                    );
                }

                let tri = TRI_TABLE[cube_index];
                let mut idx = 0;
                while tri[idx] != -1 {
                    let a = vert_list[tri[idx] as usize];
                    let b = vert_list[tri[idx + 1] as usize];
                    let c = vert_list[tri[idx + 2] as usize];
                    positions.push(a);
                    positions.push(b);
                    positions.push(c);
                    normals.push(compute_normal(density_chunk, a, voxel));
                    normals.push(compute_normal(density_chunk, b, voxel));
                    normals.push(compute_normal(density_chunk, c, voxel));
                    indices.push(next_index);
                    indices.push(next_index + 1);
                    indices.push(next_index + 2);
                    next_index += 3;
                    idx += 3;
                }
            }
        }
    }

    MeshChunk {
        positions,
        normals,
        indices,
    }
}

pub fn build_region(
    seed: WorldSeed,
    region_bounds: Aabb,
    settings: &TerrainGenSettings,
    authored_volumes: &[FeatureVolume],
) -> io::Result<BuildArtifact> {
    let output_root = settings.cache_root.clone();
    fs::create_dir_all(&output_root)?;
    let mut height_chunks = Vec::new();
    let mut density_chunks = Vec::new();
    let mut mesh_chunks = Vec::new();
    let mut chunk_bounds = HashMap::new();
    let sampler = HeightSampler::new(seed, settings.height.clone());

    let height_coords = collect_height_chunks(region_bounds, settings);
    for coord in &height_coords {
        let chunk = generate_height_chunk(seed, *coord, settings);
        write_height_chunk(&output_root, coord, &chunk)?;
        height_chunks.push(*coord);
        let bounds = height_chunk_aabb(coord, settings);
        chunk_bounds.insert(format!("height:{}:{}:{}", coord.cx, coord.cz, coord.lod), bounds);
    }

    let volume_candidates = volumes_overlapping_aabb(authored_volumes, region_bounds);
    let mut density_coord_set = BTreeSet::new();
    for volume in &volume_candidates {
        for coord in collect_density_chunks(volume.shape, settings) {
            density_coord_set.insert(coord);
        }
    }

    for coord in density_coord_set {
        let chunk_aabb = density_chunk_aabb(&coord, settings);
        let volumes_in_range = volumes_overlapping_aabb(&volume_candidates, chunk_aabb);
        if volumes_in_range.is_empty() {
            continue;
        }
        let density_chunk = generate_density_chunk(seed, coord, &volumes_in_range, settings, &sampler);
        write_density_chunk(&output_root, &coord, &density_chunk)?;
        let mesh = mesh_density_chunk(&density_chunk, settings);
        write_mesh_chunk(&output_root, &coord, &mesh)?;
        density_chunks.push(coord);
        mesh_chunks.push(coord);
        chunk_bounds.insert(format!("density:{}:{}:{}:{}", coord.cx, coord.cy, coord.cz, coord.lod), chunk_aabb);
    }

    let manifest = RegionManifest {
        region_bounds,
        height_chunks,
        density_chunks,
        mesh_chunks,
        chunk_bounds,
    };
    let manifest_path = output_root.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    Ok(BuildArtifact {
        output_root,
        manifest,
    })
}

fn collect_height_chunks(region: Aabb, settings: &TerrainGenSettings) -> Vec<ChunkCoord2> {
    let stride = (settings.height.chunk_samples - 1) as f32 * settings.height.sample_spacing;
    let min_cx = (region.min[0] / stride).floor() as i32;
    let max_cx = (region.max[0] / stride).ceil() as i32;
    let min_cz = (region.min[2] / stride).floor() as i32;
    let max_cz = (region.max[2] / stride).ceil() as i32;
    let mut coords = Vec::new();
    for cz in min_cz..=max_cz {
        for cx in min_cx..=max_cx {
            coords.push(ChunkCoord2 { cx, cz, lod: 0 });
        }
    }
    coords
}

fn collect_density_chunks(region: Aabb, settings: &TerrainGenSettings) -> Vec<ChunkCoord3> {
    let stride_x = (settings.density.chunk_dims[0] - 1) as f32 * settings.density.voxel_size;
    let stride_y = (settings.density.chunk_dims[1] - 1) as f32 * settings.density.voxel_size;
    let stride_z = (settings.density.chunk_dims[2] - 1) as f32 * settings.density.voxel_size;
    let min_cx = (region.min[0] / stride_x).floor() as i32;
    let max_cx = (region.max[0] / stride_x).ceil() as i32;
    let min_cy = (region.min[1] / stride_y).floor() as i32;
    let max_cy = (region.max[1] / stride_y).ceil() as i32;
    let min_cz = (region.min[2] / stride_z).floor() as i32;
    let max_cz = (region.max[2] / stride_z).ceil() as i32;
    let mut coords = Vec::new();
    for cz in min_cz..=max_cz {
        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                coords.push(ChunkCoord3 {
                    cx,
                    cy,
                    cz,
                    lod: 0,
                });
            }
        }
    }
    coords
}

fn height_chunk_aabb(coord: &ChunkCoord2, settings: &TerrainGenSettings) -> Aabb {
    let stride = (settings.height.chunk_samples - 1) as f32 * settings.height.sample_spacing;
    let min = [
        coord.cx as f32 * stride,
        -settings.height.height_scale,
        coord.cz as f32 * stride,
    ];
    let max = [
        (coord.cx + 1) as f32 * stride,
        settings.height.height_scale,
        (coord.cz + 1) as f32 * stride,
    ];
    Aabb { min, max }
}

fn density_chunk_aabb(coord: &ChunkCoord3, settings: &TerrainGenSettings) -> Aabb {
    let stride_x = (settings.density.chunk_dims[0] - 1) as f32 * settings.density.voxel_size;
    let stride_y = (settings.density.chunk_dims[1] - 1) as f32 * settings.density.voxel_size;
    let stride_z = (settings.density.chunk_dims[2] - 1) as f32 * settings.density.voxel_size;
    let min = [
        coord.cx as f32 * stride_x,
        coord.cy as f32 * stride_y,
        coord.cz as f32 * stride_z,
    ];
    let max = [
        (coord.cx + 1) as f32 * stride_x,
        (coord.cy + 1) as f32 * stride_y,
        (coord.cz + 1) as f32 * stride_z,
    ];
    Aabb { min, max }
}

fn write_height_chunk(root: &Path, coord: &ChunkCoord2, chunk: &HeightChunk) -> io::Result<()> {
    let height_dir = root.join("height");
    let weights_dir = root.join("weights");
    fs::create_dir_all(&height_dir)?;
    fs::create_dir_all(&weights_dir)?;
    let height_path = height_dir.join(format!("{}_{}_{}.bin", coord.cx, coord.cz, coord.lod));
    let weights_path = weights_dir.join(format!("{}_{}_{}.bin", coord.cx, coord.cz, coord.lod));
    write_f32_slice(&height_path, &chunk.heights)?;
    write_u8_slice(
        &weights_path,
        &chunk
            .material_weights
            .iter()
            .flat_map(|weights| weights.iter().copied())
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

fn write_density_chunk(root: &Path, coord: &ChunkCoord3, chunk: &DensityChunk) -> io::Result<()> {
    let density_dir = root.join("density");
    fs::create_dir_all(&density_dir)?;
    let density_path =
        density_dir.join(format!("{}_{}_{}_{}.bin", coord.cx, coord.cy, coord.cz, coord.lod));
    write_f32_slice(&density_path, &chunk.density)?;
    Ok(())
}

fn write_mesh_chunk(root: &Path, coord: &ChunkCoord3, chunk: &MeshChunk) -> io::Result<()> {
    let mesh_dir = root.join("mesh");
    fs::create_dir_all(&mesh_dir)?;
    let mesh_path =
        mesh_dir.join(format!("{}_{}_{}_{}.meshbin", coord.cx, coord.cy, coord.cz, coord.lod));
    let mut file = fs::File::create(mesh_path)?;
    file.write_all(&(chunk.positions.len() as u32).to_le_bytes())?;
    file.write_all(&(chunk.indices.len() as u32).to_le_bytes())?;
    for pos in &chunk.positions {
        for component in pos {
            file.write_all(&component.to_le_bytes())?;
        }
    }
    for normal in &chunk.normals {
        for component in normal {
            file.write_all(&component.to_le_bytes())?;
        }
    }
    for index in &chunk.indices {
        file.write_all(&index.to_le_bytes())?;
    }
    Ok(())
}

fn write_f32_slice(path: &Path, slice: &[f32]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    for value in slice {
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn write_u8_slice(path: &Path, slice: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(slice)?;
    Ok(())
}

fn compute_slope_map(size: u32, heights: &[f32], spacing: f32) -> Vec<f32> {
    let mut slope = vec![0.0; (size * size) as usize];
    let size_i = size as i32;
    for z in 0..size_i {
        for x in 0..size_i {
            let left = heights[index_2d((x - 1).clamp(0, size_i - 1) as u32, z as u32, size)];
            let right = heights[index_2d((x + 1).clamp(0, size_i - 1) as u32, z as u32, size)];
            let down = heights[index_2d(x as u32, (z - 1).clamp(0, size_i - 1) as u32, size)];
            let up = heights[index_2d(x as u32, (z + 1).clamp(0, size_i - 1) as u32, size)];
            let dx = (right - left) / (2.0 * spacing);
            let dz = (up - down) / (2.0 * spacing);
            slope[index_2d(x as u32, z as u32, size)] = (dx * dx + dz * dz).sqrt();
        }
    }
    slope
}

fn compute_material_weights(
    size: u32,
    heights: &[f32],
    slope: &[f32],
    settings: &MaterialSettings,
) -> Vec<[u8; N_LAYERS]> {
    let mut weights = vec![[0u8; N_LAYERS]; (size * size) as usize];
    for z in 0..size {
        for x in 0..size {
            let index = index_2d(x, z, size);
            let height = heights[index];
            let slope_val = slope[index];
            let mut layer_weights = [0.0; N_LAYERS];
            for (idx, layer) in settings.layers.iter().enumerate() {
                if height >= layer.min_height && height < layer.max_height {
                    layer_weights[idx] = layer.weight;
                }
            }
            if slope_val > settings.slope_rock_threshold {
                layer_weights[1] = layer_weights[1].max(1.0);
            }
            let sum: f32 = layer_weights.iter().sum();
            let sum = if sum <= 0.0 { 1.0 } else { sum };
            let mut packed = [0u8; N_LAYERS];
            for (idx, w) in layer_weights.iter().enumerate() {
                packed[idx] = ((w / sum) * 255.0).clamp(0.0, 255.0) as u8;
            }
            weights[index] = packed;
        }
    }
    weights
}

fn apply_feature_density(seed: WorldSeed, volume: &FeatureVolume, density: f32, pos: [f32; 3]) -> f32 {
    match (&volume.kind, &volume.params) {
        (FeatureKind::Cave, FeatureParams::Cave(params)) => {
            let n = noise3(seed, pos[0] * params.frequency, pos[1] * params.frequency, pos[2] * params.frequency);
            if n > params.threshold {
                density.min(-params.strength.abs())
            } else {
                density
            }
        }
        (FeatureKind::Overhang, FeatureParams::Overhang(params)) => {
            let ridge = noise2(seed, pos[0] * params.frequency, pos[2] * params.frequency).abs();
            if pos[1] > params.max_height {
                density
            } else {
                density + ridge * params.strength
            }
        }
        (FeatureKind::Cliff, FeatureParams::Cliff(params)) => {
            let n = noise2(seed, pos[0] * params.frequency, pos[2] * params.frequency);
            density + n.abs() * params.strength
        }
        _ => density,
    }
}

fn point_in_aabb(point: [f32; 3], aabb: Aabb) -> bool {
    point[0] >= aabb.min[0]
        && point[0] <= aabb.max[0]
        && point[1] >= aabb.min[1]
        && point[1] <= aabb.max[1]
        && point[2] >= aabb.min[2]
        && point[2] <= aabb.max[2]
}

fn index_2d(x: u32, z: u32, size: u32) -> usize {
    (z * size + x) as usize
}

fn index_3d(x: u32, y: u32, z: u32, dims: [u32; 3]) -> usize {
    ((z * dims[1] + y) * dims[0] + x) as usize
}

fn sample_cube(chunk: &DensityChunk, x: i32, y: i32, z: i32) -> [f32; 8] {
    let dims = chunk.dims;
    let ix = x as u32;
    let iy = y as u32;
    let iz = z as u32;
    [
        chunk.density[index_3d(ix, iy, iz, dims)],
        chunk.density[index_3d(ix + 1, iy, iz, dims)],
        chunk.density[index_3d(ix + 1, iy, iz + 1, dims)],
        chunk.density[index_3d(ix, iy, iz + 1, dims)],
        chunk.density[index_3d(ix, iy + 1, iz, dims)],
        chunk.density[index_3d(ix + 1, iy + 1, iz, dims)],
        chunk.density[index_3d(ix + 1, iy + 1, iz + 1, dims)],
        chunk.density[index_3d(ix, iy + 1, iz + 1, dims)],
    ]
}

fn cube_index(cube: &[f32; 8], iso: f32) -> usize {
    let mut idx = 0;
    for (i, value) in cube.iter().enumerate() {
        if *value < iso {
            idx |= 1 << i;
        }
    }
    idx
}

fn vertex_interp(iso: f32, v1: f32, v2: f32, p1: [i32; 3], p2: [i32; 3], voxel: f32) -> [f32; 3] {
    if (iso - v1).abs() < f32::EPSILON {
        return [p1[0] as f32 * voxel, p1[1] as f32 * voxel, p1[2] as f32 * voxel];
    }
    if (iso - v2).abs() < f32::EPSILON {
        return [p2[0] as f32 * voxel, p2[1] as f32 * voxel, p2[2] as f32 * voxel];
    }
    if (v1 - v2).abs() < f32::EPSILON {
        return [p1[0] as f32 * voxel, p1[1] as f32 * voxel, p1[2] as f32 * voxel];
    }
    let t = (iso - v1) / (v2 - v1);
    [
        (p1[0] as f32 + t * (p2[0] - p1[0]) as f32) * voxel,
        (p1[1] as f32 + t * (p2[1] - p1[1]) as f32) * voxel,
        (p1[2] as f32 + t * (p2[2] - p1[2]) as f32) * voxel,
    ]
}

fn compute_normal(chunk: &DensityChunk, pos: [f32; 3], voxel: f32) -> [f32; 3] {
    let fx = pos[0] / voxel;
    let fy = pos[1] / voxel;
    let fz = pos[2] / voxel;
    let dx = sample_density_trilinear(chunk, fx + 1.0, fy, fz)
        - sample_density_trilinear(chunk, fx - 1.0, fy, fz);
    let dy = sample_density_trilinear(chunk, fx, fy + 1.0, fz)
        - sample_density_trilinear(chunk, fx, fy - 1.0, fz);
    let dz = sample_density_trilinear(chunk, fx, fy, fz + 1.0)
        - sample_density_trilinear(chunk, fx, fy, fz - 1.0);
    let mut n = [dx, dy, dz];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        n[0] /= len;
        n[1] /= len;
        n[2] /= len;
    }
    n
}

fn sample_density_trilinear(chunk: &DensityChunk, x: f32, y: f32, z: f32) -> f32 {
    let dims = chunk.dims;
    let x0 = x.floor().clamp(0.0, (dims[0] - 1) as f32) as u32;
    let y0 = y.floor().clamp(0.0, (dims[1] - 1) as f32) as u32;
    let z0 = z.floor().clamp(0.0, (dims[2] - 1) as f32) as u32;
    let x1 = (x0 + 1).min(dims[0] - 1);
    let y1 = (y0 + 1).min(dims[1] - 1);
    let z1 = (z0 + 1).min(dims[2] - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let tz = z - z0 as f32;

    let c000 = chunk.density[index_3d(x0, y0, z0, dims)];
    let c100 = chunk.density[index_3d(x1, y0, z0, dims)];
    let c010 = chunk.density[index_3d(x0, y1, z0, dims)];
    let c110 = chunk.density[index_3d(x1, y1, z0, dims)];
    let c001 = chunk.density[index_3d(x0, y0, z1, dims)];
    let c101 = chunk.density[index_3d(x1, y0, z1, dims)];
    let c011 = chunk.density[index_3d(x0, y1, z1, dims)];
    let c111 = chunk.density[index_3d(x1, y1, z1, dims)];

    let c00 = c000 + (c100 - c000) * tx;
    let c10 = c010 + (c110 - c010) * tx;
    let c01 = c001 + (c101 - c001) * tx;
    let c11 = c011 + (c111 - c011) * tx;
    let c0 = c00 + (c10 - c00) * ty;
    let c1 = c01 + (c11 - c01) * ty;
    c0 + (c1 - c0) * tz
}

fn fbm2(
    seed: WorldSeed,
    x: f32,
    z: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
    ridged: bool,
) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    for _ in 0..octaves {
        let mut n = noise2(seed, x * frequency, z * frequency);
        if ridged {
            n = 1.0 - n.abs();
        }
        value += n * amplitude;
        frequency *= lacunarity;
        amplitude *= gain;
    }
    value
}

fn noise2(seed: WorldSeed, x: f32, z: f32) -> f32 {
    let x0 = x.floor();
    let z0 = z.floor();
    let xf = x - x0;
    let zf = z - z0;
    let u = smoothstep(xf);
    let v = smoothstep(zf);

    let v00 = hash_to_unit(seed, x0 as i32, 0, z0 as i32);
    let v10 = hash_to_unit(seed, (x0 + 1.0) as i32, 0, z0 as i32);
    let v01 = hash_to_unit(seed, x0 as i32, 0, (z0 + 1.0) as i32);
    let v11 = hash_to_unit(seed, (x0 + 1.0) as i32, 0, (z0 + 1.0) as i32);

    let i1 = lerp(v00, v10, u);
    let i2 = lerp(v01, v11, u);
    lerp(i1, i2, v) * 2.0 - 1.0
}

fn noise3(seed: WorldSeed, x: f32, y: f32, z: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let z0 = z.floor();
    let xf = x - x0;
    let yf = y - y0;
    let zf = z - z0;
    let u = smoothstep(xf);
    let v = smoothstep(yf);
    let w = smoothstep(zf);

    let c000 = hash_to_unit(seed, x0 as i32, y0 as i32, z0 as i32);
    let c100 = hash_to_unit(seed, (x0 + 1.0) as i32, y0 as i32, z0 as i32);
    let c010 = hash_to_unit(seed, x0 as i32, (y0 + 1.0) as i32, z0 as i32);
    let c110 = hash_to_unit(seed, (x0 + 1.0) as i32, (y0 + 1.0) as i32, z0 as i32);
    let c001 = hash_to_unit(seed, x0 as i32, y0 as i32, (z0 + 1.0) as i32);
    let c101 = hash_to_unit(seed, (x0 + 1.0) as i32, y0 as i32, (z0 + 1.0) as i32);
    let c011 = hash_to_unit(seed, x0 as i32, (y0 + 1.0) as i32, (z0 + 1.0) as i32);
    let c111 = hash_to_unit(seed, (x0 + 1.0) as i32, (y0 + 1.0) as i32, (z0 + 1.0) as i32);

    let x00 = lerp(c000, c100, u);
    let x10 = lerp(c010, c110, u);
    let x01 = lerp(c001, c101, u);
    let x11 = lerp(c011, c111, u);
    let y0v = lerp(x00, x10, v);
    let y1v = lerp(x01, x11, v);
    lerp(y0v, y1v, w) * 2.0 - 1.0
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash_to_unit(seed: WorldSeed, x: i32, y: i32, z: i32) -> f32 {
    let mut v = seed.0 ^ (x as u64).wrapping_mul(0x9E3779B97F4A7C15);
    v = v.wrapping_add((y as u64).wrapping_mul(0xBF58476D1CE4E5B9));
    v = v.wrapping_add((z as u64).wrapping_mul(0x94D049BB133111EB));
    let hashed = splitmix64(v);
    (hashed as f32 / u64::MAX as f32).clamp(0.0, 1.0)
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[allow(clippy::all)]
const EDGE_TABLE: [u16; 256] = [
    0x000, 0x109, 0x203, 0x30a, 0x406, 0x50f, 0x605, 0x70c, 0x80c, 0x905, 0xa0f, 0xb06,
    0xc0a, 0xd03, 0xe09, 0xf00, 0x190, 0x099, 0x393, 0x29a, 0x596, 0x49f, 0x795, 0x69c,
    0x99c, 0x895, 0xb9f, 0xa96, 0xd9a, 0xc93, 0xf99, 0xe90, 0x230, 0x339, 0x033, 0x13a,
    0x636, 0x73f, 0x435, 0x53c, 0xa3c, 0xb35, 0x83f, 0x936, 0xe3a, 0xf33, 0xc39, 0xd30,
    0x3a0, 0x2a9, 0x1a3, 0x0aa, 0x7a6, 0x6af, 0x5a5, 0x4ac, 0xbac, 0xaa5, 0x9af, 0x8a6,
    0xfaa, 0xea3, 0xda9, 0xca0, 0x460, 0x569, 0x663, 0x76a, 0x066, 0x16f, 0x265, 0x36c,
    0xc6c, 0xd65, 0xe6f, 0xf66, 0x86a, 0x963, 0xa69, 0xb60, 0x5f0, 0x4f9, 0x7f3, 0x6fa,
    0x1f6, 0x0ff, 0x3f5, 0x2fc, 0xdfc, 0xcf5, 0xfff, 0xef6, 0x9fa, 0x8f3, 0xbf9, 0xaf0,
    0x650, 0x759, 0x453, 0x55a, 0x256, 0x35f, 0x055, 0x15c, 0xe5c, 0xf55, 0xc5f, 0xd56,
    0xa5a, 0xb53, 0x859, 0x950, 0x7c0, 0x6c9, 0x5c3, 0x4ca, 0x3c6, 0x2cf, 0x1c5, 0x0cc,
    0xfcc, 0xec5, 0xdcf, 0xcc6, 0xbca, 0xac3, 0x9c9, 0x8c0, 0x8c0, 0x9c9, 0xac3, 0xbca,
    0xcc6, 0xdcf, 0xec5, 0xfcc, 0x0cc, 0x1c5, 0x2cf, 0x3c6, 0x4ca, 0x5c3, 0x6c9, 0x7c0,
    0x950, 0x859, 0xb53, 0xa5a, 0xd56, 0xc5f, 0xf55, 0xe5c, 0x15c, 0x055, 0x35f, 0x256,
    0x55a, 0x453, 0x759, 0x650, 0xaf0, 0xbf9, 0x8f3, 0x9fa, 0xef6, 0xfff, 0xcf5, 0xdfc,
    0x2fc, 0x3f5, 0x0ff, 0x1f6, 0x6fa, 0x7f3, 0x4f9, 0x5f0, 0xb60, 0xa69, 0x963, 0x86a,
    0xf66, 0xe6f, 0xd65, 0xc6c, 0x36c, 0x265, 0x16f, 0x066, 0x76a, 0x663, 0x569, 0x460,
    0xca0, 0xda9, 0xea3, 0xfaa, 0x8a6, 0x9af, 0xaa5, 0xbac, 0x4ac, 0x5a5, 0x6af, 0x7a6,
    0x0aa, 0x1a3, 0x2a9, 0x3a0, 0xd30, 0xc39, 0xf33, 0xe3a, 0x936, 0x83f, 0xb35, 0xa3c,
    0x53c, 0x435, 0x73f, 0x636, 0x13a, 0x033, 0x339, 0x230, 0xe90, 0xf99, 0xc93, 0xd9a,
    0xa96, 0xb9f, 0x895, 0x99c, 0x69c, 0x795, 0x49f, 0x596, 0x29a, 0x393, 0x099, 0x190,
    0xf00, 0xe09, 0xd03, 0xc0a, 0xb06, 0xa0f, 0x905, 0x80c, 0x70c, 0x605, 0x50f, 0x406,
    0x30a, 0x203, 0x109, 0x000,
];

#[allow(clippy::all)]
const TRI_TABLE: [[i8; 16]; 256] = include!("terrain_tri_table.in");

#[cfg(test)]
mod tests {
    use super::*;

    fn base_settings() -> TerrainGenSettings {
        TerrainGenSettings {
            world_scale: 1.0,
            cache_root: PathBuf::from("target/terrain_cache"),
            height: HeightSettings {
                chunk_samples: 16,
                border_samples: 1,
                sample_spacing: 1.0,
                height_scale: 20.0,
                noise: NoiseSettings {
                    frequency: 0.05,
                    octaves: 4,
                    lacunarity: 2.0,
                    gain: 0.5,
                    ridged: false,
                },
                warp: None,
            },
            density: DensitySettings {
                chunk_dims: [16, 16, 16],
                border_samples: 1,
                voxel_size: 1.0,
                iso_level: 0.0,
                cave: CaveParams {
                    frequency: 0.08,
                    threshold: 0.3,
                    strength: 8.0,
                },
                overhang: OverhangParams {
                    frequency: 0.12,
                    strength: 2.5,
                    max_height: 12.0,
                },
                cliff: CliffParams {
                    frequency: 0.1,
                    strength: 3.0,
                },
            },
            materials: MaterialSettings {
                layers: [
                    MaterialLayer {
                        min_height: -100.0,
                        max_height: 5.0,
                        weight: 1.0,
                    },
                    MaterialLayer {
                        min_height: 5.0,
                        max_height: 12.0,
                        weight: 1.0,
                    },
                    MaterialLayer {
                        min_height: 12.0,
                        max_height: 18.0,
                        weight: 1.0,
                    },
                    MaterialLayer {
                        min_height: 18.0,
                        max_height: 100.0,
                        weight: 1.0,
                    },
                ],
                slope_rock_threshold: 0.8,
            },
            determinism_epsilon: 1e-6,
        }
    }

    #[test]
    fn determinism_height_chunk() {
        let settings = base_settings();
        let seed = WorldSeed(1234);
        let coord = ChunkCoord2 { cx: 0, cz: 0, lod: 0 };
        let chunk_a = generate_height_chunk(seed, coord, &settings);
        let chunk_b = generate_height_chunk(seed, coord, &settings);
        assert_eq!(chunk_a.size, chunk_b.size);
        for (a, b) in chunk_a.heights.iter().zip(chunk_b.heights.iter()) {
            assert!((a - b).abs() <= settings.determinism_epsilon);
        }
        assert_eq!(chunk_a.material_weights, chunk_b.material_weights);
    }

    #[test]
    fn seam_height_chunk_border_matches() {
        let settings = base_settings();
        let seed = WorldSeed(99);
        let coord_a = ChunkCoord2 { cx: 0, cz: 0, lod: 0 };
        let coord_b = ChunkCoord2 { cx: 1, cz: 0, lod: 0 };
        let chunk_a = generate_height_chunk(seed, coord_a, &settings);
        let chunk_b = generate_height_chunk(seed, coord_b, &settings);
        let size = chunk_a.size as usize;
        let border = settings.height.border_samples as usize;
        for z in border..(size - border) {
            let a_index = z * size + (size - border - 1);
            let b_index = z * size + border;
            assert!(
                (chunk_a.heights[a_index] - chunk_b.heights[b_index]).abs()
                    <= settings.determinism_epsilon
            );
        }
    }

    #[test]
    fn seam_density_chunk_border_matches() {
        let settings = base_settings();
        let seed = WorldSeed(777);
        let sampler = HeightSampler::new(seed, settings.height.clone());
        let volume = FeatureVolume {
            id: 1,
            shape: Aabb {
                min: [0.0, -10.0, -10.0],
                max: [40.0, 20.0, 10.0],
            },
            kind: FeatureKind::Cave,
            params: FeatureParams::Cave(settings.density.cave.clone()),
        };
        let coord_a = ChunkCoord3 { cx: 0, cy: 0, cz: 0, lod: 0 };
        let coord_b = ChunkCoord3 { cx: 1, cy: 0, cz: 0, lod: 0 };
        let chunk_a = generate_density_chunk(seed, coord_a, &[volume.clone()], &settings, &sampler);
        let chunk_b = generate_density_chunk(seed, coord_b, &[volume], &settings, &sampler);
        let dims = chunk_a.dims;
        let border = settings.density.border_samples as usize;
        let x_a = dims[0] as usize - border - 1;
        let x_b = border;
        for z in border..(dims[2] as usize - border) {
            for y in border..(dims[1] as usize - border) {
                let a_index = index_3d(x_a as u32, y as u32, z as u32, dims);
                let b_index = index_3d(x_b as u32, y as u32, z as u32, dims);
                assert!(
                    (chunk_a.density[a_index] - chunk_b.density[b_index]).abs()
                        <= settings.determinism_epsilon
                );
            }
        }
    }

    #[test]
    fn mesh_sanity() {
        let mut settings = base_settings();
        settings.density.chunk_dims = [10, 10, 10];
        let dims = [
            settings.density.chunk_dims[0] + settings.density.border_samples * 2,
            settings.density.chunk_dims[1] + settings.density.border_samples * 2,
            settings.density.chunk_dims[2] + settings.density.border_samples * 2,
        ];
        let mut density = vec![0.0; (dims[0] * dims[1] * dims[2]) as usize];
        let center = [
            dims[0] as f32 / 2.0,
            dims[1] as f32 / 2.0,
            dims[2] as f32 / 2.0,
        ];
        let radius = dims[0] as f32 / 3.0;
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let dx = x as f32 - center[0];
                    let dy = y as f32 - center[1];
                    let dz = z as f32 - center[2];
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    density[index_3d(x, y, z, dims)] = radius - dist;
                }
            }
        }
        let chunk = DensityChunk {
            dims,
            density,
            material_id: None,
        };
        let mesh = mesh_density_chunk(&chunk, &settings);
        assert!(!mesh.indices.is_empty());
        for normal in mesh.normals {
            let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                .sqrt();
            assert!((len - 1.0).abs() < 0.05);
        }
    }
}
