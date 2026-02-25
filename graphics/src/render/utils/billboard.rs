use bento::builder::{AttachmentDesc, PSOBuilder, PSO};
use dashi::*;
use furikake::reservations::bindless_camera::ReservedBindlessCamera;
use furikake::reservations::bindless_materials::ReservedBindlessMaterials;
use furikake::types::{Camera, Material};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::{BillboardInfo, BillboardType};

#[derive(Clone)]
pub(crate) struct BillboardData {
    pub info: BillboardInfo,
    pub vertex_buffer: Handle<Buffer>,
    pub owns_material: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BillboardVertex {
    center: [f32; 3],
    offset: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    tex_coords: [f32; 2],
}

pub(crate) fn build_billboard_pipeline(
    ctx: &mut Context,
    state: &mut BindlessState,
    sample_count: SampleCount,
    per_obj_resource: ShaderResource,
) -> PSO {
    let shaders = miso::stdbillboard(&[]);

    let mut pso_builder = PSOBuilder::new()
        .set_debug_name("[MESHI] Deferred Billboard")
        .vertex_compiled(Some(shaders[0].clone()))
        .fragment_compiled(Some(shaders[1].clone()))
        .set_attachment_format(0, Format::BGRA8)
        .add_table_variable_with_resources(
            "per_obj_ssbo",
            vec![IndexedResource {
                resource: per_obj_resource,
                slot: 0,
            }],
        );

    pso_builder = pso_builder
        .add_reserved_table_variables(state)
        .expect("Failed to add reserved tables for billboard pipeline");

    pso_builder = pso_builder.add_depth_target(AttachmentDesc {
        format: Format::D24S8,
        samples: sample_count,
    });

    let pso = pso_builder
        .set_details(GraphicsPipelineDetails {
            color_blend_states: vec![Default::default(); 1],
            sample_count,
            depth_test: Some(DepthInfo {
                should_test: true,
                should_write: false,
                ..Default::default()
            }),
            ..Default::default()
        })
        .build(ctx)
        .expect("Failed to build billboard pipeline!");

    state.register_pso_tables(&pso);

    pso
}

pub(crate) fn allocate_billboard_material(
    state: &mut BindlessState,
    texture_id: u32,
) -> Handle<Material> {
    let mut material_handle = Handle::default();
    state
        .reserved_mut::<ReservedBindlessMaterials, _>("meshi_bindless_materials", |materials| {
            material_handle = materials.add_material();
            let material = materials.material_mut(material_handle);
            *material = Material::default();
            material.base_color_texture_id = texture_id;
            material.normal_texture_id = u32::MAX;
            material.metallic_roughness_texture_id = u32::MAX;
            material.occlusion_texture_id = u32::MAX;
            material.emissive_texture_id = u32::MAX;
        })
        .expect("Failed to allocate billboard material");

    material_handle
}

pub(crate) fn update_billboard_material_texture(
    state: &mut BindlessState,
    material: Handle<Material>,
    texture_id: u32,
) {
    state
        .reserved_mut::<ReservedBindlessMaterials, _>("meshi_bindless_materials", |materials| {
            let material = materials.material_mut(material);
            material.base_color_texture_id = texture_id;
        })
        .expect("Failed to update billboard material texture");
}

pub(crate) fn create_billboard_data(
    ctx: &mut Context,
    state: &mut BindlessState,
    mut info: BillboardInfo,
) -> BillboardData {
    let vertices = billboard_vertices(Vec3::ZERO, Vec2::ONE, Vec4::ONE);
    let vertex_buffer = ctx
        .make_buffer(&BufferInfo {
            debug_name: "[MESHI] Billboard Vertex Buffer",
            byte_size: (std::mem::size_of::<BillboardVertex>() * vertices.len()) as u32,
            visibility: MemoryVisibility::CpuAndGpu,
            usage: BufferUsage::VERTEX,
            initial_data: Some(unsafe { vertices.align_to::<u8>().1 }),
        })
        .expect("Failed to create billboard vertex buffer");

    let mut owns_material = false;
    if info.material.is_none() {
        info.material = Some(allocate_billboard_material(state, info.texture_id));
        owns_material = true;
    }

    BillboardData {
        info,
        vertex_buffer,
        owns_material,
    }
}

fn billboard_vertices(center: Vec3, size: Vec2, color: Vec4) -> [BillboardVertex; 6] {
    let offsets = [
        Vec2::new(-0.5, -0.5),
        Vec2::new(0.5, -0.5),
        Vec2::new(0.5, 0.5),
        Vec2::new(-0.5, 0.5),
    ];
    let tex_coords = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    let color = color.to_array();
    let center = center.to_array();
    let size = size.to_array();

    [
        BillboardVertex {
            center,
            offset: offsets[0].to_array(),
            size,
            color,
            tex_coords: tex_coords[0].to_array(),
        },
        BillboardVertex {
            center,
            offset: offsets[1].to_array(),
            size,
            color,
            tex_coords: tex_coords[1].to_array(),
        },
        BillboardVertex {
            center,
            offset: offsets[2].to_array(),
            size,
            color,
            tex_coords: tex_coords[2].to_array(),
        },
        BillboardVertex {
            center,
            offset: offsets[2].to_array(),
            size,
            color,
            tex_coords: tex_coords[2].to_array(),
        },
        BillboardVertex {
            center,
            offset: offsets[3].to_array(),
            size,
            color,
            tex_coords: tex_coords[3].to_array(),
        },
        BillboardVertex {
            center,
            offset: offsets[0].to_array(),
            size,
            color,
            tex_coords: tex_coords[0].to_array(),
        },
    ]
}

fn billboard_vertices_world(corners: [Vec3; 4], color: Vec4) -> [BillboardVertex; 6] {
    let tex_coords = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    let color = color.to_array();
    let size = Vec2::ZERO.to_array();
    let offset = Vec2::ZERO.to_array();

    [
        BillboardVertex {
            center: corners[0].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[0].to_array(),
        },
        BillboardVertex {
            center: corners[1].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[1].to_array(),
        },
        BillboardVertex {
            center: corners[2].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[2].to_array(),
        },
        BillboardVertex {
            center: corners[2].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[2].to_array(),
        },
        BillboardVertex {
            center: corners[3].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[3].to_array(),
        },
        BillboardVertex {
            center: corners[0].to_array(),
            offset,
            size,
            color,
            tex_coords: tex_coords[0].to_array(),
        },
    ]
}

pub(crate) fn update_billboard_vertices(
    ctx: &mut Context,
    state: &mut BindlessState,
    billboard: &BillboardData,
    transform: Mat4,
    camera: Handle<Camera>,
) {
    let center = transform.transform_point3(Vec3::ZERO);
    let mut size = Vec2::new(
        transform.transform_vector3(Vec3::X).length(),
        transform.transform_vector3(Vec3::Y).length(),
    );

    if size.x <= 0.0 {
        size.x = 1.0;
    }
    if size.y <= 0.0 {
        size.y = 1.0;
    }

    let vertices = match billboard.info.billboard_type {
        BillboardType::ScreenAligned => billboard_vertices(center, size, Vec4::ONE),
        BillboardType::AxisAligned => {
            let mut camera_position = Vec3::ZERO;
            if camera.valid() {
                state
                    .reserved_mut(
                        "meshi_bindless_cameras",
                        |a: &mut ReservedBindlessCamera| {
                            camera_position = a.camera(camera).position();
                        },
                    )
                    .expect("Failed to read camera for billboard alignment");
            }

            let mut forward = camera_position - center;
            forward.y = 0.0;
            if forward.length_squared() <= 1e-6 {
                forward = Vec3::Z;
            } else {
                forward = forward.normalize();
            }

            let mut right = forward.cross(Vec3::Y);
            if right.length_squared() <= 1e-6 {
                right = Vec3::X;
            } else {
                right = right.normalize();
            }

            let up = Vec3::Y;
            let half_right = right * (size.x * 0.5);
            let half_up = up * (size.y * 0.5);
            let corners = [
                center - half_right - half_up,
                center + half_right - half_up,
                center + half_right + half_up,
                center - half_right + half_up,
            ];
            billboard_vertices_world(corners, Vec4::ONE)
        }
        BillboardType::Fixed => {
            let right_axis = transform.transform_vector3(Vec3::X);
            let up_axis = transform.transform_vector3(Vec3::Y);

            let right = if right_axis.length_squared() <= 1e-6 {
                Vec3::X
            } else {
                right_axis.normalize()
            };
            let up = if up_axis.length_squared() <= 1e-6 {
                Vec3::Y
            } else {
                up_axis.normalize()
            };

            let half_right = right * (size.x * 0.5);
            let half_up = up * (size.y * 0.5);
            let corners = [
                center - half_right - half_up,
                center + half_right - half_up,
                center + half_right + half_up,
                center - half_right + half_up,
            ];
            billboard_vertices_world(corners, Vec4::ONE)
        }
    };

    let mapped = ctx
        .map_buffer_mut::<BillboardVertex>(BufferView::new(billboard.vertex_buffer))
        .expect("Failed to map billboard vertex buffer");
    mapped[..vertices.len()].copy_from_slice(&vertices);
    ctx.unmap_buffer(billboard.vertex_buffer)
        .expect("Failed to unmap billboard vertex buffer");
}
