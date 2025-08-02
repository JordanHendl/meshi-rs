use crate::render::database::MeshResource;
use dashi::*;
use glam::{IVec4, Vec2, Vec4};
use tracing::info;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Vertex {
    position: Vec4,
    normal: Vec4,
    tex_coords: Vec2,
    joint_ids: IVec4,
    joints: Vec4,
    color: Vec4,
}

#[repr(C)]
pub struct CubePrimitiveInfo {
    pub size: f32,
}

impl Default for CubePrimitiveInfo {
    fn default() -> Self {
        Self { size: 1.0 }
    }
}

pub fn make_cube(info: &CubePrimitiveInfo, ctx: &mut dashi::Context) -> MeshResource {
    let size = info.size;

    let cvertices: [Vertex; 8] = [
        // Front face
        Vertex {
            position: Vec4::new(-size, -size, size, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(0.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, -size, size, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(1.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, size, size, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(1.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(-size, size, size, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(0.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        // Back face
        Vertex {
            position: Vec4::new(-size, -size, -size, 1.0),
            normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
            tex_coords: Vec2::new(1.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, -size, -size, 1.0),
            normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
            tex_coords: Vec2::new(0.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, size, -size, 1.0),
            normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
            tex_coords: Vec2::new(0.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(-size, size, -size, 1.0),
            normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
            tex_coords: Vec2::new(1.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
    ];

    const INDICES: [u32; 36] = [
        // Front face
        0, 1, 2, 2, 3, 0, // Back face
        4, 5, 6, 6, 7, 4, // Left face
        4, 0, 3, 3, 7, 4, // Right face
        1, 5, 6, 6, 2, 1, // Top face
        3, 2, 6, 6, 7, 3, // Bottom face
        4, 5, 1, 1, 0, 4,
    ];

    let vertices = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Cube Vertices".to_string(),
            byte_size: (std::mem::size_of::<Vertex>() * cvertices.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::VERTEX,
            initial_data: Some(unsafe { cvertices.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    let indices = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Cube Indices".to_string(),
            byte_size: (std::mem::size_of::<u32>() * INDICES.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::INDEX,
            initial_data: Some(unsafe { INDICES.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    info!("Registering Default Cube Mesh..");
    MeshResource {
        name: "CUBE".to_string(),
        vertices,
        num_vertices: cvertices.len(),
        indices,
        num_indices: INDICES.len(),
    }
}

#[repr(C)]
pub struct TrianglePrimitiveInfo {
    pub size: f32,
}

impl Default for TrianglePrimitiveInfo {
    fn default() -> Self {
        Self { size: 1.0 }
    }
}

pub fn make_triangle(info: &TrianglePrimitiveInfo, ctx: &mut dashi::Context) -> MeshResource {
    let size = info.size;
    let tvertices: [Vertex; 3] = [
        Vertex {
            position: Vec4::new(0.0, size, 0.0, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(0.5, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(-size, -size, 0.0, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(0.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, -size, 0.0, 1.0),
            normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
            tex_coords: Vec2::new(1.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
    ];

    const INDICES: [u32; 3] = [0, 1, 2];

    let vertices = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Triangle Vertices".to_string(),
            byte_size: (std::mem::size_of::<Vertex>() * tvertices.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::VERTEX,
            initial_data: Some(unsafe { tvertices.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    let indices = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Triangle Indices".to_string(),
            byte_size: (std::mem::size_of::<u32>() * INDICES.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::INDEX,
            initial_data: Some(unsafe { INDICES.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    info!("Registering Default Triangle Mesh..");
    MeshResource {
        name: "TRIANGLE".to_string(),
        vertices,
        num_vertices: tvertices.len(),
        indices,
        num_indices: INDICES.len(),
    }
}

#[repr(C)]
pub struct SpherePrimitiveInfo {
    pub radius: f32,
    pub segments: u32,
    pub rings: u32,
}

impl Default for SpherePrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            segments: 32,
            rings: 16,
        }
    }
}

pub fn make_sphere(info: &SpherePrimitiveInfo, ctx: &mut dashi::Context) -> MeshResource {
    let SpherePrimitiveInfo {
        radius,
        segments,
        rings,
    } = *info;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for ring in 0..=rings {
        let theta = (ring as f32) * std::f32::consts::PI / (rings as f32);
        let y = radius * theta.cos();
        let ring_radius = radius * theta.sin();

        for segment in 0..=segments {
            let phi = (segment as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
            let x = ring_radius * phi.cos();
            let z = ring_radius * phi.sin();

            vertices.push(Vertex {
                position: Vec4::new(x, y, z, 1.0),
                normal: Vec4::new(x / radius, y / radius, z / radius, 0.0),
                tex_coords: Vec2::new(segment as f32 / segments as f32, ring as f32 / rings as f32),
                joint_ids: IVec4::ZERO,
                joints: Vec4::ZERO,
                color: Vec4::ZERO,
            });

            if ring < rings && segment < segments {
                let current = ring * (segments + 1) + segment;
                let next = current + segments + 1;

                indices.push(current);
                indices.push(next);
                indices.push(current + 1);

                indices.push(current + 1);
                indices.push(next);
                indices.push(next + 1);
            }
        }
    }

    let vertex_buffer = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Sphere Vertices".to_string(),
            byte_size: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::VERTEX,
            initial_data: Some(unsafe { vertices.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    let index_buffer = ctx
        .make_buffer(&BufferInfo {
            debug_name: &"Sphere Indices".to_string(),
            byte_size: (std::mem::size_of::<u32>() * indices.len()) as u32,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::INDEX,
            initial_data: Some(unsafe { indices.as_slice().align_to::<u8>().1 }),
        })
        .unwrap();

    info!("Registering Default Sphere Mesh..");
    MeshResource {
        name: "SPHERE".to_string(),
        vertices: vertex_buffer,
        num_vertices: vertices.len(),
        indices: index_buffer,
        num_indices: indices.len(),
    }
}
