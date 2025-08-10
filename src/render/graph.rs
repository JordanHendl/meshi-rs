use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::{Renderer, StaticMesh, Vertex as KojiVertex};
use koji::{render_graph::io, PipelineBuilder, RenderGraph};
use dashi::{BufferInfo, BufferUsage, MemoryVisibility};
use bytemuck::cast_slice;

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
                    return Err(RenderError::GraphConfig(e));
                }
            }
        } else {
            None
        };
        Ok(Self { graph_json, renderer: None, next_mesh: 0, headless })
    }

    fn init(&mut self, ctx: &mut dashi::Context) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            let (width, height) = (1, 1);

            let graph = if let Some(json) = &self.graph_json {
                io::from_json(json).map_err(RenderError::GraphParse)?
            } else {
                RenderGraph::new()
            };

            let mut renderer = if self.headless {
                Renderer::with_graph(width, height, ctx, graph)?
            } else {
                Renderer::with_graph_headless(width, height, ctx, graph)?
            };

            let vert = inline_spirv!(
                r#"#version 450
                layout(location=0) in vec4 position;
                void main() { gl_Position = position; }
                "#,
                vert
            );
            let frag = inline_spirv!(
                r#"#version 450
                layout(location=0) out vec4 color;
                void main() { color = vec4(1.0,1.0,1.0,1.0); }
                "#,
                frag
            );

            let (pass, _) = renderer
                .graph()
                .render_pass_for_output("swapchain")
                .expect("missing swapchain output");
            let mut pso = PipelineBuilder::new(ctx, "graph_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((pass, 0))
                .build_with_resources(renderer.resources())
                .unwrap();
            let bgr = pso.create_bind_groups(renderer.resources()).unwrap();
            renderer.register_material_pipeline("graph_pso", pso, bgr);
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
            indices: if indices.is_empty() { None } else { Some(indices) },
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
        };

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.register_static_mesh(mesh, None, "graph_pso".into());
        }

        let idx = self.next_mesh;
        self.next_mesh += 1;
        Ok(idx)
    }

    pub fn update_mesh(
        &mut self,
        ctx: &mut dashi::Context,
        idx: usize,
        obj: &MeshObject,
    ) {
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
