use super::RenderError;
use crate::object::MeshObject;
use dashi::{utils::Pool, Format};
use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::{Renderer, StaticMesh};
use koji::{CanvasBuilder, PipelineBuilder};

pub struct CanvasRenderer {
    extent: Option<[u32; 2]>,
    renderer: Option<Renderer>,
}

impl CanvasRenderer {
    pub fn new(extent: Option<[u32; 2]>) -> Self {
        Self {
            extent,
            renderer: None,
        }
    }

    pub fn render(
        &mut self,
        ctx: &mut dashi::Context,
        display: &mut dashi::Display,
        mesh_objects: &Pool<MeshObject>,
    ) -> Result<(), RenderError> {
        if self.renderer.is_none() {
            let [width, height] = if let Some(extent) = self.extent {
                extent
            } else {
                let p = display.winit_window().inner_size();
                [p.width, p.height]
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

            mesh_objects.for_each_occupied(|obj| {
                let raw_vertices: &[MeshVertex] =
                    ctx.map_buffer(obj.mesh.vertices).expect("map vertices");
                let vertices: Vec<koji::renderer::Vertex> = raw_vertices[..obj.mesh.num_vertices]
                    .iter()
                    .map(|v| koji::renderer::Vertex {
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

                let mesh = StaticMesh {
                    material_id: String::new(),
                    vertices,
                    indices: Some(indices),
                    vertex_buffer: None,
                    index_buffer: None,
                    index_count: 0,
                };
                renderer.register_static_mesh(mesh, None, "color".into());
            });

            self.renderer = Some(renderer);
        }

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.present_frame()?;
        }

        Ok(())
    }

    pub fn render_to_image(
        &mut self,
        _ctx: &mut dashi::Context,
        _mesh_objects: &Pool<MeshObject>,
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
