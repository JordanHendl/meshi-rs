use resource_pool::{Handle, resource_list::ResourceList};

use crate::{GuiInfo, GuiObject};

#[derive(Clone, Debug)]
struct GuiObjectData {
    info: GuiInfo,
    dirty: bool,
}

pub struct GuiRenderer {
    objects: ResourceList<GuiObjectData>,
}

fn to_handle(handle: Handle<GuiObjectData>) -> Handle<GuiObject> {
    Handle::new(handle.slot, handle.generation)
}

fn from_handle(handle: Handle<GuiObject>) -> Handle<GuiObjectData> {
    Handle::new(handle.slot, handle.generation)
}

impl GuiRenderer {
    pub fn new() -> Self {
        Self {
            objects: ResourceList::default(),
        }
    }

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        let handle = self.objects.push(GuiObjectData {
            info: info.clone(),
            dirty: true,
        });

        to_handle(handle)
    }

    pub fn release_gui(&mut self, handle: Handle<GuiObject>) {
        if !handle.valid() {
            return;
        }

        let handle = from_handle(handle);
        if !self.objects.entries.iter().any(|entry| entry.slot == handle.slot) {
            return;
        }

        self.objects.release(handle);
    }

    pub fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        if !handle.valid() {
            return;
        }

        let handle = from_handle(handle);
        if !self.objects.entries.iter().any(|entry| entry.slot == handle.slot) {
            return;
        }

        let object = self.objects.get_ref_mut(handle);
        object.info = info.clone();
        object.dirty = true;
    }
}
