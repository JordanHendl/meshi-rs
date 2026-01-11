use resource_pool::{Handle, resource_list::ResourceList};
use tracing::warn;

use crate::{TextInfo, TextObject};

#[derive(Clone, Debug)]
pub struct TextDraw {
    pub text: String,
    pub position: glam::Vec2,
    pub color: glam::Vec4,
    pub scale: f32,
}

#[derive(Clone, Debug)]
struct TextObjectData {
    info: TextInfo,
    dirty: bool,
}

pub struct TextRenderer {
    objects: ResourceList<TextObjectData>,
    draws: Vec<TextDraw>,
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
        }
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
            let obj = self.objects.get_ref_mut(handle);
            self.draws.push(TextDraw {
                text: obj.info.text.clone(),
                position: obj.info.position,
                color: obj.info.color,
                scale: obj.info.scale,
            });
            obj.dirty = false;
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
