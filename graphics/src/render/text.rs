use std::{collections::HashMap, ptr::NonNull};

use bento::builder::{AttachmentDesc, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use bytemuck::{Pod, Zeroable};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::Draw;
use dashi::{
    Buffer, BufferInfo, BufferUsage, BufferView, CommandStream, Context, DepthInfo, Format,
    GraphicsPipelineDetails, Handle as DashiHandle, IndexedResource, MemoryVisibility, SampleCount,
    ShaderResource, ShaderType, Viewport,
};
use furikake::PSOBuilderFurikakeExt;
use noren::{DB, meta::DeviceSDFFont};
use resource_pool::{Handle, resource_list::ResourceList};
use tracing::{error, warn};

use crate::{TextInfo, TextObject, TextRenderMode};

#[derive(Clone, Debug)]
pub enum TextDrawMode {
    Plain,
    Sdf {
        font_entry: String,
        font: Option<DeviceSDFFont>,
    },
}

#[derive(Clone, Debug)]
pub struct TextDraw {
    pub text: String,
    pub position: glam::Vec2,
    pub color: glam::Vec4,
    pub scale: f32,
    pub mode: TextDrawMode,
}

#[derive(Clone, Debug)]
struct TextObjectData {
    info: TextInfo,
    dirty: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct TextGlyph {
    origin: [f32; 2],
    size: [f32; 2],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    color: [f32; 4],
    texture_id: u32,
    _padding: [u32; 3],
}

pub struct TextRenderer {
    objects: ResourceList<TextObjectData>,
    draws: Vec<TextDraw>,
    sdf_fonts: HashMap<String, DeviceSDFFont>,
    db: Option<NonNull<DB>>,
    text_pso: Option<bento::builder::PSO>,
    glyph_buffer: Option<DashiHandle<Buffer>>,
    glyph_capacity: usize,
}

fn to_handle(h: Handle<TextObjectData>) -> Handle<TextObject> {
    Handle::new(h.slot, h.generation)
}

fn from_handle(h: Handle<TextObject>) -> Handle<TextObjectData> {
    Handle::new(h.slot, h.generation)
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            objects: ResourceList::default(),
            draws: Vec::new(),
            sdf_fonts: HashMap::new(),
            db: None,
            text_pso: None,
            glyph_buffer: None,
            glyph_capacity: 4096,
        }
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
    }

    pub fn initialize_renderer(
        &mut self,
        ctx: &mut Context,
        state: &mut furikake::BindlessState,
        sample_count: SampleCount,
    ) {
        if self.text_pso.is_some() {
            return;
        }

        let glyph_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI] Text Glyph Buffer",
                byte_size: (std::mem::size_of::<TextGlyph>() * self.glyph_capacity) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create text glyph buffer");

        let text_pso = Self::build_text_pipeline(ctx, state, sample_count, glyph_buffer);
        state.register_pso_tables(&text_pso);

        self.text_pso = Some(text_pso);
        self.glyph_buffer = Some(glyph_buffer);
    }

    fn build_text_pipeline(
        ctx: &mut Context,
        state: &mut furikake::BindlessState,
        sample_count: SampleCount,
        glyph_buffer: DashiHandle<Buffer>,
    ) -> bento::builder::PSO {
        let compiler = Compiler::new().expect("Failed to create shader compiler");
        let base_request = Request {
            name: Some("meshi_text".to_string()),
            lang: ShaderLang::Slang,
            stage: ShaderType::Vertex,
            optimization: OptimizationLevel::Performance,
            debug_symbols: true,
            defines: HashMap::new(),
        };

        let vertex = compiler
            .compile(
                include_str!("shaders/text_vert.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Vertex,
                    ..base_request.clone()
                },
            )
            .expect("Failed to compile text vertex shader");
        let fragment = compiler
            .compile(
                include_str!("shaders/text_frag.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Fragment,
                    ..base_request
                },
            )
            .expect("Failed to compile text fragment shader");

        PSOBuilder::new()
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
            .add_table_variable_with_resources(
                "text_glyph_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(glyph_buffer.into()),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .unwrap()
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: sample_count,
            })
            .set_attachment_format(0, Format::BGRA8)
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count,
                depth_test: Some(DepthInfo {
                    should_test: false,
                    should_write: false,
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build text pipeline")
    }

    fn fetch_sdf_font(&mut self, entry: &str) -> Option<DeviceSDFFont> {
        if let Some(font) = self.sdf_fonts.get(entry) {
            return Some(font.clone());
        }

        let Some(mut db) = self.db else {
            warn!(
                "Attempted to fetch SDF font '{}' without a database.",
                entry
            );
            return None;
        };

        match unsafe { db.as_mut() }.fetch_gpu_sdf_font(entry) {
            Ok(font) => {
                self.sdf_fonts.insert(entry.to_string(), font.clone());
                Some(font)
            }
            Err(err) => {
                warn!("Failed to fetch SDF font '{}': {:?}", entry, err);
                None
            }
        }
    }

    fn build_text_glyphs(&self, draws: &[TextDraw], viewport: &Viewport) -> Vec<TextGlyph> {
        let mut glyphs = Vec::new();
        let screen_w = viewport.area.w.max(1.0);
        let screen_h = viewport.area.h.max(1.0);

        for draw in draws {
            let TextDrawMode::Sdf { font, .. } = &draw.mode else {
                continue;
            };
            let Some(font) = font else {
                continue;
            };
            let Some(texture_id) = font.furikake_texture_id else {
                warn!("SDF font '{}' is missing a bindless texture id.", font.name);
                continue;
            };

            let atlas_w = font.image.info.dim[0].max(1) as f32;
            let atlas_h = font.image.info.dim[1].max(1) as f32;
            let scale = draw.scale;
            let mut pen_x = draw.position.x;
            let mut baseline_y = draw.position.y + font.font.metrics.ascender * scale;

            let glyph_map: HashMap<u32, _> = font
                .font
                .glyphs
                .iter()
                .map(|glyph| (glyph.unicode, glyph))
                .collect();

            for ch in draw.text.chars() {
                if ch == '\n' {
                    pen_x = draw.position.x;
                    baseline_y += font.font.metrics.line_height * scale;
                    continue;
                }

                let Some(glyph) = glyph_map.get(&(ch as u32)) else {
                    continue;
                };

                let Some(plane_bounds) = &glyph.plane_bounds else {
                    pen_x += glyph.advance * scale;
                    continue;
                };
                let Some(atlas_bounds) = &glyph.atlas_bounds else {
                    pen_x += glyph.advance * scale;
                    continue;
                };

                let x0 = pen_x + plane_bounds.left * scale;
                let x1 = pen_x + plane_bounds.right * scale;
                let y0 = baseline_y - plane_bounds.top * scale;
                let y1 = baseline_y - plane_bounds.bottom * scale;

                let ndc_x0 = (x0 / screen_w) * 2.0 - 1.0;
                let ndc_x1 = (x1 / screen_w) * 2.0 - 1.0;
                let ndc_y0 = 1.0 - (y0 / screen_h) * 2.0;
                let ndc_y1 = 1.0 - (y1 / screen_h) * 2.0;

                let uv_min = [atlas_bounds.left / atlas_w, atlas_bounds.bottom / atlas_h];
                let uv_max = [atlas_bounds.right / atlas_w, atlas_bounds.top / atlas_h];

                glyphs.push(TextGlyph {
                    origin: [ndc_x0, ndc_y0],
                    size: [ndc_x1 - ndc_x0, ndc_y1 - ndc_y0],
                    uv_min,
                    uv_max,
                    color: draw.color.to_array(),
                    texture_id: texture_id as u32,
                    _padding: [0; 3],
                });

                pen_x += glyph.advance * scale;
            }
        }

        glyphs
    }

    fn upload_text_glyphs(&mut self, ctx: &mut Context, glyphs: &[TextGlyph]) -> usize {
        if glyphs.is_empty() {
            return 0;
        }

        let Some(buffer) = self.glyph_buffer else {
            return 0;
        };

        let count = glyphs.len().min(self.glyph_capacity);
        if glyphs.len() > self.glyph_capacity {
            warn!(
                "Text glyph buffer overflow ({} > {}), truncating.",
                glyphs.len(),
                self.glyph_capacity
            );
        }

        let mapped = ctx
            .map_buffer_mut::<TextGlyph>(BufferView::new(buffer))
            .expect("Failed to map text glyph buffer");
        mapped[..count].copy_from_slice(&glyphs[..count]);
        ctx.unmap_buffer(buffer)
            .expect("Failed to unmap text glyph buffer");
        count
    }

    pub fn render_transparent(
        &mut self,
        ctx: &mut Context,
        viewport: &Viewport,
    ) -> CommandStream<PendingGraphics> {
        let draws = self.emit_draws().to_vec();
        let glyphs = self.build_text_glyphs(&draws, viewport);
        let glyph_count = self.upload_text_glyphs(ctx, &glyphs);
        
        let mut cmd = CommandStream::<PendingGraphics>::subdraw();
        let Some(pso) = self.text_pso.as_ref() else {
            error!("Failed to  build text without a text pso");
            return cmd;
        };

        if glyph_count == 0 {
            return cmd;
        }

        cmd = cmd
            .bind_graphics_pipeline(pso.handle)
            .draw(&Draw {
                bind_tables: pso.tables(),
                count: 6,
                instance_count: glyph_count as u32,
                ..Default::default()
            })
            .unbind_graphics_pipeline();

        cmd
    }

    pub fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        let h = self.objects.push(TextObjectData {
            info: info.clone(),
            dirty: true,
        });
        to_handle(h)
    }

    pub fn release_text(&mut self, handle: Handle<TextObject>) {
        if !handle.valid() {
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        self.objects.release(from_handle(handle));
    }

    pub fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        if !handle.valid() {
            warn!("Attempted to update text on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update text for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));
        obj.info.text.clear();
        obj.info.text.push_str(text);
        obj.dirty = true;
    }

    pub fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        if !handle.valid() {
            warn!("Attempted to update text info on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update text info for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));
        obj.info = info.clone();
        obj.dirty = true;
    }

    pub fn build_draws(&mut self) {
        self.draws.clear();

        let handles: Vec<_> = self.objects.entries.clone();
        for handle in handles {
            let info = {
                let obj = self.objects.get_ref_mut(handle);
                obj.dirty = false;
                obj.info.clone()
            };
            let mode = match &info.render_mode {
                TextRenderMode::Plain => TextDrawMode::Plain,
                TextRenderMode::Sdf { font } => TextDrawMode::Sdf {
                    font_entry: font.clone(),
                    font: self.fetch_sdf_font(font),
                },
            };

            self.draws.push(TextDraw {
                text: info.text,
                position: info.position,
                color: info.color,
                scale: info.scale,
                mode,
            });
        }
    }

    pub fn emit_draws(&mut self) -> &[TextDraw] {
        let needs_rebuild = self
            .objects
            .entries
            .iter()
            .any(|h| self.objects.get_ref(*h).dirty);

        if needs_rebuild {
            self.build_draws();
        }

        &self.draws
    }
}
