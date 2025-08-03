use dashi::{utils::Pool, Attachment, DrawIndexed, Format, RenderPassBegin, SubmitInfo};
use koji::{Canvas, CanvasBuilder};

use crate::object::MeshObject;

pub struct CanvasRenderer {
    canvas: Option<Canvas>,
}

impl CanvasRenderer {
    pub fn new() -> Self {
        Self { canvas: None }
    }

    pub fn render(
        &mut self,
        ctx: &mut dashi::Context,
        display: &mut dashi::Display,
        mesh_objects: &Pool<MeshObject>,
    ) {
        let canvas = self.canvas.get_or_insert_with(|| {
            CanvasBuilder::new()
                .color_attachment("color", Format::RGBA8)
                .build(ctx)
                .expect("failed to build canvas")
        });

        let (img, acquire_sem, _idx, _sub_opt) = ctx
            .acquire_new_image(display)
            .expect("failed to acquire image");
        canvas.target_mut().colors[0].attachment.img = img;

        let target = canvas.target();
        let mut attachments: Vec<Attachment> = target.colors.iter().map(|c| c.attachment).collect();
        if let Some(depth) = &target.depth {
            attachments.push(depth.attachment);
        }

        let mut cmd = ctx
            .begin_command_list(&Default::default())
            .expect("begin command list");

        cmd.begin_render_pass(&RenderPassBegin {
            render_pass: canvas.render_pass(),
            viewport: Default::default(),
            attachments: &attachments,
        })
        .expect("begin render pass");

        mesh_objects.for_each_occupied(|obj| {
            cmd.draw_indexed(DrawIndexed {
                vertices: obj.mesh.vertices,
                indices: obj.mesh.indices,
                index_count: obj.mesh.num_indices as u32,
                ..Default::default()
            });
        });

        // ensure render pass closed
        let _ = cmd.end_drawing();

        let fence = ctx
            .submit(
                &mut cmd,
                &SubmitInfo {
                    wait_sems: &[acquire_sem],
                    signal_sems: &[],
                },
            )
            .expect("submit");

        let _ = ctx.wait(fence);
        let _ = ctx.present_display(display, &[]);
        ctx.destroy_cmd_list(cmd);
    }
}
