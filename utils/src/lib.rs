pub mod timer;

use bento::BentoError;
use furikake::error::FurikakeError;
use noren::NorenError;

#[derive(Debug)]
pub struct MeshiError {

}


impl From<dashi::GPUError> for MeshiError {
    fn from(value: dashi::GPUError) -> Self {
        todo!()
    }
}

impl From<NorenError> for MeshiError {
    fn from(value: NorenError) -> Self {
        todo!()
    }
}

impl From<BentoError> for MeshiError {
    fn from(value: BentoError) -> Self {
        todo!()
    }
}

impl From<FurikakeError> for MeshiError {
    fn from(value: FurikakeError) -> Self {
        todo!()
    }
}


//impl From<dashi::GPUError> for MeshiError {
//    fn from(value: dashi::GPUError) -> Self {
//        todo!()
//    }
//}
