use dashi::{Handle, SampleCount};
use glam::{Mat4, Vec3};
use meshi_graphics::{DisplayInfo, RenderEngine, RenderEngineInfo, RendererSelect, WindowInfo};

pub struct RuntimeFrame {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
}

#[derive(Default)]
pub struct RuntimeControlState {
    pub playing: bool,
    step_requested: bool,
}

impl RuntimeControlState {
    pub fn request_step(&mut self) {
        self.step_requested = true;
    }

    pub fn consume_step(&mut self) -> bool {
        if self.step_requested {
            self.step_requested = false;
            true
        } else {
            false
        }
    }
}

pub struct RuntimeBridge {
    engine: Option<RenderEngine>,
    display: Option<Handle<meshi_graphics::Display>>,
    camera: Option<Handle<meshi_graphics::Camera>>,
    viewport_size: [u32; 2],
    last_frame: Option<RuntimeFrame>,
}

impl RuntimeBridge {
    pub fn new() -> Self {
        Self {
            engine: None,
            display: None,
            camera: None,
            viewport_size: [0, 0],
            last_frame: None,
        }
    }

    pub fn latest_frame(&self) -> Option<&RuntimeFrame> {
        self.last_frame.as_ref()
    }

    pub fn tick(
        &mut self,
        delta_time: f32,
        controls: &mut RuntimeControlState,
        viewport_pixels: [u32; 2],
    ) -> bool {
        let viewport_pixels = [viewport_pixels[0].max(1), viewport_pixels[1].max(1)];
        let mut size_changed = false;
        if self.engine.is_none() || self.viewport_size != viewport_pixels {
            size_changed = true;
            self.recreate_engine(viewport_pixels);
        }

        let should_step = controls.consume_step();
        let should_render = controls.playing || should_step || size_changed;
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };

        if should_render {
            let frame_delta = if controls.playing || should_step {
                delta_time.max(1.0 / 240.0)
            } else {
                0.0
            };
            engine.update(frame_delta);

            let Some(display) = self.display else {
                return false;
            };
            if let Some(frame) = engine.frame_dump(display) {
                let pixel_len = (frame.width as usize)
                    .saturating_mul(frame.height as usize)
                    .saturating_mul(4);
                let src = unsafe { std::slice::from_raw_parts(frame.pixels, pixel_len) };
                let mut pixels = Vec::with_capacity(pixel_len);
                for chunk in src.chunks_exact(4) {
                    pixels.push(chunk[2]);
                    pixels.push(chunk[1]);
                    pixels.push(chunk[0]);
                    pixels.push(chunk[3]);
                }
                self.last_frame = Some(RuntimeFrame {
                    size: [frame.width as usize, frame.height as usize],
                    pixels,
                });
            }
        }
        should_render
    }

    fn recreate_engine(&mut self, viewport_pixels: [u32; 2]) {
        let info = RenderEngineInfo {
            headless: true,
            canvas_extent: Some(viewport_pixels),
            renderer: RendererSelect::Deferred,
            sample_count: Some(SampleCount::S1),
            skybox_cubemap_entry: None,
            debug_mode: false,
            shadow_cascades: Default::default(),
        };
        let mut engine = RenderEngine::new(&info).expect("Failed to create RenderEngine");
        let mut display_info = DisplayInfo::default();
        display_info.window = WindowInfo {
            title: "Meshi Editor Viewport".to_string(),
            size: viewport_pixels,
            resizable: false,
        };
        let display = engine.register_cpu_display(display_info);

        let camera = engine.register_camera(&Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0)));
        engine.set_camera_perspective(
            camera,
            60f32.to_radians(),
            viewport_pixels[0] as f32,
            viewport_pixels[1] as f32,
            0.1,
            2000.0,
        );
        engine.attach_camera_to_display(display, camera);

        self.engine = Some(engine);
        self.display = Some(display);
        self.camera = Some(camera);
        self.viewport_size = viewport_pixels;
    }
}
