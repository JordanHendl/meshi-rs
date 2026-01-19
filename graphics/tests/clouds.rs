use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use dashi::{
    AspectMask, BufferView, Context, ContextInfo, Format, FRect2D, ImageInfo, ImageView,
    ImageViewType, Rect2D, SampleCount, SubresourceRange, Viewport,
};
use furikake::{BindlessState, reservations::bindless_camera::ReservedBindlessCamera};
use graphics::CloudRenderer;

fn hash_transmittance(ctx: &mut Context, buffer: dashi::Handle<dashi::Buffer>) -> u64 {
    let view = BufferView::new(buffer);
    let data = ctx.map_buffer::<f32>(view).expect("map transmittance");
    let mut hasher = DefaultHasher::new();
    for value in data.iter().take(1024) {
        let quantized = (value * 65535.0).round() as u32;
        quantized.hash(&mut hasher);
    }
    ctx.unmap_buffer(buffer).expect("unmap transmittance");
    hasher.finish()
}

#[test]
fn cloud_transmittance_deterministic_and_jittered() {
    let mut ctx = Context::headless(&ContextInfo::default()).expect("create context");
    let mut state = BindlessState::new(&mut ctx);

    let viewport = Viewport {
        area: FRect2D {
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 360.0,
        },
        scissor: Rect2D { x: 0, y: 0, w: 640, h: 360 },
        ..Default::default()
    };

    let depth_image = ctx
        .make_image(&ImageInfo {
            debug_name: "[TEST] Depth",
            dim: [640, 360, 1],
            layers: 1,
            format: Format::D24S8,
            mip_levels: 1,
            samples: SampleCount::S1,
            initial_data: None,
            ..Default::default()
        })
        .expect("create depth image");
    let depth_view = ImageView {
        img: depth_image,
        aspect: AspectMask::Depth,
        view_type: ImageViewType::Type2D,
        range: SubresourceRange::new(0, 1, 0, 1),
    };

    let mut camera = dashi::Handle::default();
    state
        .reserved_mut("meshi_bindless_cameras", |cameras: &mut ReservedBindlessCamera| {
            camera = cameras.add_camera();
            let cam = cameras.camera_mut(camera);
            cam.set_perspective(std::f32::consts::FRAC_PI_3, 640.0, 360.0, 0.1, 10000.0);
            cam.set_position(glam::Vec3::new(0.0, 1500.0, 0.0));
        })
        .expect("create camera");

    let mut clouds_a = CloudRenderer::new(&mut ctx, &mut state, &viewport, depth_view, SampleCount::S1);
    clouds_a.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    let hash_a = hash_transmittance(&mut ctx, clouds_a.transmittance_buffer());

    let mut clouds_b = CloudRenderer::new(&mut ctx, &mut state, &viewport, depth_view, SampleCount::S1);
    clouds_b.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    let hash_b = hash_transmittance(&mut ctx, clouds_b.transmittance_buffer());

    assert_eq!(hash_a, hash_b, "Cloud transmittance must be deterministic for identical inputs.");

    clouds_b.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    let hash_c = hash_transmittance(&mut ctx, clouds_b.transmittance_buffer());
    assert_ne!(hash_b, hash_c, "Cloud transmittance should change with frame index jitter.");
}
