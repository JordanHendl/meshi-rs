use super::RenderError;
use crate::object::MeshObject;
use crate::render::database::Vertex as MeshVertex;
use bytemuck::cast_slice;
use dashi::{BufferInfo, BufferUsage, Format, MemoryVisibility};
use glam::Mat4;
use image::{imageops::FilterType, RgbaImage};
use inline_spirv::inline_spirv;
use koji::canvas::Canvas;
use koji::renderer::{Renderer, StaticMesh, Vertex as KojiVertex};
use koji::{CanvasBuilder, PipelineBuilder};

pub struct CanvasRenderer {
    extent: Option<[u32; 2]>,
    renderer: Option<Renderer>,
    canvas: Option<Box<Canvas>>,
    headless: bool,
    next_mesh: usize,
}

impl CanvasRenderer {
    pub fn new(extent: Option<[u32; 2]>, headless: bool) -> Self {
        Self {
            extent,
            renderer: None,
            canvas: None,
            next_mesh: 0,
            headless,
        }
    }

    fn init(&mut self, ctx: &mut dashi::Context) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            let [width, height] = self.extent.unwrap_or([1024, 1024]);

            if self.canvas.is_none() {
                let canvas = CanvasBuilder::new()
                    .extent([width, height])
                    .color_attachment("color", Format::RGBA8)
                    .depth_attachment("depth", Format::D24S8)
                    .build(ctx)?;
                self.canvas = Some(Box::new(canvas));
            }

            let canvas = self.canvas.as_ref().expect("canvas should be initialized");

            // Build pipeline before moving the canvas into the renderer so we don't
            // duplicate GPU resources. Pipeline creation only needs a reference to the
            // canvas's render pass at this stage.
            let vert = inline_spirv!(
                r#"#version 450
                #define KOJI_MAX_CAMERAS 4
                struct Camera {
                    mat4 view_proj;
                    vec4 cam_pos;
                };
                layout(set = 0, binding = 4) uniform CameraBuffer { Camera cameras[KOJI_MAX_CAMERAS]; } KOJI_cameras;
                layout(set = 1, binding = 0) uniform ModelBuffer { mat4 model; } KOJI_model;
                layout(location=0) in vec3 position;
                layout(location=1) in vec3 normal;
                layout(location=2) in vec4 tangent;
                layout(location=3) in vec2 uv;
                layout(location=4) in vec4 color;
                void main() {
                    vec4 world = KOJI_model.model * vec4(position, 1.0);
                    gl_Position = KOJI_cameras.cameras[0].view_proj * world;
                }
                "#,
                vert
            );
            let frag = inline_spirv!(
                r#"#version 450
                layout(location=0) out vec4 color;
                void main() { color = vec4(1.0,0.0,0.0,1.0); }
                "#,
                frag
            );

            let mut pso = PipelineBuilder::new(ctx, "canvas_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((canvas.render_pass(), 0))
                .build();

            // Now create the renderer consuming the canvas.
            let mut renderer = if self.headless {
                Renderer::with_canvas_headless(width, height, ctx, (**canvas).clone())?
            } else {
                Renderer::with_canvas(width, height, ctx, (**canvas).clone())?
            };

            // Register default model matrix for objects.
            renderer
                .resources()
                .register_variable("KOJI_model", ctx, Mat4::IDENTITY);

            // Create bind groups for the pipeline now that the renderer exists and
            // has a resource manager.
            let bind_groups = pso
                .create_bind_groups(renderer.resources())
                .map_err(|_| RenderError::Gpu(dashi::GPUError::LibraryError()))?;
            renderer.register_pipeline_for_pass("color", pso, bind_groups);

            self.renderer = Some(renderer);
        }
        Ok(())
    }

    pub fn register_mesh(
        &mut self,
        ctx: &mut dashi::Context,
        obj: &MeshObject,
    ) -> Result<usize, RenderError> {
        self.init(ctx)?;

        let vertices: Vec<KojiVertex> = obj.mesh.vertices[..obj.mesh.num_vertices]
            .iter()
            .map(|v: &MeshVertex| KojiVertex {
                position: [v.position.x, v.position.y, v.position.z],
                normal: [v.normal.x, v.normal.y, v.normal.z],
                tangent: [0.0, 0.0, 0.0, 0.0],
                uv: [v.tex_coords.x, v.tex_coords.y],
                color: [v.color.x, v.color.y, v.color.z, v.color.w],
            })
            .collect();

        // Register mesh with Koji renderer using CPU data; Koji will upload GPU
        // buffers internally, so no need to allocate them here.
        let indices = obj.mesh.indices[..obj.mesh.num_indices].to_vec();
        let mesh = StaticMesh {
            material_id: String::new(),
            vertices,
            indices: if indices.is_empty() {
                None
            } else {
                Some(indices)
            },
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
        };

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.register_static_mesh(mesh, None, "color".into());
        }

        let idx = self.next_mesh;
        self.next_mesh += 1;
        Ok(idx)
    }

    pub fn update_mesh(&mut self, ctx: &mut dashi::Context, idx: usize, obj: &MeshObject) {
        if self.init(ctx).is_err() {
            return;
        }

        let vertices: Vec<KojiVertex> = obj.mesh.vertices[..obj.mesh.num_vertices]
            .iter()
            .map(|v: &MeshVertex| KojiVertex {
                position: [v.position.x, v.position.y, v.position.z],
                normal: [v.normal.x, v.normal.y, v.normal.z],
                tangent: [0.0, 0.0, 0.0, 0.0],
                uv: [v.tex_coords.x, v.tex_coords.y],
                color: [v.color.x, v.color.y, v.color.z, v.color.w],
            })
            .collect();

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_static_mesh(idx, &vertices);
        }
    }

    pub fn render(&mut self, ctx: &mut dashi::Context) -> Result<(), RenderError> {
        self.init(ctx)?;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.present_frame()?;
        }
        Ok(())
    }

    pub fn render_to_image(
        &mut self,
        ctx: &mut dashi::Context,
        extent: [u32; 2],
    ) -> Result<RgbaImage, RenderError> {
        self.init(ctx)?;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.present_frame()?;
            let data = renderer.read_color_target("color");
            let [rw, rh] = self.extent.unwrap_or(extent);
            let mut img =
                RgbaImage::from_raw(rw, rh, data).expect("failed to create image from bytes");
            if [rw, rh] != extent {
                img = image::imageops::resize(&img, extent[0], extent[1], FilterType::Nearest);
            }
            Ok(img)
        } else {
            Err(RenderError::Gpu(dashi::GPUError::LibraryError()))
        }
    }
}
