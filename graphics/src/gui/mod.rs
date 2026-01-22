//! GUI rendering entry points.
//!
//! Expected entry points:
//! - **Initialization**: create a [`GuiContext`] tied to the renderer/device.
//! - **Registration**: register GUI layers, fonts, and resources up front.
//! - **Draw submission**: submit [`GuiDraw`] records each frame for rendering.
//! - **Frame build**: generate a sorted, batched mesh for GPU upload.

use std::ops::Range;

use crate::render::gui::{GuiMesh, GuiVertex};

/// Primary GUI state owned by the renderer/user layer.
#[derive(Debug, Default)]
pub struct GuiContext {
    draws: Vec<GuiDraw>,
}

impl GuiContext {
    /// Initialize the GUI context.
    pub fn new() -> Self {
        Self { draws: Vec::new() }
    }

    /// Register GUI resources or layer configurations.
    pub fn register_layer(&mut self, _layer: GuiLayer) {}

    /// Submit a draw call to be collected for this frame.
    pub fn submit_draw(&mut self, draw: GuiDraw) {
        self.draws.push(draw);
    }

    /// Build a frame mesh by sorting by layer and grouping by texture id.
    pub fn build_frame(&mut self) -> GuiFrame {
        if self.draws.is_empty() {
            return GuiFrame::default();
        }

        let mut indexed: Vec<(usize, GuiDraw)> = self.draws.drain(..).enumerate().collect();
        indexed.sort_by(|(a_index, a), (b_index, b)| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| a.texture_id.cmp(&b.texture_id))
                .then_with(|| a_index.cmp(b_index))
        });

        let mut batches: Vec<GuiBatchMesh> = Vec::new();
        let mut current_batch: Option<GuiBatch> = None;
        let mut current_mesh = GuiMesh::default();

        for (_, draw) in indexed {
            let needs_new_batch = current_batch.as_ref().map_or(true, |batch| {
                batch.layer != draw.layer
                    || batch.texture_id != draw.texture_id
                    || batch.clip_rect != draw.clip_rect
            });

            if needs_new_batch {
                if let Some(batch) = current_batch.take() {
                    batches.push(GuiBatchMesh {
                        batch,
                        mesh: current_mesh,
                    });
                }

                current_mesh = GuiMesh::default();
                current_batch = Some(GuiBatch {
                    layer: draw.layer,
                    texture_id: draw.texture_id,
                    index_range: 0..0,
                    clip_rect: draw.clip_rect,
                });
            }

            let base_vertex = current_mesh.vertices.len() as u32;
            current_mesh.vertices
                .extend(draw.quad.vertices().into_iter().map(|vertex| GuiVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    color: vertex.color,
                }));
            current_mesh.indices.extend_from_slice(&[
                base_vertex,
                base_vertex + 1,
                base_vertex + 2,
                base_vertex + 2,
                base_vertex + 3,
                base_vertex,
            ]);

            if let Some(batch) = current_batch.as_mut() {
                let end = current_mesh.indices.len() as u32;
                if batch.index_range.end == 0 {
                    batch.index_range = (end - 6)..end;
                } else {
                    batch.index_range.end = end;
                }
            }
        }

        if let Some(batch) = current_batch.take() {
            batches.push(GuiBatchMesh {
                batch,
                mesh: current_mesh,
            });
        }

        GuiFrame { batches }
    }
}

/// Minimal draw submission payload for GUI rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiDraw {
    pub layer: GuiLayer,
    pub texture_id: u32,
    pub quad: GuiQuad,
    pub clip_rect: Option<GuiClipRect>,
}

impl GuiDraw {
    pub fn new(layer: GuiLayer, texture_id: u32, quad: GuiQuad) -> Self {
        Self {
            layer,
            texture_id,
            quad,
            clip_rect: None,
        }
    }

    pub fn with_clip_rect(
        layer: GuiLayer,
        texture_id: u32,
        quad: GuiQuad,
        clip_rect: GuiClipRect,
    ) -> Self {
        Self {
            layer,
            texture_id,
            quad,
            clip_rect: Some(clip_rect),
        }
    }
}

/// GUI compositing layers, ordered back-to-front by convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GuiLayer {
    Background,
    World,
    Overlay,
}

/// A single GUI quad (two triangles) worth of data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiQuad {
    pub positions: [[f32; 2]; 4],
    pub uvs: [[f32; 2]; 4],
    pub color: [f32; 4],
}

impl GuiQuad {
    pub fn vertices(&self) -> [GuiVertexData; 4] {
        [
            GuiVertexData {
                position: self.positions[0],
                uv: self.uvs[0],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[1],
                uv: self.uvs[1],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[2],
                uv: self.uvs[2],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[3],
                uv: self.uvs[3],
                color: self.color,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy)]
struct GuiVertexData {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Clip rectangle for GUI draw submissions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiClipRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl GuiClipRect {
    pub fn from_min_max(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    pub fn from_position_size(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            min: position,
            max: [position[0] + size[0], position[1] + size[1]],
        }
    }
}

/// A frame-ready GUI mesh plus batch metadata.
#[derive(Debug, Default)]
pub struct GuiFrame {
    pub batches: Vec<GuiBatchMesh>,
}

/// Consecutive indices sharing the same layer and texture binding.
#[derive(Debug, Clone, PartialEq)]
pub struct GuiBatch {
    pub layer: GuiLayer,
    pub texture_id: u32,
    pub index_range: Range<u32>,
    pub clip_rect: Option<GuiClipRect>,
}

#[derive(Debug, Clone)]
pub struct GuiBatchMesh {
    pub batch: GuiBatch,
    pub mesh: GuiMesh,
}
