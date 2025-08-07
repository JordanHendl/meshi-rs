use dashi::{utils::Pool, Attachment, DrawIndexed, Format, RenderPassBegin, SubmitInfo};
use koji::{render_graph::io, Canvas, CanvasBuilder, RenderGraph};

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
        })
    }

    pub fn render(
        &mut self,
        ctx: &mut dashi::Context,
        display: &mut dashi::Display,
        mesh_objects: &Pool<MeshObject>,
    ) -> Result<(), RenderError> {
        if self.canvas.is_none() {
            let canvas = CanvasBuilder::new()
                .color_attachment("color", Format::RGBA8)
                .build(ctx)?;
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
            cmd.begin_render_pass(&RenderPassBegin {
                render_pass: canvas.render_pass(),
                viewport: Default::default(),
                attachments: &attachments,
            })?;

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
}
