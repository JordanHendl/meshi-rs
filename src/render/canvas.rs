use super::RenderError;
use crate::object::MeshObject;
use dashi::Format;
use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::{Renderer, StaticMesh, Vertex};
use koji::{CanvasBuilder, PipelineBuilder};

pub struct CanvasRenderer {
    extent: Option<[u32; 2]>,
    renderer: Option<Renderer>,
    next_mesh: usize,
}

impl CanvasRenderer {
    pub fn new(extent: Option<[u32; 2]>) -> Self {
        Self { extent, renderer: None, next_mesh: 0 }
    }

    fn init(
        &mut self,
        ctx: &mut dashi::Context,
        display: Option<&mut dashi::Display>,
    ) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            let [width, height] = if let Some(extent) = self.extent {
                extent
            } else if let Some(display) = display {
                let p = display.winit_window().inner_size();
                [p.width, p.height]
            } else {
                [1, 1]
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

            let mut pso = PipelineBuilder::new(ctx, "canvas_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((canvas.render_pass(), 0))
                .build_with_resources(renderer.resources())
                .map_err(|_| RenderError::Gpu(dashi::GPUError::LibraryError()))?;
            let bgr = pso.create_bind_groups(renderer.resources()).unwrap();
            renderer.register_pipeline_for_pass("main", pso, bgr);

            self.renderer = Some(renderer);
        }
        Ok(())
    }

    pub fn register_mesh(
        &mut self,
        ctx: &mut dashi::Context,
        display: Option<&mut dashi::Display>,
        obj: &MeshObject,
    ) -> Result<usize, RenderError> {
        self.init(ctx, display)?;

        #[repr(C)]
        #[derive(Clone, Copy)]
        struct MeshVertex {
            position: [f32; 4],
            normal: [f32; 4],
            tex_coords: [f32; 2],
            joint_ids: [i32; 4],
            joints: [f32; 4],
            color: [f32; 4],
        }

        let raw_vertices: &[MeshVertex] = ctx.map_buffer(obj.mesh.vertices).expect("map vertices");
        let vertices: Vec<Vertex> = raw_vertices[..obj.mesh.num_vertices]
            .iter()
            .map(|v| Vertex {
                position: [v.position[0], v.position[1], v.position[2]],
                normal: [v.normal[0], v.normal[1], v.normal[2]],
                tangent: [0.0, 0.0, 0.0, 0.0],
                uv: [v.tex_coords[0], v.tex_coords[1]],
                color: [v.color[0], v.color[1], v.color[2], v.color[3]],
            })
            .collect();
        ctx.unmap_buffer(obj.mesh.vertices).expect("unmap vertices");

        let raw_indices: &[u32] = ctx.map_buffer(obj.mesh.indices).expect("map indices");
        let indices = raw_indices[..obj.mesh.num_indices].to_vec();
        ctx.unmap_buffer(obj.mesh.indices).expect("unmap indices");

        let material_id = obj
            .targets
            .get(0)
            .map(|t| t.material.clone())
            .unwrap_or_default();
        let mesh = StaticMesh {
            material_id: material_id.clone(),
            vertices,
            indices: Some(indices),
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
        };

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.register_static_mesh(mesh, None, material_id);
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
        if self.init(ctx, None).is_err() {
            return;
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        struct MeshVertex {
            position: [f32; 4],
            normal: [f32; 4],
            tex_coords: [f32; 2],
            joint_ids: [i32; 4],
            joints: [f32; 4],
            color: [f32; 4],
        }

        let raw_vertices: &[MeshVertex] = ctx.map_buffer(obj.mesh.vertices).expect("map vertices");
        let vertices: Vec<Vertex> = raw_vertices[..obj.mesh.num_vertices]
            .iter()
            .map(|v| Vertex {
                position: [v.position[0], v.position[1], v.position[2]],
                normal: [v.normal[0], v.normal[1], v.normal[2]],
                tangent: [0.0, 0.0, 0.0, 0.0],
                uv: [v.tex_coords[0], v.tex_coords[1]],
                color: [v.color[0], v.color[1], v.color[2], v.color[3]],
            })
            .collect();
        ctx.unmap_buffer(obj.mesh.vertices).expect("unmap vertices");

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_static_mesh(idx, &vertices);
        }
    }

    pub fn render(
        &mut self,
        ctx: &mut dashi::Context,
        display: Option<&mut dashi::Display>,
    ) -> Result<(), RenderError> {
        self.init(ctx, display)?;
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
