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

        let mut mesh = GuiMesh::default();
        let mut batches: Vec<GuiBatch> = Vec::new();

        for (_, draw) in indexed {
            if batches
                .last()
                .map(|batch| batch.layer != draw.layer || batch.texture_id != draw.texture_id)
                .unwrap_or(true)
            {
                batches.push(GuiBatch {
                    layer: draw.layer,
                    texture_id: draw.texture_id,
                    index_range: 0..0,
                });
            }

            let base_vertex = mesh.vertices.len() as u32;
            mesh.vertices
                .extend(draw.quad.vertices().into_iter().map(|vertex| GuiVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    color: vertex.color,
                }));
            mesh.indices.extend_from_slice(&[
                base_vertex,
                base_vertex + 1,
                base_vertex + 2,
                base_vertex + 2,
                base_vertex + 3,
                base_vertex,
            ]);

            if let Some(batch) = batches.last_mut() {
                let end = mesh.indices.len() as u32;
                if batch.index_range.end == 0 {
                    batch.index_range = (end - 6)..end;
                } else {
                    batch.index_range.end = end;
                }
            }
        }

        GuiFrame { mesh, batches }
    }
}

/// Minimal draw submission payload for GUI rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiDraw {
    pub layer: GuiLayer,
    pub texture_id: u32,
    pub quad: GuiQuad,
}

impl GuiDraw {
    pub fn new(layer: GuiLayer, texture_id: u32, quad: GuiQuad) -> Self {
        Self {
            layer,
            texture_id,
            quad,
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

/// A frame-ready GUI mesh plus batch metadata.
#[derive(Debug, Default)]
pub struct GuiFrame {
    pub mesh: GuiMesh,
    pub batches: Vec<GuiBatch>,
}

/// Consecutive indices sharing the same layer and texture binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiBatch {
    pub layer: GuiLayer,
    pub texture_id: u32,
    pub index_range: Range<u32>,
}
