use super::RenderError;
use crate::object::MeshObject;
use crate::render::database::Vertex as MeshVertex;
use bytemuck::cast_slice;
use dashi::{BufferInfo, BufferUsage, Format, MemoryVisibility};
use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::{Renderer, StaticMesh, Vertex as KojiVertex};
use koji::{CanvasBuilder, PipelineBuilder};

pub struct CanvasRenderer {
    extent: Option<[u32; 2]>,
    renderer: Option<Renderer>,
    display: Option<dashi::Display>,
    next_mesh: usize,
}

impl CanvasRenderer {
    pub fn new(extent: Option<[u32; 2]>) -> Self {
        Self {
            extent,
            renderer: None,
            display: None,
            next_mesh: 0,
        }
    }

    pub fn init(&mut self, ctx: &mut dashi::Context) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            if self.display.is_none() {
                if let Ok(d) = ctx.make_display(&Default::default()) {
                    self.display = Some(d);
                }
            }

            let [width, height] = if let Some(extent) = self.extent {
                extent
            } else if let Some(display) = self.display.as_ref() {
                let p = display.winit_window().inner_size();
                [p.width, p.height]
            } else {
                [1024, 1024]
            };

            let canvas = CanvasBuilder::new()
                .extent([width, height])
                .color_attachment("color", Format::RGBA8)
                .build(ctx)?;

            let mut renderer = Renderer::with_canvas(width, height, ctx, canvas.clone())?;

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

            let pso = PipelineBuilder::new(ctx, "canvas_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((canvas.render_pass(), 0))
                .build_with_resources(renderer.resources())
                .map_err(|_| RenderError::Gpu(dashi::GPUError::LibraryError()))?;

            renderer.register_pipeline_for_pass("main", pso, [None, None, None, None]);

            self.renderer = Some(renderer);
        }
        Ok(())
    }

    pub fn take_display(&mut self) -> Option<dashi::Display> {
        self.display.take()
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
