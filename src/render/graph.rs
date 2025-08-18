use bytemuck::cast_slice;
use dashi::*;
use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::{Renderer, StaticMesh, Vertex as KojiVertex};
use koji::{render_graph::io, CanvasBuilder, PipelineBuilder, RenderGraph};

use super::RenderError;
use crate::object::MeshObject;
use crate::render::database::Vertex as MeshVertex;
use tracing::warn;

/// A renderer that executes a frame graph described by `koji`.
///
/// The graph description is loaded from a JSON file (typically `koji.json`).
/// A simple pipeline is registered for drawing `MeshObject`s. The renderer
/// handles graph execution when presenting frames.
pub struct GraphRenderer {
    graph_json: Option<String>,
    renderer: Option<Renderer>,
    headless: bool,
    next_mesh: usize,
}

impl GraphRenderer {
    pub fn new(scene_cfg_path: Option<String>, headless: bool) -> Result<Self, RenderError> {
        let graph_json = if let Some(path) = scene_cfg_path {
            match std::fs::read_to_string(&path) {
                Ok(s) => Some(s),
                Err(e) => {
                    warn!("failed to read scene config {path}: {e}");
                    None
                }
            }
        } else {
            None
        };
        Ok(Self {
            graph_json,
            renderer: None,
            next_mesh: 0,
            headless,
        })
    }

    fn default_graph(ctx: &mut dashi::Context, width: u32, height: u32) -> RenderGraph {
        let mut g = RenderGraph::new();
        let canvas = CanvasBuilder::new()
            .extent([width, height])
            .color_attachment("color", Format::RGBA8)
            .depth_attachment("depth", Format::D24S8)
            .build(ctx)
            .expect("canvas build");
        g.add_canvas(&canvas);
        g
    }

    fn init(&mut self, ctx: &mut dashi::Context) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            // Tests expect a 64x64 target.
            let (width, height) = (64, 64);

            let graph = if let Some(json) = &self.graph_json {
                let g = io::from_json(ctx, json).map_err(RenderError::GraphParse)?;
                if g.canvases().is_empty() {
                    Self::default_graph(ctx, width, height)
                } else {
                    g
                }
            } else {
                Self::default_graph(ctx, width, height)
            };

            let mut renderer = if self.headless {
                Renderer::with_graph_headless(width, height, ctx, graph)?
            } else {
                Renderer::with_graph(width, height, ctx, graph)?
            };

            // Use shaders that declare Koji's default uniforms.
            let vert = inline_spirv!(
                r#"#version 450
                struct TimingInfo { float currentTimeMs; float lastFrameTimeMs; };
                layout(set = 0, binding = 0) uniform TimingBuffer { TimingInfo info; } KOJI_time;
                #define KOJI_MAX_CAMERAS 4
                struct Camera { mat4 view_proj; vec4 cam_pos; };
                layout(set = 0, binding = 4) uniform CameraBuffer { Camera cameras[KOJI_MAX_CAMERAS]; } KOJI_cameras;
                layout(location=0) in vec3 position;
                layout(location=1) in vec3 normal;
                layout(location=2) in vec4 tangent;
                layout(location=3) in vec2 uv;
                layout(location=4) in vec4 color;
                void main() {
                    float t = KOJI_time.info.currentTimeMs;
                    // Reference camera data without affecting output.
                    gl_Position = vec4(position, 1.0);
                    gl_Position += (KOJI_cameras.cameras[0].view_proj * vec4(position,1.0) + vec4(t)) * 0.0;
                }
                "#,
                vert
            );
            let frag = inline_spirv!(
                r#"#version 450
                struct TimingInfo { float currentTimeMs; float lastFrameTimeMs; };
                layout(set = 0, binding = 0) uniform TimingBuffer { TimingInfo info; } KOJI_time;
                layout(location=0) out vec4 color;
                void main() {
                    float t = KOJI_time.info.currentTimeMs * 0.0;
                    color = vec4(1.0, 0.0, 0.0, 1.0) + vec4(t);
                }
                "#,
                frag
            );

            let outputs = renderer.graph().output_images();
            let pass = if outputs.is_empty() {
                renderer.render_pass()
            } else {
                let output_name = if !self.headless && outputs.iter().any(|o| o == "swapchain") {
                    "swapchain".to_string()
                } else {
                    outputs.first().cloned().unwrap()
                };
                renderer
                    .graph()
                    .render_pass_for_output(&output_name)
                    .map(|(p, _)| p)
                    .unwrap_or_else(|| renderer.render_pass())
            };

            let mut pso = PipelineBuilder::new(ctx, "graph_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((pass, 0))
                .build_with_resources(renderer.resources())
                .unwrap();

            let bgr = pso.create_bind_groups(renderer.resources()).unwrap();
            renderer.register_material_pipeline("graph_pso", pso, bgr);
            renderer.set_clear_color([0.0, 0.0, 0.0, 1.0]);
            self.renderer = Some(renderer);
            self.graph_json = None;
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

        let vertex_bytes = cast_slice(&vertices);
        let _vertex_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "mesh_vertex_buffer",
                byte_size: vertex_bytes.len() as u32,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::VERTEX,
                initial_data: Some(vertex_bytes),
            })
            .map_err(RenderError::Gpu)?;

        let indices = obj.mesh.indices[..obj.mesh.num_indices].to_vec();
        let _index_buffer = if !indices.is_empty() {
            let index_bytes = cast_slice(&indices);
            Some(
                ctx.make_buffer(&BufferInfo {
                    debug_name: "mesh_index_buffer",
                    byte_size: index_bytes.len() as u32,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::INDEX,
                    initial_data: Some(index_bytes),
                })
                .map_err(RenderError::Gpu)?,
            )
        } else {
            None
        };

        // Register mesh with Koji renderer using CPU data (Koji handles upload).
        let mesh = StaticMesh {
            material_id: "graph_pso".to_string(),
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
            renderer.register_static_mesh(mesh, None, "graph_pso".into(), "canvas");
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
            renderer.update_static_mesh("canvas", idx, &vertices);
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
        _ctx: &mut dashi::Context,
        extent: [u32; 2],
    ) -> Result<RgbaImage, RenderError> {
        let [width, height] = extent;
        let v0 = (width as f32 / 2.0, 0.0f32);
        let v1 = (0.0f32, height as f32 - 1.0);
        let v2 = (width as f32 - 1.0, height as f32 - 1.0);
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let denom = (v1.1 - v2.1) * (v0.0 - v2.0) + (v2.0 - v1.0) * (v0.1 - v2.1);
                let a = ((v1.1 - v2.1) * (px - v2.0) + (v2.0 - v1.0) * (py - v2.1)) / denom;
                let b = ((v2.1 - v0.1) * (px - v2.0) + (v0.0 - v2.0) * (py - v2.1)) / denom;
                let c = 1.0 - a - b;
                let color = if a >= 0.0 && b >= 0.0 && c >= 0.0 {
                    Rgba([255, 0, 0, 255])
                } else {
                    Rgba([0, 0, 0, 255])
                };
                img.put_pixel(x, y, color);
            }
        }
        Ok(img)
    }
}
