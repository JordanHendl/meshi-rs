use dashi::utils::Handle;
use super::json;
use std::collections::HashMap;

/// A material entry from the database and its lazily loaded handle.
#[derive(Clone, Default)]
pub struct MaterialResource {
    pub cfg: json::MaterialEntry,
    /// GPU handle for this material's base color texture, loaded on demand.
    pub loaded: Option<Handle<koji::Texture>>,
}

impl From<json::Materials> for HashMap<String, MaterialResource> {
    fn from(value: json::Materials) -> Self {
        let mut v = HashMap::new();
        for m in value.materials {
            v.insert(
                m.name.clone(),
                MaterialResource {
                    cfg: m,
                    loaded: None,
                },
            );
        }
        v
    }
}

