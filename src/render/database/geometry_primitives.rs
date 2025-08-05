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

pub fn make_cube(
    info: &CubePrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
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

    let vertices = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cube Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * cvertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { cvertices.as_slice().align_to::<u8>().1 }),
    })?;

    let indices = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cube Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * INDICES.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { INDICES.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Cube Mesh..");
    Ok(MeshResource {
        name: "CUBE".to_string(),
        vertices,
        num_vertices: cvertices.len(),
        indices,
        num_indices: INDICES.len(),
    })
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

pub fn make_triangle(
    info: &TrianglePrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
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

    let vertices = ctx.make_buffer(&BufferInfo {
        debug_name: &"Triangle Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * tvertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { tvertices.as_slice().align_to::<u8>().1 }),
    })?;

    let indices = ctx.make_buffer(&BufferInfo {
        debug_name: &"Triangle Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * INDICES.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { INDICES.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Triangle Mesh..");
    Ok(MeshResource {
        name: "TRIANGLE".to_string(),
        vertices,
        num_vertices: tvertices.len(),
        indices,
        num_indices: INDICES.len(),
    })
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

pub fn make_sphere(
    info: &SpherePrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
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

    let vertex_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Sphere Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { vertices.as_slice().align_to::<u8>().1 }),
    })?;

    let index_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Sphere Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * indices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { indices.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Sphere Mesh..");
    Ok(MeshResource {
        name: "SPHERE".to_string(),
        vertices: vertex_buffer,
        num_vertices: vertices.len(),
        indices: index_buffer,
        num_indices: indices.len(),
    })
}

#[repr(C)]
pub struct CylinderPrimitiveInfo {
    pub radius: f32,
    pub height: f32,
    pub segments: u32,
}

impl Default for CylinderPrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            height: 1.0,
            segments: 32,
        }
    }
}

pub fn make_cylinder(
    info: &CylinderPrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
    let CylinderPrimitiveInfo {
        radius,
        height,
        segments,
    } = *info;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Side vertices
    for i in 0..=segments {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        let u = i as f32 / segments as f32;
        vertices.push(Vertex {
            position: Vec4::new(x, height * 0.5, z, 1.0),
            normal: Vec4::new(x / radius, 0.0, z / radius, 0.0),
            tex_coords: Vec2::new(u, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
        vertices.push(Vertex {
            position: Vec4::new(x, -height * 0.5, z, 1.0),
            normal: Vec4::new(x / radius, 0.0, z / radius, 0.0),
            tex_coords: Vec2::new(u, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
    }

    for i in 0..segments {
        let top1 = 2 * i as u32;
        let bottom1 = top1 + 1;
        let top2 = 2 * (i + 1) as u32;
        let bottom2 = top2 + 1;

        indices.push(top1);
        indices.push(bottom1);
        indices.push(top2);

        indices.push(top2);
        indices.push(bottom1);
        indices.push(bottom2);
    }

    let top_start = vertices.len();
    for i in 0..segments {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        vertices.push(Vertex {
            position: Vec4::new(x, height * 0.5, z, 1.0),
            normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
            tex_coords: Vec2::ZERO,
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
    }
    let top_center = vertices.len() as u32;
    vertices.push(Vertex {
        position: Vec4::new(0.0, height * 0.5, 0.0, 1.0),
        normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
        tex_coords: Vec2::ZERO,
        joint_ids: IVec4::ZERO,
        joints: Vec4::ZERO,
        color: Vec4::ZERO,
    });
    for i in 0..segments {
        let current = top_start as u32 + i;
        let next = top_start as u32 + ((i + 1) % segments);
        indices.push(top_center);
        indices.push(current);
        indices.push(next);
    }

    let bottom_start = vertices.len();
    for i in 0..segments {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        vertices.push(Vertex {
            position: Vec4::new(x, -height * 0.5, z, 1.0),
            normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
            tex_coords: Vec2::ZERO,
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
    }
    let bottom_center = vertices.len() as u32;
    vertices.push(Vertex {
        position: Vec4::new(0.0, -height * 0.5, 0.0, 1.0),
        normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
        tex_coords: Vec2::ZERO,
        joint_ids: IVec4::ZERO,
        joints: Vec4::ZERO,
        color: Vec4::ZERO,
    });
    for i in 0..segments {
        let current = bottom_start as u32 + i;
        let next = bottom_start as u32 + ((i + 1) % segments);
        indices.push(bottom_center);
        indices.push(next);
        indices.push(current);
    }

    let vertex_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cylinder Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { vertices.as_slice().align_to::<u8>().1 }),
    })?;

    let index_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cylinder Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * indices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { indices.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Cylinder Mesh..");
    Ok(MeshResource {
        name: "CYLINDER".to_string(),
        vertices: vertex_buffer,
        num_vertices: vertices.len(),
        indices: index_buffer,
        num_indices: indices.len(),
    })
}

#[repr(C)]
pub struct PlanePrimitiveInfo {
    pub size: f32,
}

impl Default for PlanePrimitiveInfo {
    fn default() -> Self {
        Self { size: 1.0 }
    }
}

pub fn make_plane(
    info: &PlanePrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
    let size = info.size;

    let vertices: [Vertex; 4] = [
        Vertex {
            position: Vec4::new(-size, 0.0, -size, 1.0),
            normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
            tex_coords: Vec2::new(0.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, 0.0, -size, 1.0),
            normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
            tex_coords: Vec2::new(1.0, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(size, 0.0, size, 1.0),
            normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
            tex_coords: Vec2::new(1.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
        Vertex {
            position: Vec4::new(-size, 0.0, size, 1.0),
            normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
            tex_coords: Vec2::new(0.0, 1.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        },
    ];

    const INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

    let vertex_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Plane Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { vertices.as_slice().align_to::<u8>().1 }),
    })?;

    let index_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Plane Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * INDICES.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { INDICES.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Plane Mesh..");
    Ok(MeshResource {
        name: "PLANE".to_string(),
        vertices: vertex_buffer,
        num_vertices: vertices.len(),
        indices: index_buffer,
        num_indices: INDICES.len(),
    })
}

#[repr(C)]
pub struct ConePrimitiveInfo {
    pub radius: f32,
    pub height: f32,
    pub segments: u32,
}

impl Default for ConePrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            height: 1.0,
            segments: 32,
        }
    }
}

pub fn make_cone(
    info: &ConePrimitiveInfo,
    ctx: &mut dashi::Context,
) -> Result<MeshResource, dashi::GPUError> {
    let ConePrimitiveInfo {
        radius,
        height,
        segments,
    } = *info;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..segments {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        let normal = Vec4::new(x, radius / height, z, 0.0).normalize();
        vertices.push(Vertex {
            position: Vec4::new(x, -height * 0.5, z, 1.0),
            normal,
            tex_coords: Vec2::new(i as f32 / segments as f32, 0.0),
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
    }
    let apex_index = vertices.len() as u32;
    vertices.push(Vertex {
        position: Vec4::new(0.0, height * 0.5, 0.0, 1.0),
        normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
        tex_coords: Vec2::new(0.5, 1.0),
        joint_ids: IVec4::ZERO,
        joints: Vec4::ZERO,
        color: Vec4::ZERO,
    });

    for i in 0..segments {
        let current = i;
        let next = (i + 1) % segments;
        indices.push(current);
        indices.push(next);
        indices.push(apex_index);
    }

    let base_start = vertices.len();
    for i in 0..segments {
        let theta = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        vertices.push(Vertex {
            position: Vec4::new(x, -height * 0.5, z, 1.0),
            normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
            tex_coords: Vec2::ZERO,
            joint_ids: IVec4::ZERO,
            joints: Vec4::ZERO,
            color: Vec4::ZERO,
        });
    }
    let base_center = vertices.len() as u32;
    vertices.push(Vertex {
        position: Vec4::new(0.0, -height * 0.5, 0.0, 1.0),
        normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
        tex_coords: Vec2::ZERO,
        joint_ids: IVec4::ZERO,
        joints: Vec4::ZERO,
        color: Vec4::ZERO,
    });
    for i in 0..segments {
        let current = base_start as u32 + i;
        let next = base_start as u32 + ((i + 1) % segments);
        indices.push(base_center);
        indices.push(next);
        indices.push(current);
    }

    let vertex_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cone Vertices".to_string(),
        byte_size: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::VERTEX,
        initial_data: Some(unsafe { vertices.as_slice().align_to::<u8>().1 }),
    })?;

    let index_buffer = ctx.make_buffer(&BufferInfo {
        debug_name: &"Cone Indices".to_string(),
        byte_size: (std::mem::size_of::<u32>() * indices.len()) as u32,
        visibility: MemoryVisibility::Gpu,
        usage: BufferUsage::INDEX,
        initial_data: Some(unsafe { indices.as_slice().align_to::<u8>().1 }),
    })?;

    info!("Registering Default Cone Mesh..");
    Ok(MeshResource {
        name: "CONE".to_string(),
        vertices: vertex_buffer,
        num_vertices: vertices.len(),
        indices: index_buffer,
        num_indices: indices.len(),
    })
}
