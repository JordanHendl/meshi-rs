use std::{collections::HashMap, ptr::NonNull};

use crate::{RenderObject, RenderObjectInfo, render::scene::*};
use bento::builder::{GraphicsPipelineBuilder, PSO};
use dashi::*;
use furikake::{
    BindlessState, reservations::ReservedBinding, reservations::bindless_transformations::*,
    types::Material, types::*,
};
use glam::{Mat4, Vec3};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
use noren::{
    DB,
    meta::{DeviceModel, HostMaterial},
};
use resource_pool::resource_list::ResourceList;
use tare::transient::TransientAllocator;
use tracing::info;
use utils::gpupool::GPUPool;

use super::scene::GPUScene;
use tare::graph::*;

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

#[repr(u32)]
pub enum PassMask {
    MAIN_COLOR = 0x00000001,
}

pub struct DeferredRendererInfo {
    pub headless: bool,
    pub initial_viewport: Viewport,
}

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

pub struct DeferredRenderer {
    ctx: Box<Context>,
    viewport: Viewport,
    state: Box<BindlessState>,
    db: Option<NonNull<DB>>,
    scene: GPUScene<furikake::BindlessState>,
    pipelines: HashMap<Handle<Material>, PSO>,
    objects: ResourceList<RenderObjectData>,
    dynamic: DynamicAllocator,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
}

struct RenderObjectData {
    model: DeviceModel,
    scene_handle: Handle<SceneObject>,
    transformation: Handle<Transformation>,
}

fn to_handle(h: Handle<RenderObjectData>) -> Handle<RenderObject> {
    return Handle::new(h.slot, h.generation);
}

fn from_handle(h: Handle<RenderObject>) -> Handle<RenderObjectData> {
    return Handle::new(h.slot, h.generation);
}


impl DeferredRenderer {
    pub fn new(info: &DeferredRendererInfo) -> Self {
        let mut ctx = if info.headless {
            Box::new(Context::headless(&Default::default()).expect(""))
        } else {
            Box::new(Context::new(&Default::default()).expect(""))
        };

        let mut state = Box::new(BindlessState::new(&mut ctx));

        let scene = GPUScene::new(
            &GPUSceneInfo {
                name: "[MESHI] Deferred Renderer Scene",
                ctx: ctx.as_mut(),
                draw_bins: &[SceneBin {
                    id: 0,
                    mask: PassMask::MAIN_COLOR as u32,
                }],
                ..Default::default()
            },
            state.as_mut(),
        );

        let mut alloc = Box::new(TransientAllocator::new(ctx.as_mut()));

        let dynamic = ctx
            .make_dynamic_allocator(&DynamicAllocatorInfo {
                ..Default::default()
            })
            .expect("Unable to create dynamic allocator!");

        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        Self {
            ctx,
            state,
            scene,
            alloc,
            graph,
            db: None,
            dynamic,
            pipelines: Default::default(),
            objects: Default::default(),
            viewport: info.initial_viewport,
        }
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn build_pipeline(&mut self, mat: &HostMaterial) -> PSO {
        let ctx: *mut Context = self.ctx.as_mut();

        let mut defines = Vec::new();

        if mat.material.render_mask & PassMask::MAIN_COLOR as u16 > 0 {
            defines.push("-DLMAO".to_string());
        }

        let shaders = miso::stddeferred(&defines);

        let mut state = GraphicsPipelineBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .add_variable(
                "per_object_ssbo",
                ShaderResource::DynamicStorage(self.dynamic.state()),
            );

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_cameras")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_cameras", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_textures")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_textures", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_materials")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_materials", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_transformations")
                .unwrap()
                .binding();
            state = state
                .add_table_variable_with_resources("meshi_bindless_transformations", resources);
        }

        state
            .build(unsafe { &mut (*ctx) })
            .expect("Failed to build material!")
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);

        let materials = db.enumerate_materials();

        for name in materials {
            let (mat, handle) = db.fetch_host_material(&name).unwrap();
            info!("[MESHI/GFX] Creating pipelines for material {}.", name);
            let p = self.build_pipeline(&mat);
            self.pipelines.insert(handle.unwrap(), p);
        }

        self.db = Some(NonNull::new(db).expect("lmao"));
    }

    fn alloc_transform(&mut self) -> Handle<Transformation> {
        let mut handle = Handle::default();
        self.state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    handle = transforms.add_transform();
                },
            )
            .expect("allocate bindless transform");

        handle
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        let transformation = self.alloc_transform();

        self.state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    transforms.transform_mut(transformation).transform = Mat4::IDENTITY;
                },
            )
            .expect("transformations available");

        let scene_handle = self.scene.register_object(&SceneObjectInfo {
            local: Default::default(),
            global: Default::default(),
            transformation,
            scene_mask: PassMask::MAIN_COLOR as u32,
        });

        match info {
            RenderObjectInfo::Model(m) => {
                let h = self.objects.push(RenderObjectData {
                    model: m.clone(),
                    scene_handle,
                    transformation,
                });

                Ok(to_handle(h))
            }
            RenderObjectInfo::Empty => todo!(),//Err(MeshiError::ResourceUnavailable),
        }
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        if !handle.valid() {
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.scene.release_object(obj.scene_handle);

        let transformation = obj.transformation;
        self.state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| transforms.remove_transform(transformation),
            )
            .expect("transformations available");

        self.objects.release(from_handle(handle));
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        if !handle.valid() {
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| transforms.transform_mut(obj.transformation).transform = *transform,
            )
            .expect("transformations available");

        self.scene.set_object_transform(obj.scene_handle, transform);
    }

    pub fn update(&mut self, _delta_time: f32) {
        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.w as u32, self.viewport.area.h as u32, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            samples: SampleCount::S1,
            initial_data: None,
        };

        let position = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Position Framebuffer",
            ..default_framebuffer_info
        });

        let normal = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Normal Framebuffer",
            ..default_framebuffer_info
        });

        let diffuse = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Diffuse Framebuffer",
            ..default_framebuffer_info
        });

        let mut deferred_pass_attachments: [Option<ImageView>; 8] = [None; 8];
        deferred_pass_attachments[0] = Some(position);
        deferred_pass_attachments[1] = Some(normal);
        deferred_pass_attachments[2] = Some(diffuse);

        let mut deferred_pass_clear: [Option<ClearValue>; 8] = [None; 8];
        deferred_pass_clear[..3].fill(Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0])));

        let mut culled_cmds = self.scene.cull();
        let object_draws: Vec<_> = self
            .objects
            .entries
            .iter()
            .map(|handle| self.objects.get_ref(*handle).model.clone())
            .collect();

        self.graph.add_subpass(
            &SubpassInfo {
                viewport: self.viewport,
                color_attachments: deferred_pass_attachments,
                depth_attachment: None,
                clear_values: deferred_pass_clear,
                depth_clear: None,
            },
            |cmd| {
                //                let alloc = self.dynamic.bump();
                //                let mut cmd = cmd.combine(&mut culled_cmds);
                //
                //                for model in &object_draws {
                //                    for mesh in &model.meshes {
//                        if let Some(material) = &mesh.material {
//                            // TODO: retrieve the material's bindless handle once it is exposed
//                            // on DeviceMaterial so we can select the correct PSO here.
//                            if let Some((_, pso)) = self.pipelines.iter().next() {
//                                cmd = cmd
//                                    .bind_pso(pso.clone())
//                                    .bind_vertex_buffer(mesh.geometry.base.vertices)
//                                    .bind_index_buffer(mesh.geometry.base.indices)
//                                    .draw_indexed(mesh.geometry.base.indices, 0, 1);
//                            }
//                        }
                //                    }
                //                }
                //
                //                cmd
                return cmd;
            },
        );

        self.graph.execute();
    }
}
