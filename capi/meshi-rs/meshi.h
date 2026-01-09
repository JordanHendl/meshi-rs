#pragma once

#include <stddef.h>
#include <stdint.h>
#include "meshi_types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*MeshiEventCallback)(struct MeshiEvent*, void*);
typedef void (*MeshiAudioFinishedCallback)(MeshiAudioSourceHandle, void*);

// Engine
struct MeshiEngine* meshi_make_engine(const struct MeshiEngineInfo* info);
struct MeshiEngine* meshi_make_engine_headless(const char* application_name, const char* application_location);
void meshi_destroy_engine(struct MeshiEngine* engine);
void meshi_register_event_callback(struct MeshiEngine* engine, void* user_data, MeshiEventCallback cb);
float meshi_update(struct MeshiEngine* engine);
struct MeshiEngine* meshi_get_graphics_system(struct MeshiEngine* engine);
struct MeshiAudioEngine* meshi_get_audio_system(struct MeshiEngine* engine);

// Audio
MeshiAudioSourceHandle meshi_audio_create_source(struct MeshiAudioEngine* audio, const char* path);
void meshi_audio_destroy_source(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h);
void meshi_audio_play(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h);
void meshi_audio_pause(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h);
void meshi_audio_stop(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h);
MeshiPlaybackState meshi_audio_get_state(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h);
void meshi_audio_set_looping(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h, int32_t looping);
void meshi_audio_set_volume(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h, float volume);
void meshi_audio_set_pitch(struct MeshiAudioEngine* audio, MeshiAudioSourceHandle h, float pitch);
MeshiAudioSourceHandle meshi_audio_create_stream(struct MeshiAudioEngine* audio, const char* path);
size_t meshi_audio_update_stream(
    struct MeshiAudioEngine* audio,
    MeshiAudioSourceHandle h,
    uint8_t* out_samples,
    size_t max);
void meshi_audio_set_source_transform(
    struct MeshiAudioEngine* audio,
    MeshiAudioSourceHandle h,
    const MeshiMat4* transform,
    MeshiVec3 velocity);
void meshi_audio_set_listener_transform(
    struct MeshiAudioEngine* audio,
    const MeshiMat4* transform,
    MeshiVec3 velocity);
void meshi_audio_set_bus_volume(struct MeshiAudioEngine* audio, MeshiAudioBusHandle h, float volume);
void meshi_audio_register_finished_callback(struct MeshiAudioEngine* audio, void* user_data, MeshiAudioFinishedCallback cb);

// Graphics
MeshiMeshObjectHandle meshi_gfx_create_mesh_object(struct MeshiEngine* render, const MeshiFFIMeshObjectInfo* info);
void meshi_gfx_release_render_object(struct MeshiEngine* render, const MeshiMeshObjectHandle* h);
void meshi_gfx_set_transform(struct MeshiEngine* render, MeshiMeshObjectHandle h, const MeshiMat4* transform);
MeshiDirectionalLightHandle meshi_gfx_create_light(struct MeshiEngine* render, const MeshiDirectionalLightInfo* info);
void meshi_gfx_release_light(struct MeshiEngine* render, const MeshiDirectionalLightHandle* h);
void meshi_gfx_set_light_transform(struct MeshiEngine* render, MeshiDirectionalLightHandle h, const MeshiMat4* transform);
void meshi_gfx_set_light_info(struct MeshiEngine* render, MeshiDirectionalLightHandle h, const MeshiDirectionalLightInfo* info);
void meshi_gfx_set_camera_transform(struct MeshiEngine* render, const MeshiMat4* transform);
MeshiCameraHandle meshi_gfx_register_camera(struct MeshiEngine* render, const MeshiMat4* initial_transform);
void meshi_gfx_set_camera_projection(struct MeshiEngine* render, const MeshiMat4* transform);
void meshi_gfx_capture_mouse(struct MeshiEngine* render, int32_t value);


// Physics
struct MeshiPhysicsSimulation* meshi_get_physics_system(struct MeshiEngine* engine);
void meshi_physx_set_gravity(struct MeshiPhysicsSimulation* physics, float gravity_mps);
MeshiMaterialHandle meshi_physx_create_material(struct MeshiPhysicsSimulation* physics, const MeshiMaterialInfo* info);
void meshi_physx_release_material(struct MeshiPhysicsSimulation* physics, const MeshiMaterialHandle* h);
MeshiRigidBodyHandle meshi_physx_create_rigid_body(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyInfo* info);
void meshi_physx_release_rigid_body(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h);
void meshi_physx_apply_force_to_rigid_body(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h, const MeshiForceApplyInfo* info);
int32_t meshi_physx_set_rigid_body_transform(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h, const MeshiActorStatus* info);
int32_t meshi_physx_get_rigid_body_status(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h, MeshiActorStatus* out_status);
// Returns the current velocity of a rigid body or a zero vector on failure.
MeshiVec3 meshi_physx_get_rigid_body_velocity(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h);
int32_t meshi_physx_set_collision_shape(struct MeshiPhysicsSimulation* physics, const MeshiRigidBodyHandle* h, const MeshiCollisionShape* shape);
size_t meshi_physx_get_contacts(struct MeshiPhysicsSimulation* physics, MeshiContactInfo* out_contacts, size_t max);
MeshiCollisionShape meshi_physx_collision_shape_sphere(float radius);
MeshiCollisionShape meshi_physx_collision_shape_box(MeshiVec3 dimensions);
MeshiCollisionShape meshi_physx_collision_shape_capsule(float half_height, float radius);

#ifdef __cplusplus
} // extern "C"
#endif
