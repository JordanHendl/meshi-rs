//! GUI rendering entry points.
//!
//! Expected entry points:
//! - **Initialization**: create a [`GuiContext`] tied to the renderer/device.
//! - **Registration**: register GUI layers, fonts, and resources up front.
//! - **Draw submission**: submit [`GuiDraw`] records each frame for rendering.

/// Primary GUI state owned by the renderer/user layer.
#[derive(Debug, Default)]
pub struct GuiContext {
    _private: (),
}

impl GuiContext {
    /// Initialize the GUI context.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Register GUI resources or layer configurations.
    pub fn register_layer(&mut self, _layer: GuiLayer) {}

    /// Submit a draw call to be collected for this frame.
    pub fn submit_draw(&mut self, _draw: GuiDraw) {}
}

/// Minimal draw submission payload for GUI rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiDraw {
    pub layer: GuiLayer,
    pub sort_key: u32,
}

impl GuiDraw {
    pub fn new(layer: GuiLayer, sort_key: u32) -> Self {
        Self { layer, sort_key }
    }
}

/// GUI compositing layers, ordered back-to-front by convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuiLayer {
    Background,
    World,
    Overlay,
}
