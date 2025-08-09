use std::collections::HashSet;
use std::ffi::CString;

use dashi::utils::Handle;
use glam::Vec3;
use tracing::warn;

use crate::object::{FFIMeshObjectInfo, MeshObject, MeshObjectInfo};
use crate::render::database::Database;
use crate::render::RenderEngine;

#[derive(Clone, Copy)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    pub fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

pub struct Region {
    pub bounds: AABB,
    pub objects: Vec<MeshObjectInfo>,
    mesh_cstrings: Vec<CString>,
    material_cstrings: Vec<CString>,
    handles: Vec<Handle<MeshObject>>,
}

impl Region {
    pub fn new(bounds: AABB, objects: Vec<MeshObjectInfo>) -> Self {
        let mut mesh_cstrings = Vec::with_capacity(objects.len());
        let mut material_cstrings = Vec::with_capacity(objects.len());
        for obj in &objects {
            mesh_cstrings.push(CString::new(obj.mesh).unwrap_or_else(|e| {
                warn!("invalid mesh name '{}': {}", obj.mesh, e);
                CString::default()
            }));
            material_cstrings.push(CString::new(obj.material).unwrap_or_else(|e| {
                warn!("invalid material name '{}': {}", obj.material, e);
                CString::default()
            }));
        }
        Self {
            bounds,
            objects,
            mesh_cstrings,
            material_cstrings,
            handles: Vec::new(),
        }
    }
}

pub struct StreamingManager {
    pub regions: Vec<Region>,
    pub active_regions: HashSet<usize>,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
            active_regions: HashSet::new(),
        }
    }

    pub fn register_region(&mut self, region: Region) -> usize {
        let idx = self.regions.len();
        self.regions.push(region);
        idx
    }

    pub fn unregister_region(&mut self, index: usize, renderer: &mut RenderEngine) {
        if index >= self.regions.len() {
            return;
        }

        if self.active_regions.remove(&index) {
            let region = &mut self.regions[index];
            for handle in region.handles.drain(..) {
                renderer.release_mesh_object(handle);
            }
        }

        let last = self.regions.len() - 1;
        self.regions.swap_remove(index);
        if index != last {
            if self.active_regions.remove(&last) {
                self.active_regions.insert(index);
            }
        }
    }

    pub fn reset(&mut self, renderer: &mut RenderEngine) {
        for idx in self.active_regions.drain() {
            if let Some(region) = self.regions.get_mut(idx) {
                for handle in region.handles.drain(..) {
                    renderer.release_mesh_object(handle);
                }
            }
        }
    }

    pub fn update(&mut self, player_pos: Vec3, db: &mut Database, renderer: &mut RenderEngine) {
        let _ = db; // currently unused
        let mut new_active = HashSet::new();
        for (i, region) in self.regions.iter().enumerate() {
            if region.bounds.contains(player_pos) {
                new_active.insert(i);
            }
        }

        // handle newly entered regions
        for idx in new_active
            .difference(&self.active_regions)
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(region) = self.regions.get_mut(idx) {
                region.handles.clear();
                for (i, obj) in region.objects.iter().enumerate() {
                    let ffi_info = FFIMeshObjectInfo {
                        mesh: region.mesh_cstrings[i].as_ptr(),
                        material: region.material_cstrings[i].as_ptr(),
                        transform: obj.transform,
                    };
                    match renderer.register_mesh_object(&ffi_info) {
                        Ok(handle) => region.handles.push(handle),
                        Err(e) => warn!("failed to register mesh object '{}': {}", obj.mesh, e),
                    }
                }
            }
        }

        // handle exited regions
        for idx in self
            .active_regions
            .difference(&new_active)
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(region) = self.regions.get_mut(idx) {
                for handle in region.handles.drain(..) {
                    renderer.release_mesh_object(handle);
                }
            }
        }

        self.active_regions = new_active;
    }
}
