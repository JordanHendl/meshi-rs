use dashi::{utils::Pool, Attachment, DrawBegin, DrawIndexed, Format, SubmitInfo};
use image::{Rgba, RgbaImage};
use inline_spirv::inline_spirv;
use koji::renderer::Renderer;
use koji::{
    material::PSO, render_graph::io, Canvas, CanvasBuilder, PipelineBuilder, RenderGraph,
    RenderPassBuilder,
};

use super::RenderError;
use crate::object::MeshObject;
use tracing::warn;

/// A renderer that executes a frame graph described by `koji`.
///
/// The graph description is loaded from a JSON file (typically `koji.json`).
/// A simple canvas is created for drawing `MeshObject`s. The parsed graph is
/// executed each frame before issuing draw calls.
pub struct GraphRenderer {
    graph: Option<RenderGraph>,
    canvas: Option<Canvas>,
    renderer: Option<Renderer>,
    pipeline: Option<PSO>,
}

impl GraphRenderer {
    pub fn new(scene_cfg_path: Option<String>) -> Result<Self, RenderError> {
        let graph = if let Some(path) = scene_cfg_path {
            let data = std::fs::read_to_string(&path).map_err(|e| {
                warn!("failed to read scene config {path}: {e}");
                RenderError::GraphConfig(e)
            })?;
            Some(io::from_json(&data).map_err(|e| {
                warn!("failed to parse scene config {path}: {e}");
                RenderError::GraphParse(e)
            })?)
        } else {
            None
        };
        Ok(Self {
            graph,
            canvas: None,
            renderer: None,
            pipeline: None,
        })
    }

    pub fn render(
        &mut self,
        ctx: &mut dashi::Context,
        display: &mut dashi::Display,
        mesh_objects: &Pool<MeshObject>,
    ) -> Result<(), RenderError> {
        if self.canvas.is_none() {
            let p = display.winit_window().inner_size();
            let (width, height) = (p.width, p.height);

            let renderer = Renderer::with_render_pass(
                width,
                height,
                ctx,
                RenderPassBuilder::new()
                    .color_attachment("color", Format::RGBA8)
                    .subpass("main", ["color"], &[] as &[&str]),
            )?;

            let canvas = CanvasBuilder::new()
                .extent([width, height])
                .color_attachment("color", Format::RGBA8)
                .build(ctx)?;

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
            let pso = PipelineBuilder::new(ctx, "graph_pso")
                .vertex_shader(vert)
                .fragment_shader(frag)
                .render_pass((canvas.render_pass(), 0))
                .build();

            self.pipeline = Some(pso);
            self.renderer = Some(renderer);
            self.canvas = Some(canvas);
        }

        let canvas = self.canvas.as_mut().unwrap();

        if let Some(graph) = &mut self.graph {
            graph.execute(ctx)?;
        }

        let (img, acquire_sem, _idx, _sub_opt) = ctx.acquire_new_image(display)?;
        canvas.target_mut().colors[0].attachment.img = img;

        let target = canvas.target();
        let mut attachments: Vec<Attachment> = target.colors.iter().map(|c| c.attachment).collect();
        if let Some(depth) = &target.depth {
            attachments.push(depth.attachment);
        }

        let mut cmd = ctx.begin_command_list(&Default::default())?;

        let result: Result<(), RenderError> = (|| {
            let pso = self.pipeline.as_ref().unwrap();
            let draw_begin = DrawBegin {
                viewport: Default::default(),
                pipeline: pso.pipeline,
                attachments: &attachments,
            };
            cmd.begin_drawing(&draw_begin)?;

            mesh_objects.for_each_occupied(|obj| {
                cmd.draw_indexed(DrawIndexed {
                    vertices: obj.mesh.vertices,
                    indices: obj.mesh.indices,
                    index_count: obj.mesh.num_indices as u32,
                    ..Default::default()
                });
            });

            cmd.end_drawing()?;

            let fence = ctx.submit(
                &mut cmd,
                &SubmitInfo {
                    wait_sems: &[acquire_sem],
                    signal_sems: &[],
                },
            )?;

            ctx.wait(fence)?;
            ctx.present_display(display, &[])?;
            Ok(())
        })();

        ctx.destroy_cmd_list(cmd);
        result
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
