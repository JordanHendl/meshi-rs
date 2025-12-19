pub mod gpuoption;
use bento::builder::PSO;
use furikake::types::Material;
pub use gpuoption::*;

#[repr(C)]
pub struct HPSO {
    pub pso: PSO,
    pub hash: u64,
}

pub fn hash_material(mat: &Material) {
    todo!()
}
