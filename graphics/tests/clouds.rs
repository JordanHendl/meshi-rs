use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use dashi::{
    AspectMask, BufferView, CommandQueueInfo2, Context, ContextInfo, Format, FRect2D, ImageInfo,
    ImageView, ImageViewType, Rect2D, SampleCount, SubresourceRange, Viewport,
};
use dashi::cmd::Executable;
use dashi::execution::CommandRing;
use dashi::QueueType;
use furikake::{BindlessState, reservations::bindless_camera::ReservedBindlessCamera};
use graphics::CloudRenderer;
use noren::rdb::imagery::{HostCubemap, ImageInfo as NorenImageInfo};

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

fn submit_compute(queue: &mut CommandRing, stream: dashi::CommandStream<Executable>) {
    queue
        .record(move |c| {
            stream.append(c).expect("record cloud compute");
        })
        .expect("record cloud compute ring");
    queue
        .submit(&Default::default())
        .expect("submit cloud compute");
    queue.wait_all().expect("wait cloud compute");
}

fn default_environment_cubemap(ctx: &mut Context) -> ImageView {
    let face = vec![135, 206, 235, 255];
    let faces = [
        face.clone(),
        face.clone(),
        face.clone(),
        face.clone(),
        face.clone(),
        face,
    ];

    let info = NorenImageInfo {
        name: "[TEST CLOUD] Environment Cubemap".to_string(),
        dim: [1, 1, 1],
        layers: 6,
        format: Format::RGBA8,
        mip_levels: 1,
    };

    let cubemap = HostCubemap::from_faces(info, faces).expect("create env cubemap");
    let mut dashi_info = cubemap.info.dashi_cube();
    dashi_info.initial_data = Some(cubemap.data());

    let image = ctx
        .make_image(&dashi_info)
        .expect("create env cubemap image");

    ImageView {
        img: image,
        aspect: AspectMask::Color,
        view_type: ImageViewType::Cube,
        range: SubresourceRange::new(0, cubemap.info.mip_levels, 0, 6),
    }
}

#[test]
fn cloud_transmittance_deterministic_and_jittered() {
    let mut ctx = Context::headless(&ContextInfo::default()).expect("create context");
    let mut state = BindlessState::new(&mut ctx);
    let mut queue = ctx
        .make_command_ring(&CommandQueueInfo2 {
            debug_name: "[TEST CLOUD COMPUTE]",
            parent: None,
            queue_type: QueueType::Compute,
        })
        .expect("create cloud compute ring");

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

    let environment_view = default_environment_cubemap(&mut ctx);
    let mut clouds_a = CloudRenderer::new(
        &mut ctx,
        &mut state,
        &viewport,
        depth_view,
        SampleCount::S1,
        environment_view,
    );
    let clouds_a_cmd = clouds_a.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    submit_compute(&mut queue, clouds_a_cmd);
    let hash_a = hash_transmittance(&mut ctx, clouds_a.transmittance_buffer());

    let environment_view_b = default_environment_cubemap(&mut ctx);
    let mut clouds_b = CloudRenderer::new(
        &mut ctx,
        &mut state,
        &viewport,
        depth_view,
        SampleCount::S1,
        environment_view_b,
    );
    let clouds_b_cmd = clouds_b.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    submit_compute(&mut queue, clouds_b_cmd);
    let hash_b = hash_transmittance(&mut ctx, clouds_b.transmittance_buffer());

    assert_eq!(hash_a, hash_b, "Cloud transmittance must be deterministic for identical inputs.");

    let clouds_b_cmd = clouds_b.update(&mut ctx, &mut state, &viewport, camera, 0.0);
    submit_compute(&mut queue, clouds_b_cmd);
    let hash_c = hash_transmittance(&mut ctx, clouds_b.transmittance_buffer());
    assert_ne!(hash_b, hash_c, "Cloud transmittance should change with frame index jitter.");
}
