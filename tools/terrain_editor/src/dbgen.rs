use meshi_graphics::rdb::terrain::TerrainChunkArtifact;

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
        _request: &TerrainGenerationRequest,
    ) -> Option<TerrainChunkArtifact> {
        // TODO: Wire this up to noren's dbgen to generate procedural terrain artifacts.
        // When dbgen is online, return the generated TerrainChunkArtifact for preview.
        let _ = self.seed;
        None
    }

    pub fn apply_brush(&mut self, _request: &TerrainGenerationRequest) -> bool {
        // TODO: Hook this to noren's dbgen manual editing operations.
        false
    }
}
