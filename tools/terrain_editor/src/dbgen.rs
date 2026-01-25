use meshi_graphics::rdb::terrain::TerrainChunkArtifact;
use noren::{
    RDBFile,
    rdb::terrain::{
        TerrainChunkState, TerrainDirtyReason, TerrainGeneratorDefinition,
        TerrainMaterialBlendMode, TerrainMutationLayer, TerrainMutationOp, TerrainMutationOpKind,
        TerrainMutationParams, TerrainProjectSettings, chunk_coord_key, chunk_state_entry,
        generator_entry, mutation_layer_entry, mutation_op_entry, project_settings_entry,
    },
    terrain::{
        TerrainChunkBuildRequest, build_terrain_chunk_with_context, prepare_terrain_build_context,
    },
};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct TerrainGenerationRequest {
    pub chunk_key: String,
    pub generator_graph_id: String,
    pub lod: u8,
}

#[derive(Clone, Debug)]
pub struct TerrainBrushRequest {
    pub chunk_key: String,
    pub generator_graph_id: String,
    pub lod: u8,
    pub world_pos: [f32; 3],
    pub radius: f32,
    pub strength: f32,
    pub tool: TerrainMutationOpKind,
}

pub struct TerrainDbgen {
    seed: u64,
}

impl TerrainDbgen {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn status(&self) -> &'static str {
        "stub"
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
    }

    pub fn generate_chunk(
        &mut self,
        request: &TerrainGenerationRequest,
    ) -> Option<TerrainChunkArtifact> {
        let chunk_hash = hash_chunk_key(self.seed, &request.chunk_key);
        let chunk_coords = self.chunk_coords_for_key(&request.chunk_key);
        let project_key = sanitize_project_key(&request.chunk_key);

        let mut settings = TerrainProjectSettings::default();
        settings.name = format!("Terrain Preview {}", request.chunk_key);
        settings.seed = self.seed ^ chunk_hash;
        if !request.generator_graph_id.is_empty() {
            settings.generator_graph_id = request.generator_graph_id.clone();
        }

        let mut generator = TerrainGeneratorDefinition::default();
        if !request.generator_graph_id.is_empty() {
            generator.graph_id = request.generator_graph_id.clone();
        }

        let mutation_layer = TerrainMutationLayer::default();
        let mut rdb = RDBFile::new();
        rdb.add(&project_settings_entry(&project_key), &settings)
            .ok()?;
        rdb.add(
            &generator_entry(&project_key, settings.active_generator_version),
            &generator,
        )
        .ok()?;
        rdb.add(
            &mutation_layer_entry(
                &project_key,
                &mutation_layer.layer_id,
                settings.active_mutation_version,
            ),
            &mutation_layer,
        )
        .ok()?;

        let context = prepare_terrain_build_context(&mut rdb, &project_key).ok()?;
        let request = TerrainChunkBuildRequest {
            chunk_coords,
            lod: request.lod,
        };
        let outcome = build_terrain_chunk_with_context(
            &mut rdb,
            &project_key,
            &context,
            request,
            |_| {},
            || false,
        )
        .ok()?;

        outcome.artifact
    }

    pub fn apply_brush(
        &mut self,
        request: &TerrainBrushRequest,
        rdb_path: &Path,
    ) -> Result<TerrainChunkArtifact, String> {
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
    ) -> Result<TerrainChunkArtifact, String> {
        self.apply_brush_internal(request, rdb)
    }

    pub fn chunk_coords_for_key(&self, chunk_key: &str) -> [i32; 2] {
        let chunk_hash = hash_chunk_key(self.seed, chunk_key);
        [chunk_hash as i32, (chunk_hash >> 32) as i32]
    }

    fn apply_brush_internal(
        &mut self,
        request: &TerrainBrushRequest,
        rdb: &mut RDBFile,
    ) -> Result<TerrainChunkArtifact, String> {
        let chunk_coords = self.chunk_coords_for_key(&request.chunk_key);
        let project_key = sanitize_project_key(&request.chunk_key);
        let chunk_hash = hash_chunk_key(self.seed, &request.chunk_key);

        let mut settings = rdb
            .fetch::<TerrainProjectSettings>(&project_settings_entry(&project_key))
            .unwrap_or_else(|_| {
                let mut settings = TerrainProjectSettings::default();
                settings.name = format!("Terrain Preview {}", request.chunk_key);
                settings.seed = self.seed ^ chunk_hash;
                settings
            });
        if !request.generator_graph_id.is_empty() {
            settings.generator_graph_id = request.generator_graph_id.clone();
        }

        let generator_entry_key = generator_entry(&project_key, settings.active_generator_version);
        let mut generator = rdb
            .fetch::<TerrainGeneratorDefinition>(&generator_entry_key)
            .unwrap_or_default();
        if !request.generator_graph_id.is_empty() {
            generator.graph_id = request.generator_graph_id.clone();
        }

        let layer_id = "layer-1";
        let layer_entry_key =
            mutation_layer_entry(&project_key, layer_id, settings.active_mutation_version);
        let layer = rdb
            .fetch::<TerrainMutationLayer>(&layer_entry_key)
            .unwrap_or_else(|_| TerrainMutationLayer::new(layer_id, "Layer 1", 0));

        rdb.upsert(&project_settings_entry(&project_key), &settings)
            .map_err(|err| format!("Project settings upsert failed: {err}"))?;
        rdb.upsert(&generator_entry_key, &generator)
            .map_err(|err| format!("Generator upsert failed: {err}"))?;
        rdb.upsert(&layer_entry_key, &layer)
            .map_err(|err| format!("Mutation layer upsert failed: {err}"))?;

        let (order, event_id) = next_op_order_and_event(
            rdb,
            &project_key,
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
                &project_key,
                layer_id,
                settings.active_mutation_version,
                order,
                event_id,
            ),
            &op,
        )
        .map_err(|err| format!("Mutation op add failed: {err}"))?;

        mark_chunk_dirty(rdb, &project_key, &settings, chunk_coords)
            .map_err(|err| format!("Chunk dirty update failed: {err}"))?;

        let context =
            prepare_terrain_build_context(rdb, &project_key).map_err(|err| err.to_string())?;
        let build_request = TerrainChunkBuildRequest {
            chunk_coords,
            lod: request.lod,
        };
        let outcome = build_terrain_chunk_with_context(
            rdb,
            &project_key,
            &context,
            build_request,
            |_| {},
            || false,
        )
        .map_err(|err| err.to_string())?;
        let artifact = outcome
            .artifact
            .ok_or_else(|| "Brush build returned no artifact".to_string())?;

        rdb.upsert(&request.chunk_key, &artifact)
            .map_err(|err| format!("Brush artifact upsert failed: {err}"))?;

        Ok(artifact)
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

fn sanitize_project_key(chunk_key: &str) -> String {
    let mut sanitized = String::with_capacity(chunk_key.len());
    for ch in chunk_key.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        "terrain-preview".to_string()
    } else {
        sanitized
    }
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
