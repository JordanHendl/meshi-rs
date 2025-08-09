use std::collections::HashMap;
use std::path::Path;

use gltf::Gltf;

use super::{geometry_primitives, MeshResource};

/// Load the default set of mesh primitives into a map keyed by their names.
///
/// The database uses this during initialization so tests can rely on a small
/// library of meshes (triangle, cube and sphere) without hitting the file
/// system.
pub fn load_primitives() -> HashMap<String, MeshResource> {
    let mut geometry = HashMap::new();
    let tri = geometry_primitives::make_triangle(&Default::default());
    geometry.insert(
        "MESHI_TRIANGLE".to_string(),
        MeshResource::from_primitive("TRIANGLE", tri),
    );
    let cube = geometry_primitives::make_cube(&Default::default());
    geometry.insert(
        "MESHI_CUBE".to_string(),
        MeshResource::from_primitive("CUBE", cube),
    );
    let sphere = geometry_primitives::make_sphere(&Default::default());
    geometry.insert(
        "MESHI_SPHERE".to_string(),
        MeshResource::from_primitive("SPHERE", sphere),
    );
    let cyl = geometry_primitives::make_cylinder(&Default::default());
    geometry.insert(
        "MESHI_CYLINDER".to_string(),
        MeshResource::from_primitive("CYLINDER", cyl),
    );
    let plane = geometry_primitives::make_plane(&Default::default());
    geometry.insert(
        "MESHI_PLANE".to_string(),
        MeshResource::from_primitive("PLANE", plane),
    );
    let cone = geometry_primitives::make_cone(&Default::default());
    geometry.insert(
        "MESHI_CONE".to_string(),
        MeshResource::from_primitive("CONE", cone),
    );
    geometry
}

/// Parse a glTF file from disk and return its document.
///
/// The helper consolidates error handling and provides a single place for
/// future glTF related extensions.
pub fn parse_gltf<P: AsRef<Path>>(path: P) -> gltf::Result<Gltf> {
    Gltf::open(path)
}

#[cfg(test)]
mod tests {
    use super::parse_gltf;
    use std::fs;
    use tempfile::tempdir;

    // Build a minimal glTF asset containing a single triangle primitive and
    // ensure the parser succeeds.
    #[test]
    fn parse_triangle_gltf() {
        let dir = tempdir().unwrap();
        let bin_path = dir.path().join("data.bin");
        // Positions for three vertices followed by indices 0,1,2
        let mut bin = Vec::new();
        for f in [
            0.0f32, 0.0, 0.0, // v0
            1.0, 0.0, 0.0, // v1
            0.0, 1.0, 0.0, // v2
        ] {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        fs::write(&bin_path, &bin).unwrap();

        let gltf = format!(
            "{{\n  \"asset\": {{ \"version\": \"2.0\" }},\n  \"scenes\": [{{ \"nodes\": [0] }}],\n  \"scene\": 0,\n  \"nodes\": [{{ \"mesh\": 0 }}],\n  \"meshes\": [{{ \"primitives\": [{{ \"attributes\": {{ \"POSITION\": 0 }}, \"indices\": 1 }}] }}],\n  \"buffers\": [{{ \"uri\": \"data.bin\", \"byteLength\": {} }}],\n  \"bufferViews\": [{{ \"buffer\": 0, \"byteOffset\": 0, \"byteLength\": 36 }}, {{ \"buffer\": 0, \"byteOffset\": 36, \"byteLength\": 6 }}],\n  \"accessors\": [{{ \"bufferView\": 0, \"componentType\": 5126, \"count\": 3, \"type\": \"VEC3\", \"min\": [0.0,0.0,0.0], \"max\": [1.0,1.0,0.0] }}, {{ \"bufferView\": 1, \"componentType\": 5123, \"count\": 3, \"type\": \"SCALAR\" }}]\n}}",
            bin.len()
        );
        let gltf_path = dir.path().join("scene.gltf");
        fs::write(&gltf_path, gltf).unwrap();

        let doc = parse_gltf(&gltf_path).unwrap();
        assert_eq!(doc.meshes().len(), 1);
    }
}
