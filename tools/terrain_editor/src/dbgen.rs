use meshi_graphics::rdb::terrain::TerrainChunkArtifact;
use noren::{
    rdb::terrain::{
        chunk_artifact_entry, chunk_coord_key, chunk_state_entry, generator_entry, lod_key,
        mutation_layer_entry, mutation_op_entry, parse_chunk_artifact_entry,
        project_settings_entry, TerrainChunk, TerrainChunkState, TerrainDirtyReason,
        TerrainGeneratorDefinition, TerrainMaterialBlendMode, TerrainMutationLayer,
        TerrainMutationOp, TerrainMutationOpKind, TerrainMutationParams, TerrainProjectSettings,
        TerrainTile,
    },
    terrain::{
        build_terrain_chunk_with_context, prepare_terrain_build_context,
        sample_height_with_mutations, TerrainChunkBuildRequest,
    },
    RDBFile,
};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct TerrainGenerationRequest {
    pub chunk_key: String,
    pub generator_graph_id: String,
    pub lod: u8,
    pub generator_frequency: f32,
    pub generator_amplitude: f32,
    pub generator_biome_frequency: f32,
    pub generator_algorithm: String,
    pub world_chunks: [u32; 2],
    pub vertex_resolution: u32,
}

#[derive(Clone, Debug)]
pub struct TerrainBrushRequest {
    pub chunk_key: String,
    pub generator_graph_id: String,
    pub lod: u8,
    pub generator_frequency: f32,
    pub generator_amplitude: f32,
    pub generator_biome_frequency: f32,
    pub generator_algorithm: String,
    pub world_chunks: [u32; 2],
    pub vertex_resolution: u32,
    pub world_pos: [f32; 3],
    pub radius: f32,
    pub strength: f32,
    pub tool: TerrainMutationOpKind,
}

pub struct TerrainDbgen {
    seed: u64,
}

const DEFAULT_LAYER_ID: &str = "layer-1";

pub struct TerrainChunkResult {
    pub entry_key: String,
    pub artifact: TerrainChunkArtifact,
    pub chunk_entry: String,
    pub chunk: TerrainChunk,
}

impl TerrainDbgen {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn status(&self) -> &'static str {
        "noren dbgen"
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
    }

    pub fn generate_chunk(
        &mut self,
        request: &TerrainGenerationRequest,
        rdb: &mut RDBFile,
    ) -> Result<TerrainChunkResult, String> {
        let target = self.resolve_chunk_target(&request.chunk_key, request.lod);
        let chunk_hash = hash_chunk_key(self.seed, &request.chunk_key);

        self.upsert_project_state(
            rdb,
            &target.project_key,
            &request.chunk_key,
            chunk_hash,
            &request.generator_graph_id,
            &request.generator_algorithm,
            request.generator_frequency,
            request.generator_amplitude,
            request.generator_biome_frequency,
            request.world_chunks,
            request.vertex_resolution,
        )?;

        let context = prepare_terrain_build_context(rdb, &target.project_key).map_err(|err| {
            format!(
                "Terrain build context failed for project '{}': {err}",
                target.project_key
            )
        })?;
        let build_request = TerrainChunkBuildRequest {
            chunk_coords: target.chunk_coords,
            lod: request.lod,
        };
        let outcome = build_terrain_chunk_with_context(
            rdb,
            &target.project_key,
            &context,
            build_request,
            |_| {},
            || false,
        )
        .map_err(|err| format!("Terrain build failed: {err}"))?;

        if let Some(state) = outcome.state {
            rdb.upsert(&target.state_entry, &state).map_err(|err| {
                format!(
                    "Chunk state upsert failed for entry '{}': {err}",
                    target.state_entry
                )
            })?;
        }

        let artifact = if let Some(artifact) = outcome.artifact {
            rdb.upsert(&target.artifact_entry, &artifact)
                .map_err(|err| {
                    format!(
                        "Chunk artifact upsert failed for entry '{}': {err}",
                        target.artifact_entry
                    )
                })?;
            artifact
        } else {
            rdb.fetch::<TerrainChunkArtifact>(&target.artifact_entry)
                .map_err(|err| {
                    format!(
                        "Chunk artifact fetch failed for entry '{}': {err}",
                        target.artifact_entry
                    )
                })?
        };

        let chunk = build_chunk_data(&context, target.chunk_coords);
        let chunk_entry = chunk_entry_for_coords(target.chunk_coords);
        rdb.upsert(&chunk_entry, &chunk)
            .map_err(|err| format!("Chunk data upsert failed for entry '{chunk_entry}': {err}"))?;

        Ok(TerrainChunkResult {
            entry_key: target.artifact_entry,
            artifact,
            chunk_entry,
            chunk,
        })
    }

    pub fn apply_brush(
        &mut self,
        request: &TerrainBrushRequest,
        rdb_path: &Path,
    ) -> Result<TerrainChunkResult, String> {
        let mut rdb = RDBFile::load(rdb_path).unwrap_or_else(|_| RDBFile::new());
        let artifact = self.apply_brush_internal(request, &mut rdb)?;
        rdb.save(rdb_path)
            .map_err(|err| format!("Brush RDB save failed: {err}"))?;
        Ok(artifact)
    }

    pub fn apply_brush_in_memory(
        &mut self,
        request: &TerrainBrushRequest,
        rdb: &mut RDBFile,
    ) -> Result<TerrainChunkResult, String> {
        self.apply_brush_internal(request, rdb)
    }

    pub fn chunk_coords_for_key(&self, chunk_key: &str) -> [i32; 2] {
        if let Some(parsed) = parse_chunk_artifact_entry(chunk_key) {
            return parsed.chunk_coords;
        }

        if let Some(coords) = parse_chunk_entry(chunk_key) {
            return coords;
        }

        let chunk_hash = hash_chunk_key(self.seed, chunk_key);
        let x = (chunk_hash & 0xffff) as i16;
        let y = ((chunk_hash >> 16) & 0xffff) as i16;
        [x as i32, y as i32]
    }

    pub fn project_key_for_chunk(&self, chunk_key: &str) -> String {
        if let Some(parsed) = parse_chunk_artifact_entry(chunk_key) {
            return parsed.project_key;
        }

        short_project_key(chunk_key)
    }

    pub fn chunk_entry_for_key(&self, chunk_key: &str, lod: u8) -> String {
        self.resolve_chunk_target(chunk_key, lod).artifact_entry
    }

    fn apply_brush_internal(
        &mut self,
        request: &TerrainBrushRequest,
        rdb: &mut RDBFile,
    ) -> Result<TerrainChunkResult, String> {
        let target = self.resolve_chunk_target(&request.chunk_key, request.lod);
        let chunk_hash = hash_chunk_key(self.seed, &request.chunk_key);

        let settings = self.upsert_project_state(
            rdb,
            &target.project_key,
            &request.chunk_key,
            chunk_hash,
            &request.generator_graph_id,
            &request.generator_algorithm,
            request.generator_frequency,
            request.generator_amplitude,
            request.generator_biome_frequency,
            request.world_chunks,
            request.vertex_resolution,
        )?;

        let layer_id = DEFAULT_LAYER_ID;
        let (order, event_id) = next_op_order_and_event(
            rdb,
            &target.project_key,
            layer_id,
            settings.active_mutation_version,
        );
        let op_id = format!("op-{}", current_timestamp());
        let params = build_brush_params(request.tool, request.world_pos);
        let op = TerrainMutationOp {
            op_id,
            layer_id: layer_id.to_string(),
            enabled: true,
            order,
            kind: request.tool,
            params,
            radius: request.radius.max(0.0),
            strength: request.strength,
            falloff: 0.5,
            event_id,
            timestamp: current_timestamp(),
            author: None,
        };
        rdb.add(
            &mutation_op_entry(
                &target.project_key,
                layer_id,
                settings.active_mutation_version,
                order,
                event_id,
            ),
            &op,
        )
        .map_err(|err| format!("Mutation op add failed: {err}"))?;

        mark_chunk_dirty(rdb, &target.project_key, &settings, target.chunk_coords)
            .map_err(|err| format!("Chunk dirty update failed: {err}"))?;

        let context = prepare_terrain_build_context(rdb, &target.project_key)
            .map_err(|err| err.to_string())?;
        let build_request = TerrainChunkBuildRequest {
            chunk_coords: target.chunk_coords,
            lod: request.lod,
        };
        let outcome = build_terrain_chunk_with_context(
            rdb,
            &target.project_key,
            &context,
            build_request,
            |_| {},
            || false,
        )
        .map_err(|err| err.to_string())?;
        let artifact = outcome
            .artifact
            .ok_or_else(|| "Brush build returned no artifact".to_string())?;

        if let Some(state) = outcome.state {
            rdb.upsert(&target.state_entry, &state)
                .map_err(|err| format!("Chunk state upsert failed: {err}"))?;
        }

        rdb.upsert(&target.artifact_entry, &artifact)
            .map_err(|err| format!("Brush artifact upsert failed: {err}"))?;

        let chunk = build_chunk_data(&context, target.chunk_coords);
        let chunk_entry = chunk_entry_for_coords(target.chunk_coords);
        rdb.upsert(&chunk_entry, &chunk)
            .map_err(|err| format!("Chunk data upsert failed for entry '{chunk_entry}': {err}"))?;

        Ok(TerrainChunkResult {
            entry_key: target.artifact_entry,
            artifact,
            chunk_entry,
            chunk,
        })
    }

    fn upsert_project_state(
        &self,
        rdb: &mut RDBFile,
        project_key: &str,
        chunk_key: &str,
        chunk_hash: u64,
        generator_graph_id: &str,
        generator_algorithm: &str,
        generator_frequency: f32,
        generator_amplitude: f32,
        generator_biome_frequency: f32,
        world_chunks: [u32; 2],
        vertex_resolution: u32,
    ) -> Result<TerrainProjectSettings, String> {
        let mut settings = rdb
            .fetch::<TerrainProjectSettings>(&project_settings_entry(project_key))
            .unwrap_or_else(|_| {
                let mut settings = TerrainProjectSettings::default();
                settings.name = format!("Terrain Preview {chunk_key}");
                settings.seed = self.seed ^ chunk_hash;
                settings
            });
        if !generator_graph_id.is_empty() {
            settings.generator_graph_id = generator_graph_id.to_string();
        }
        let vertex_resolution = vertex_resolution.max(1);
        settings.tiles_per_chunk = [vertex_resolution, vertex_resolution];
        let world_chunks = [world_chunks[0].max(1), world_chunks[1].max(1)];
        let chunk_size_x = settings.tile_size * settings.tiles_per_chunk[0] as f32;
        let chunk_size_y = settings.tile_size * settings.tiles_per_chunk[1] as f32;
        settings.world_bounds_max[0] =
            settings.world_bounds_min[0] + chunk_size_x * world_chunks[0] as f32;
        settings.world_bounds_max[1] =
            settings.world_bounds_min[1] + chunk_size_y * world_chunks[1] as f32;

        let generator_entry_key = generator_entry(project_key, settings.active_generator_version);
        let mut generator = rdb
            .fetch::<TerrainGeneratorDefinition>(&generator_entry_key)
            .unwrap_or_else(|_| {
                let mut generator = TerrainGeneratorDefinition::default();
                generator.version = settings.active_generator_version;
                generator
            });
        if !generator_graph_id.is_empty() {
            generator.graph_id = generator_graph_id.to_string();
        }
        if !generator_algorithm.is_empty() {
            generator.algorithm = generator_algorithm.to_string();
        }
        generator.frequency = generator_frequency;
        generator.amplitude = generator_amplitude;
        generator.biome_frequency = generator_biome_frequency;

        let layer_id = DEFAULT_LAYER_ID;
        let layer_entry_key =
            mutation_layer_entry(project_key, layer_id, settings.active_mutation_version);
        let mut layer = rdb
            .fetch::<TerrainMutationLayer>(&layer_entry_key)
            .unwrap_or_else(|_| TerrainMutationLayer::new(layer_id, "Layer 1", 0));
        layer.version = settings.active_mutation_version;

        rdb.upsert(&project_settings_entry(project_key), &settings)
            .map_err(|err| format!("Project settings upsert failed: {err}"))?;
        rdb.upsert(&generator_entry_key, &generator)
            .map_err(|err| format!("Generator upsert failed: {err}"))?;
        rdb.upsert(&layer_entry_key, &layer)
            .map_err(|err| format!("Mutation layer upsert failed: {err}"))?;

        Ok(settings)
    }

    fn resolve_chunk_target(&self, chunk_key: &str, lod: u8) -> TerrainChunkTarget {
        if let Some(parsed) = parse_chunk_artifact_entry(chunk_key) {
            let coord_key = chunk_coord_key(parsed.chunk_coords[0], parsed.chunk_coords[1]);
            let project_key = parsed.project_key.clone();
            return TerrainChunkTarget {
                project_key: project_key.clone(),
                chunk_coords: parsed.chunk_coords,
                artifact_entry: chunk_artifact_entry(&project_key, &coord_key, &lod_key(lod)),
                state_entry: chunk_state_entry(&project_key, &coord_key),
            };
        }

        let project_key = short_project_key(chunk_key);
        let chunk_coords = self.chunk_coords_for_key(chunk_key);
        let coord_key = chunk_coord_key(chunk_coords[0], chunk_coords[1]);
        TerrainChunkTarget {
            project_key: project_key.clone(),
            chunk_coords,
            artifact_entry: chunk_artifact_entry(&project_key, &coord_key, &lod_key(lod)),
            state_entry: chunk_state_entry(&project_key, &coord_key),
        }
    }
}

struct TerrainChunkTarget {
    project_key: String,
    chunk_coords: [i32; 2],
    artifact_entry: String,
    state_entry: String,
}

fn chunk_entry_for_coords(chunk_coords: [i32; 2]) -> String {
    format!("terrain/chunk_{}_{}", chunk_coords[0], chunk_coords[1])
}

fn parse_chunk_entry(entry: &str) -> Option<[i32; 2]> {
    let suffix = entry.strip_prefix("terrain/chunk_")?;
    let mut parts = suffix.split('_');
    let x = parts.next()?.parse::<i32>().ok()?;
    let y = parts.next()?.parse::<i32>().ok()?;
    Some([x, y])
}

fn build_chunk_data(
    context: &noren::terrain::TerrainBuildContext,
    chunk_coords: [i32; 2],
) -> TerrainChunk {
    let settings = &context.settings;
    let tile_size = settings.tile_size;
    let tiles_per_chunk = settings.tiles_per_chunk;
    let origin_x = chunk_coords[0] as f32 * tiles_per_chunk[0] as f32 * tile_size;
    let origin_y = chunk_coords[1] as f32 * tiles_per_chunk[1] as f32 * tile_size;
    let grid_x = tiles_per_chunk[0].saturating_add(1).max(1);
    let grid_y = tiles_per_chunk[1].saturating_add(1).max(1);
    let mut heights = vec![0.0; (grid_x * grid_y) as usize];
    for y in 0..grid_y {
        for x in 0..grid_x {
            let world_x = origin_x + x as f32 * tile_size;
            let world_y = origin_y + y as f32 * tile_size;
            let idx = (y * grid_x + x) as usize;
            heights[idx] = sample_height_with_mutations(
                settings,
                &context.generator,
                &context.mutation_layers,
                world_x,
                world_y,
            );
        }
    }
    let tiles = vec![
        TerrainTile {
            tile_id: 1,
            flags: 0,
        };
        (tiles_per_chunk[0] * tiles_per_chunk[1]) as usize
    ];
    TerrainChunk {
        chunk_coords,
        origin: [origin_x, origin_y],
        tile_size,
        tiles_per_chunk,
        tiles,
        heights,
        mesh_entry: "geometry/terrain_chunk".to_string(),
    }
}

fn hash_chunk_key(seed: u64, chunk_key: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64 ^ seed;
    for byte in chunk_key.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn short_project_key(chunk_key: &str) -> String {
    let hash = hash_chunk_key(0, chunk_key);
    format!("t{hash:016x}")
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn build_brush_params(tool: TerrainMutationOpKind, center: [f32; 3]) -> TerrainMutationParams {
    match tool {
        TerrainMutationOpKind::SphereAdd | TerrainMutationOpKind::SphereSub => {
            TerrainMutationParams::Sphere { center }
        }
        TerrainMutationOpKind::CapsuleAdd | TerrainMutationOpKind::CapsuleSub => {
            TerrainMutationParams::Capsule {
                start: center,
                end: center,
            }
        }
        TerrainMutationOpKind::Smooth => TerrainMutationParams::Smooth { center },
        TerrainMutationOpKind::MaterialPaint => TerrainMutationParams::MaterialPaint {
            center,
            material_id: 0,
            blend_mode: TerrainMaterialBlendMode::Blend,
        },
    }
}

fn next_op_order_and_event(
    rdb: &mut RDBFile,
    project_key: &str,
    layer_id: &str,
    version: u32,
) -> (u32, u32) {
    let prefix = format!("terrain/mutation_op/{project_key}/{layer_id}/v{version}/");
    let mut max_order = 0;
    let mut max_event = 0;
    for entry in rdb.entries() {
        if let Some(remainder) = entry.name.strip_prefix(&prefix) {
            let mut parts = remainder.split('/');
            if let (Some(order_part), Some(event_part)) = (parts.next(), parts.next()) {
                if let (Some(order), Some(event_id)) = (
                    order_part
                        .strip_prefix('o')
                        .and_then(|v| v.parse::<u32>().ok()),
                    event_part
                        .strip_prefix('e')
                        .and_then(|v| v.parse::<u32>().ok()),
                ) {
                    max_order = max_order.max(order + 1);
                    max_event = max_event.max(event_id);
                }
            }
        }
    }
    (max_order, max_event + 1)
}

fn mark_chunk_dirty(
    rdb: &mut RDBFile,
    project_key: &str,
    settings: &TerrainProjectSettings,
    chunk_coords: [i32; 2],
) -> Result<(), noren::RdbErr> {
    let coord_key = chunk_coord_key(chunk_coords[0], chunk_coords[1]);
    let state_key = chunk_state_entry(project_key, &coord_key);
    let mut state = rdb
        .fetch::<TerrainChunkState>(&state_key)
        .unwrap_or(TerrainChunkState {
            project_key: project_key.to_string(),
            chunk_coords,
            dirty_flags: 0,
            dirty_reasons: Vec::new(),
            generator_version: settings.active_generator_version,
            mutation_version: settings.active_mutation_version,
            last_built_hashes: Vec::new(),
            dependency_hashes: noren::rdb::terrain::TerrainChunkDependencyHashes {
                settings_hash: 0,
                generator_hash: 0,
                mutation_hash: 0,
            },
        });
    state.dirty_flags |= noren::rdb::terrain::TERRAIN_DIRTY_MUTATION;
    if !state
        .dirty_reasons
        .iter()
        .any(|reason| *reason == TerrainDirtyReason::MutationChanged)
    {
        state
            .dirty_reasons
            .push(TerrainDirtyReason::MutationChanged);
    }
    rdb.upsert(&state_key, &state)?;
    Ok(())
}
