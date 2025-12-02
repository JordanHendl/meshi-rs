use std::ptr::NonNull;

use arrayvec::ArrayVec;
use dashi::{Buffer, CommandStream, Context, Handle, cmd::Recording};
use furikake::{types::{Camera, Material}, GPUState};
use glam::Mat4;
use noren::meta::DeviceModel;
use resource_pool::resource_list::ResourceList;

use crate::{RenderObject, RenderObjectInfo};

const MAX_CHILDREN: usize = 18;
pub struct SceneObject {
    model: DeviceModel,
    parent: Option<Handle<SceneObject>>,
    children: ArrayVec<Handle<SceneObject>, MAX_CHILDREN>,
    local_transform: Mat4,
    world_transform: Mat4,
}

const MAX_NUM_MESHES: usize = 32;
#[repr(C)]
pub struct CulledMesh {
    vertices: Handle<Buffer>,
    indices: Handle<Buffer>,
    material: Handle<Material>,
}

#[repr(C)]
pub struct CulledObject {
    transform: Mat4,
    meshes: ArrayVec<CulledMesh, MAX_NUM_MESHES>,
}
pub struct GPUScene<State: GPUState> {
    state: NonNull<State>,
    ctx: NonNull<Context>,
    resources: ResourceList<RenderObject>,
}

impl<State: GPUState> GPUScene<State> {
    pub fn new(ctx: &mut Context, state: &mut State) -> Self {
        if State::reserved_names().iter().find(|name| **name == "meshi_bindless_materials") == None {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        if State::reserved_names().iter().find(|name| **name == "meshi_bindless_camera") == None {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        todo!("Initialize compute pipelines for GPU based culling. allocate data for outputs/intermediary steps")
    }
    
    pub fn set_active_camera(&mut self, camera: Handle<Camera>) {
        todo!("Set active camera in furikake state's active camera resources to use.")
    }

    pub fn register_object(&mut self, info: &RenderObjectInfo) -> Handle<RenderObject> {
        todo!("Add object to scene objects (to be used for frustum/LOD culling)")
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!("Release object from scene objects")
    }

    pub fn transform_object(&mut self, handle: Handle<RenderObject>, transform: &Mat4) {
        todo!("Modify scene object transform")
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &Mat4) {
        todo!("Set scene object transform")
    }

    pub fn add_child(&mut self, parent: Handle<RenderObject>, child: Handle<RenderObject>) {
        todo!("Add child/parent relationship")
    }

    pub fn draw_list(&mut self) -> (CommandStream<Recording>, Handle<Buffer>) {
        todo!(
            "Solves parent/children via compute shader, record operations. Return a buffer containing the list of meshes to draw (with data)"
        )

            // Idea is: Either user can pull this in a full GPU driven renderer with inderect
            // drawing.... or they can just pull to CPU and then iterate through draw list.
    }
}
