use super::json;
use std::collections::HashMap;

/// A material entry from the database and its load state.
#[derive(Clone, Default)]
pub struct MaterialResource {
    pub cfg: json::MaterialEntry,
    /// Whether the material's GPU resources have been registered.
    pub loaded: bool,
}

impl From<json::Materials> for HashMap<String, MaterialResource> {
    fn from(value: json::Materials) -> Self {
        let mut v = HashMap::new();
        for m in value.materials {
            v.insert(
                m.name.clone(),
                MaterialResource {
                    cfg: m,
                    loaded: false,
                },
            );
        }
        v
    }
}

