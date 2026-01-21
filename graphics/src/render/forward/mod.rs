use super::environment::{EnvironmentFrameSettings, EnvironmentRenderer, EnvironmentRendererInfo};
use super::gui::GuiRenderer;
use super::scene::GPUScene;
use super::skinning::{SkinningDispatcher, SkinningHandle, SkinningInfo};
use super::text::TextRenderer;
use super::{Renderer, RendererInfo, ViewOutput};
use crate::{
    AnimationState, BillboardInfo, CloudSettings, GuiInfo, GuiObject, RenderObject,
    RenderObjectInfo, TextInfo, TextObject, render::scene::*,
};
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use dashi::structs::{IndexedIndirectCommand, IndirectCommand};
use dashi::*;
use driver::command::{DrawIndexedIndirect, DrawIndirect};
use execution::{CommandDispatch, CommandRing};
use furikake::PSOBuilderFurikakeExt;
use furikake::{
    BindlessState, reservations::bindless_materials::ReservedBindlessMaterials, types::Material,
    types::*,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use meshi_utils::MeshiError;
use noren::{
    DB,
    meta::{DeviceModel, HostMaterial},
};
use resource_pool::resource_list::ResourceList;
use std::{collections::HashMap, ptr::NonNull};
use tare::graph::*;
use tare::transient::TransientAllocator;
use tracing::{info, warn};

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

#[repr(u32)]
pub enum PassMask {
    MAIN_COLOR = 0x00000001,
}

pub struct ForwardRenderer {
    ctx: Box<Context>,
    viewport: Viewport,
    sample_count: SampleCount,
    state: Box<BindlessState>,
    db: Option<NonNull<DB>>,
    scene: GPUScene,
    environment: EnvironmentRenderer,
    pipelines: HashMap<Handle<Material>, PSO>,
    billboard_pso: PSO,
    objects: ResourceList<RenderObjectData>,
    scene_lookup: HashMap<u16, Handle<RenderObjectData>>,
    dynamic: DynamicAllocator,
    cull_queue: CommandRing,
    skinning: SkinningDispatcher,
    skinning_complete: Option<Handle<Semaphore>>,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
    text: TextRenderer,
    gui: GuiRenderer,
    cloud_settings: CloudSettings,
}

struct RenderObjectData {
    kind: RenderObjectKind,
    scene_handle: Handle<SceneObject>,
    draw_range: RenderObjectKind,
}

enum RenderObjectKind {
    Model(DeviceModel),
    SkinnedModel(SkinnedRenderData),
    Billboard(BillboardData),
}

#[derive(Clone)]
struct SkinnedRenderData {
    model: DeviceModel,
    skinning: SkinningInfo,
    skinning_handle: SkinningHandle,
}

#[derive(Clone)]
struct BillboardData {
    info: BillboardInfo,
    vertex_buffer: Handle<Buffer>,
    owns_material: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BillboardVertex {
    center: [f32; 3],
    offset: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    tex_coords: [f32; 2],
}

fn to_handle(h: Handle<RenderObjectData>) -> Handle<RenderObject> {
    return Handle::new(h.slot, h.generation);
}

fn from_handle(h: Handle<RenderObject>) -> Handle<RenderObjectData> {
    return Handle::new(h.slot, h.generation);
}

impl ForwardRenderer {
    fn build_billboard_pipeline(
        ctx: &mut Context,
        state: &mut BindlessState,
        per_instance_buffer: Handle<Buffer>,
        sample_count: SampleCount,
    ) -> PSO {
        let shaders = miso::stdbillboard(&[]);
        let per_obj_resource = ShaderResource::StorageBuffer(per_instance_buffer.into());

        let mut pso_builder = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, Format::BGRA8)
            .add_table_variable_with_resources(
                "per_obj_ssbo",
                vec![IndexedResource {
                    resource: per_obj_resource,
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variables(state)
            .expect("Failed to add reserved tables for billboard pipeline");

        pso_builder = pso_builder.add_depth_target(AttachmentDesc {
            format: Format::D24S8,
            samples: sample_count,
        });

        let pso = pso_builder
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count,
                depth_test: Some(DepthInfo {
                    should_test: true,
                    should_write: false,
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build billboard pipeline!");

        state.register_pso_tables(&pso);

        pso
    }

    pub fn new(info: &RendererInfo) -> Self {
        todo!()
    }

    pub fn alloc(&mut self) -> &mut TransientAllocator {
        &mut self.alloc
    }

    fn build_pipeline(&mut self, mat: &HostMaterial) -> PSO {
        todo!()
        //        let ctx: *mut Context = self.ctx.as_mut();
        //
        //        let mut defines = Vec::new();
        //
        //        if mat.material.render_mask & PassMask::MAIN_COLOR as u32 > 0 {
        //            defines.push("-DLMAO".to_string());
        //        }
        //
        //        let shaders = miso::stdforward(&defines);
        //        let per_obj_resource =
        //            ShaderResource::StorageBuffer(self.scene.per_instance_buffer().into());
        //
        //        let mut state = PSOBuilder::new()
        //            .vertex_compiled(Some(shaders[0].clone()))
        //            .fragment_compiled(Some(shaders[1].clone()))
        //            .set_attachment_format(0, Format::BGRA8)
        //            .add_table_variable_with_resources(
        //                "per_obj_ssbo",
        //                vec![IndexedResource {
        //                    resource: per_obj_resource,
        //                    slot: 0,
        //                }],
        //            );
        //
        //        state = state
        //            .add_reserved_table_variables(self.state.as_mut())
        //            .unwrap();
        //
        //        state = state.add_depth_target(AttachmentDesc {
        //            format: Format::D24S8,
        //            samples: self.sample_count,
        //        });
        //
        //        let s = state
        //            .set_details(GraphicsPipelineDetails {
        //                color_blend_states: vec![Default::default(); 1],
        //                sample_count: self.sample_count,
        //                depth_test: Some(DepthInfo {
        //                    should_test: true,
        //                    should_write: true,
        //                }),
        //                ..Default::default()
        //            })
        //            .build(unsafe { &mut (*ctx) })
        //            .expect("Failed to build material!");
        //
        //        assert!(s.bind_table[0].is_some());
        //        assert!(s.bind_table[1].is_some());
        //
        //        self.state.register_pso_tables(&s);
        //        s
    }

    fn allocate_billboard_material(&mut self, texture_id: u32) -> Handle<Material> {
        todo!()
    }

    fn update_billboard_material_texture(&mut self, material: Handle<Material>, texture_id: u32) {
        todo!()
    }

    fn create_billboard_data(&mut self, mut info: BillboardInfo) -> BillboardData {
        todo!()
    }

    fn billboard_vertices(center: Vec3, size: Vec2, color: Vec4) -> [BillboardVertex; 6] {
        todo!()
    }

    fn update_billboard_vertices(
        &mut self,
        billboard: &BillboardData,
        transform: Mat4,
        camera: Handle<Camera>,
    ) {
        todo!()
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);
        self.alloc.set_bindless_registry(self.state.as_mut());

        let materials = db.enumerate_materials();

        for name in materials {
            let (mat, handle) = db.fetch_host_material(&name).unwrap();
            let p = self.build_pipeline(&mat);
            info!(
                "[MESHI/GFX] Creating pipelines for material {} (Handle => {}).",
                name,
                handle.as_ref().unwrap().slot
            );
            self.pipelines.insert(handle.unwrap(), p);
        }

        self.db = Some(NonNull::new(db).expect("lmao"));
        self.text.initialize_database(db);
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        todo!()
    }

    pub fn set_skinned_animation_state(
        &mut self,
        handle: Handle<RenderObject>,
        state: AnimationState,
    ) {
        if !handle.valid() {
            warn!("Attempted to update animation on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update animation for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));
        match &mut obj.kind {
            RenderObjectKind::SkinnedModel(skinned) => {
                self.skinning
                    .set_animation_state(skinned.skinning_handle, state);
            }
            _ => {
                warn!("Attempted to update animation on non-skinned object.");
            }
        }
    }

    pub fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32) {
        if !handle.valid() {
            warn!("Attempted to update billboard texture on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!(
                "Failed to update billboard texture for object {}",
                handle.slot
            );
            return;
        }

        let (owns_material, material_handle) = {
            let obj = self.objects.get_ref_mut(from_handle(handle));
            match &mut obj.kind {
                RenderObjectKind::Billboard(billboard) => {
                    billboard.info.texture_id = texture_id;
                    (billboard.owns_material, billboard.info.material)
                }
                _ => {
                    warn!("Attempted to update billboard texture on non-billboard object.");
                    return;
                }
            }
        };

        if owns_material {
            if let Some(material) = material_handle {
                self.update_billboard_material_texture(material, texture_id);
            }
        }
    }

    pub fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        todo!()
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        if !handle.valid() {
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let (skinning_handle, billboard_material) = {
            let obj = self.objects.get_ref(from_handle(handle));
            self.scene.release_object(obj.scene_handle);
            self.scene_lookup.remove(&obj.scene_handle.slot);

            match &obj.kind {
                RenderObjectKind::SkinnedModel(skinned) => (Some(skinned.skinning_handle), None),
                RenderObjectKind::Billboard(billboard) => {
                    if billboard.owns_material {
                        (None, billboard.info.material)
                    } else {
                        (None, None)
                    }
                }
                _ => (None, None),
            }
        };

        if let Some(skinned_handle) = skinning_handle {
            self.skinning
                .unregister(skinned_handle, self.state.as_mut());
        }

        if let Some(material) = billboard_material {
            self.state
                .reserved_mut::<ReservedBindlessMaterials, _>(
                    "meshi_bindless_materials",
                    |materials| {
                        materials.remove_material(material);
                    },
                )
                .expect("Failed to remove billboard material");
        }

        self.objects.release(from_handle(handle));
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        if !handle.valid() {
            return Default::default();
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return Default::default();
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.scene.get_object_transform(obj.scene_handle)
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        if !handle.valid() {
            warn!("Attempted to update transformation of invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update transform for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.scene.set_object_transform(obj.scene_handle, transform);
    }

    pub fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        self.text.register_text(info)
    }

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        self.gui.register_gui(info)
    }

    pub fn release_text(&mut self, handle: Handle<TextObject>) {
        self.text.release_text(handle);
    }

    pub fn release_gui(&mut self, handle: Handle<GuiObject>) {
        self.gui.release_gui(handle);
    }

    pub fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        self.text.set_text(handle, text);
    }

    pub fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        self.text.set_text_info(handle, info);
    }

    pub fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        self.gui.set_gui_info(handle, info);
    }

    pub fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        self.gui.set_gui_visibility(handle, visible);
    }

    fn pull_scene(&mut self) {
        todo!()
    }

    pub fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        if views.is_empty() {
            return Vec::new();
        }
        self.gui
            .initialize_renderer(self.ctx.as_mut(), self.state.as_mut(), self.sample_count);
        self.text
            .initialize_renderer(self.ctx.as_mut(), self.state.as_mut(), self.sample_count);
        if self.cull_queue.current_index() == 0 {
            self.dynamic.reset();
            self.environment.reset();
        }

        let _ = self.text.emit_draws();

        self.skinning_complete = self.skinning.update(delta_time);

        // Set active scene cameras..
        self.scene.set_active_cameras(views);
        // Pull scene GPU --> CPU to read.
        self.pull_scene();

        // Default framebuffer info.
        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.w as u32, self.viewport.area.h as u32, 1],
            layers: 1,
            format: Format::BGRA8,
            mip_levels: 1,
            samples: self.sample_count,
            initial_data: None,
            ..Default::default()
        };

        let semaphores = self.graph.make_semaphores(1);
        let mut outputs = Vec::with_capacity(views.len());
        let mut depth = self.graph.make_image(&ImageInfo {
            debug_name: &format!("[MESHI FORWARD] Depth buffer"),
            format: Format::D24S8,
            ..default_framebuffer_info
        });

        depth.view.aspect = AspectMask::Depth;

        self.environment.update(EnvironmentFrameSettings {
            delta_time,
            ..Default::default()
        });

        let sky_cubemap_pass = views.first().and_then(|camera| {
            self.environment.prepare_sky_cubemap(
                self.ctx.as_mut(),
                self.state.as_mut(),
                &self.viewport,
                *camera,
            )
        });

        if let Some(pass) = sky_cubemap_pass {
            for (face_index, face_view) in pass.face_views.iter().enumerate() {
                let mut attachments: [Option<ImageView>; 8] = [None; 8];
                attachments[0] = Some(*face_view);

                let mut clear_values: [Option<ClearValue>; 8] = [None; 8];
                clear_values[0] = Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0]));

                let cubemap_viewport = pass.viewport;
                self.graph.add_subpass(
                    &SubpassInfo {
                        viewport: cubemap_viewport,
                        color_attachments: attachments,
                        depth_attachment: None,
                        clear_values,
                        depth_clear: None,
                    },
                    |mut cmd| {
                        cmd.combine(
                            self.environment
                                .render_sky_cubemap_face(&cubemap_viewport, face_index),
                        )
                    },
                );
            }
        }

        for (view_idx, camera) in views.iter().enumerate() {
            let color = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI FORWARD] Position Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let mut forward_pass_attachments: [Option<ImageView>; 8] = [None; 8];
            forward_pass_attachments[0] = Some(color.view);

            let mut forward_pass_clear: [Option<ClearValue>; 8] = [None; 8];
            forward_pass_clear[0] = Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0]));

            let camera_handle = *camera;
            struct BillboardDraw {
                vertex_buffer: Handle<Buffer>,
                material: Handle<Material>,
                transformation: Handle<Transformation>,
                total_transform: Mat4,
                draw_index: u32,
            }
            let mut billboard_draws = Vec::new();
            {
                let handles = self.objects.entries.clone();
                for handle in handles {
                    let (scene_handle, draw_range, billboard) = {
                        let obj = self.objects.get_ref(handle);
                        let RenderObjectKind::Billboard(billboard) = &obj.kind else {
                            continue;
                        };
                        (obj.scene_handle, todo!(), billboard.clone())
                    };
                    if let Some(material) = billboard.info.material {
                        let transform = self.scene.get_object_transform(scene_handle);
                        self.update_billboard_vertices(&billboard, transform, camera_handle);
                        billboard_draws.push(BillboardDraw {
                            vertex_buffer: billboard.vertex_buffer,
                            material,
                            transformation: todo!(),
                            total_transform: transform,
                            draw_index: todo!(),
                        });
                    }
                }
            }

            // Forward SPLIT pass. Renders the following framebuffers:
            // 1) Color
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: forward_pass_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: forward_pass_clear,
                    depth_clear: Some(ClearValue::DepthStencil {
                        depth: 1.0,
                        stencil: 0,
                    }),
                },
                |mut cmd| cmd.combine(self.environment.render(&self.viewport, camera_handle)),
            );

            let mut gui_attachments: [Option<ImageView>; 8] = [None; 8];
            gui_attachments[0] = Some(color.view);
            let gui_clear: [Option<ClearValue>; 8] = [None; 8];

            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: gui_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: gui_clear,
                    depth_clear: None,
                },
                |mut cmd| {
                    cmd = cmd.combine(
                        self.text
                            .render_transparent(self.ctx.as_mut(), &self.viewport),
                    );
                    cmd.combine(self.gui.render_gui(&self.viewport))
                },
            );

            outputs.push(ViewOutput {
                camera: *camera,
                image: color.view,
                semaphore: semaphores[0],
            });
        }

        let mut wait_sems = Vec::with_capacity(sems.len() + 1);
        wait_sems.extend_from_slice(sems);
        if let Some(semaphore) = self.skinning_complete {
            wait_sems.push(semaphore);
        }

        self.graph.execute_with(&SubmitInfo {
            wait_sems: &wait_sems,
            signal_sems: &[semaphores[0]],
        });

        outputs
    }

    pub fn shut_down(self) {
        self.ctx.destroy();
    }
}

impl Renderer for ForwardRenderer {
    fn context(&mut self) -> &'static mut Context {
        unsafe { &mut (*(self.ctx.as_mut() as *mut Context)) }
    }

    fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn initialize_database(&mut self, db: &mut DB) {
        ForwardRenderer::initialize_database(self, db);
    }

    fn set_skybox_cubemap(&mut self, cubemap: noren::rdb::imagery::DeviceCubemap) {
        self.environment
            .update_skybox(super::environment::sky::SkyboxFrameSettings {
                cubemap: Some(cubemap),
                use_procedural_cubemap: false,
                ..Default::default()
            });
    }

    fn set_skybox_settings(&mut self, settings: super::environment::sky::SkyboxFrameSettings) {
        self.environment.update_skybox(settings);
    }

    fn set_sky_settings(&mut self, settings: super::environment::sky::SkyFrameSettings) {
        self.environment.update_sky(settings);
    }

    fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        ForwardRenderer::register_object(self, info)
    }

    fn set_skinned_animation_state(&mut self, handle: Handle<RenderObject>, state: AnimationState) {
        ForwardRenderer::set_skinned_animation_state(self, handle, state);
    }

    fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32) {
        ForwardRenderer::set_billboard_texture(self, handle, texture_id);
    }

    fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        ForwardRenderer::set_billboard_material(self, handle, material);
    }

    fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        ForwardRenderer::set_object_transform(self, handle, transform);
    }

    fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        ForwardRenderer::object_transform(self, handle)
    }

    fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        ForwardRenderer::register_text(self, info)
    }

    fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        ForwardRenderer::register_gui(self, info)
    }

    fn release_text(&mut self, handle: Handle<TextObject>) {
        ForwardRenderer::release_text(self, handle);
    }

    fn release_gui(&mut self, handle: Handle<GuiObject>) {
        ForwardRenderer::release_gui(self, handle);
    }

    fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        ForwardRenderer::set_text(self, handle, text);
    }

    fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        ForwardRenderer::set_text_info(self, handle, info);
    }

    fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        ForwardRenderer::set_gui_info(self, handle, info);
    }

    fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        ForwardRenderer::set_gui_visibility(self, handle, visible);
    }

    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        ForwardRenderer::update(self, sems, views, delta_time)
    }

    fn cloud_settings(&self) -> CloudSettings {
        self.cloud_settings
    }

    fn set_cloud_settings(&mut self, settings: CloudSettings) {
        self.cloud_settings = settings;
    }

    fn set_cloud_weather_map(&mut self, _view: Option<ImageView>) {}

    fn shut_down(self: Box<Self>) {
        self.ctx.destroy();
    }
}
