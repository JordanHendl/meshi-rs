use meshi_graphics::rdb::terrain::TerrainChunkArtifact;
use noren::{
    RDBFile,
    rdb::terrain::{
        TerrainGeneratorDefinition, TerrainMutationLayer, TerrainProjectSettings, generator_entry,
        mutation_layer_entry, project_settings_entry,
    },
    terrain::{
        TerrainChunkBuildRequest, build_terrain_chunk_with_context, prepare_terrain_build_context,
    },
};

#[derive(Clone, Debug)]
pub struct TerrainGenerationRequest {
    pub chunk_key: String,
    pub mode: String,
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

    pub fn generate_chunk(
        &mut self,
        request: &TerrainGenerationRequest,
    ) -> Option<TerrainChunkArtifact> {
        let chunk_hash = hash_chunk_key(self.seed, &request.chunk_key);
        let chunk_coords = [chunk_hash as i32, (chunk_hash >> 32) as i32];
        let project_key = sanitize_project_key(&request.chunk_key);

        let mut settings = TerrainProjectSettings::default();
        settings.name = format!("Terrain Preview {}", request.chunk_key);
        settings.seed = self.seed ^ chunk_hash;
        if !request.mode.is_empty() {
            settings.generator_graph_id = request.mode.clone();
        }

        let mut generator = TerrainGeneratorDefinition::default();
        if !request.mode.is_empty() {
            generator.graph_id = request.mode.clone();
        }

        let mutation_layer = TerrainMutationLayer::default();
        let mut rdb = RDBFile::new();
        rdb.add(&project_settings_entry(&project_key), &settings).ok()?;
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
            lod: lod_from_mode(&request.mode),
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

    pub fn apply_brush(&mut self, _request: &TerrainGenerationRequest) -> bool {
        // TODO: Hook this to noren's dbgen manual editing operations.
        false
    }
}

fn lod_from_mode(mode: &str) -> u8 {
    mode.strip_prefix("lod")
        .and_then(|lod| lod.parse::<u8>().ok())
        .unwrap_or(0)
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
