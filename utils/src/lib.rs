pub mod timer;

use bento::BentoError;
use furikake::error::FurikakeError;
use noren::NorenError;

#[derive(Debug)]
pub struct MeshiError {

}


impl From<dashi::GPUError> for MeshiError {
    fn from(_value: dashi::GPUError) -> Self {
        todo!()
    }
}

impl From<NorenError> for MeshiError {
    fn from(_value: NorenError) -> Self {
        todo!()
    }
}

impl From<BentoError> for MeshiError {
    fn from(_value: BentoError) -> Self {
        todo!()
    }
}

impl From<FurikakeError> for MeshiError {
    fn from(_value: FurikakeError) -> Self {
        todo!()
    }
}


//impl From<dashi::GPUError> for MeshiError {
//    fn from(value: dashi::GPUError) -> Self {
//        todo!()
//    }
//}
