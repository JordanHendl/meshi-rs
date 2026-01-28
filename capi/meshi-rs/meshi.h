#pragma once

#include <stddef.h>
#include <stdint.h>
#include "meshi_types.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*MeshiEventCallback)(struct MeshiEvent*, void*);
typedef void (*MeshiAudioFinishedCallback)(MeshiAudioSourceHandle, void*);
typedef const struct MeshiPluginApi* (*MeshiPluginGetApiFn)(void);
typedef void* (*MeshiSymbolLoader)(const char* name);

#define MESHI_PLUGIN_GET_API_SYMBOL "meshi_plugin_get_api"
#define MESHI_PLUGIN_LOAD_API(loader_fn) \
    ((loader_fn) ? ((MeshiPluginGetApiFn)(loader_fn(MESHI_PLUGIN_GET_API_SYMBOL)))() : NULL)

typedef struct MeshiPluginApi {
    uint32_t abi_version;
    struct MeshiEngine* (*make_engine)(const struct MeshiEngineInfo* info);
    struct MeshiEngine* (*make_engine_headless)(const char* application_name, const char* application_location);
    void (*destroy_engine)(struct MeshiEngine* engine);
    void (*register_event_callback)(struct MeshiEngine* engine, void* user_data, MeshiEventCallback cb);
    float (*update)(struct MeshiEngine* engine);
    struct MeshiEngine* (*get_graphics_system)(struct MeshiEngine* engine);
    struct MeshiEngine* (*get_audio_system)(struct MeshiEngine* engine);
    struct MeshiEngine* (*get_physics_system)(struct MeshiEngine* engine);
    MeshiRenderObjectHandle (*gfx_create_mesh_object)(struct MeshiEngine* render, const MeshiMeshObjectInfo* info);
    MeshiRenderObjectHandle (*gfx_create_render_object)(struct MeshiEngine* render, const MeshiRenderObjectInfo* info);
    void (*gfx_release_render_object)(struct MeshiEngine* render, const MeshiRenderObjectHandle* h);
    void (*gfx_set_transform)(struct MeshiEngine* render, MeshiRenderObjectHandle h, const MeshiMat4* transform);
    MeshiLightHandle (*gfx_create_light)(struct MeshiEngine* render, const MeshiLightInfo* info);
    void (*gfx_release_light)(struct MeshiEngine* render, const MeshiLightHandle* h);
    void (*gfx_set_light_transform)(struct MeshiEngine* render, MeshiLightHandle h, const MeshiMat4* transform);
    void (*gfx_set_light_info)(struct MeshiEngine* render, MeshiLightHandle h, const MeshiLightInfo* info);
    void (*gfx_set_camera_transform)(struct MeshiEngine* render, const MeshiMat4* transform);
    MeshiCameraHandle (*gfx_register_camera)(struct MeshiEngine* render, const MeshiMat4* initial_transform);
    void (*gfx_set_camera_projection)(struct MeshiEngine* render, const MeshiMat4* transform);
    void (*gfx_capture_mouse)(struct MeshiEngine* render, int32_t value);
    MeshiAudioSourceHandle (*audio_create_source)(struct MeshiEngine* engine, const char* path);
    void (*audio_destroy_source)(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
    void (*audio_play)(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
    void (*audio_pause)(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
    void (*audio_stop)(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
    MeshiPlaybackState (*audio_get_state)(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
    void (*audio_set_looping)(struct MeshiEngine* engine, MeshiAudioSourceHandle h, int32_t looping);
    void (*audio_set_volume)(struct MeshiEngine* engine, MeshiAudioSourceHandle h, float volume);
    void (*audio_set_pitch)(struct MeshiEngine* engine, MeshiAudioSourceHandle h, float pitch);
    MeshiAudioSourceHandle (*audio_create_stream)(struct MeshiEngine* engine, const char* path);
    size_t (*audio_update_stream)(
        struct MeshiEngine* engine,
        MeshiAudioSourceHandle h,
        uint8_t* out_samples,
        size_t max);
    void (*audio_set_source_transform)(
        struct MeshiEngine* engine,
        MeshiAudioSourceHandle h,
        const MeshiMat4* transform,
        MeshiVec3 velocity);
    void (*audio_set_listener_transform)(
        struct MeshiEngine* engine,
        const MeshiMat4* transform,
        MeshiVec3 velocity);
    void (*audio_set_bus_volume)(struct MeshiEngine* engine, MeshiAudioBusHandle h, float volume);
    void (*audio_register_finished_callback)(struct MeshiEngine* engine, void* user_data, MeshiAudioFinishedCallback cb);
    void (*physx_set_gravity)(struct MeshiEngine* engine, float gravity_mps);
    MeshiMaterialHandle (*physx_create_material)(struct MeshiEngine* engine, const MeshiMaterialInfo* info);
    void (*physx_release_material)(struct MeshiEngine* engine, const MeshiMaterialHandle* h);
    MeshiRigidBodyHandle (*physx_create_rigid_body)(struct MeshiEngine* engine, const MeshiRigidBodyInfo* info);
    void (*physx_release_rigid_body)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h);
    void (*physx_apply_force_to_rigid_body)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiForceApplyInfo* info);
    int32_t (*physx_set_rigid_body_transform)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiActorStatus* info);
    int32_t (*physx_get_rigid_body_status)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, MeshiActorStatus* out_status);
    MeshiVec3 (*physx_get_rigid_body_velocity)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h);
    int32_t (*physx_set_collision_shape)(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiCollisionShape* shape);
    size_t (*physx_get_contacts)(struct MeshiEngine* engine, MeshiContactInfo* out_contacts, size_t max);
    MeshiCollisionShape (*physx_collision_shape_sphere)(float radius);
    MeshiCollisionShape (*physx_collision_shape_box)(MeshiVec3 dimensions);
    MeshiCollisionShape (*physx_collision_shape_capsule)(float half_height, float radius);
    int32_t (*pair_render_physics)(
        struct MeshiEngine* engine,
        MeshiRenderObjectHandle render_handle,
        MeshiRigidBodyHandle physics_handle);
    void (*unpair_render_physics)(
        struct MeshiEngine* engine,
        const MeshiRenderObjectHandle* render_handle,
        const MeshiRigidBodyHandle* physics_handle);
} MeshiPluginApi;

// Engine
struct MeshiEngine* meshi_make_engine(const struct MeshiEngineInfo* info);
struct MeshiEngine* meshi_make_engine_headless(const char* application_name, const char* application_location);
void meshi_destroy_engine(struct MeshiEngine* engine);
void meshi_register_event_callback(struct MeshiEngine* engine, void* user_data, MeshiEventCallback cb);
float meshi_update(struct MeshiEngine* engine);
struct MeshiEngine* meshi_get_graphics_system(struct MeshiEngine* engine);
struct MeshiEngine* meshi_get_audio_system(struct MeshiEngine* engine);
const struct MeshiPluginApi* meshi_plugin_get_api(void);

// Audio
MeshiAudioSourceHandle meshi_audio_create_source(struct MeshiEngine* engine, const char* path);
void meshi_audio_destroy_source(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
void meshi_audio_play(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
void meshi_audio_pause(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
void meshi_audio_stop(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
MeshiPlaybackState meshi_audio_get_state(struct MeshiEngine* engine, MeshiAudioSourceHandle h);
void meshi_audio_set_looping(struct MeshiEngine* engine, MeshiAudioSourceHandle h, int32_t looping);
void meshi_audio_set_volume(struct MeshiEngine* engine, MeshiAudioSourceHandle h, float volume);
void meshi_audio_set_pitch(struct MeshiEngine* engine, MeshiAudioSourceHandle h, float pitch);
MeshiAudioSourceHandle meshi_audio_create_stream(struct MeshiEngine* engine, const char* path);
size_t meshi_audio_update_stream(
    struct MeshiEngine* engine,
    MeshiAudioSourceHandle h,
    uint8_t* out_samples,
    size_t max);
void meshi_audio_set_source_transform(
    struct MeshiEngine* engine,
    MeshiAudioSourceHandle h,
    const MeshiMat4* transform,
    MeshiVec3 velocity);
void meshi_audio_set_listener_transform(
    struct MeshiEngine* engine,
    const MeshiMat4* transform,
    MeshiVec3 velocity);
void meshi_audio_set_bus_volume(struct MeshiEngine* engine, MeshiAudioBusHandle h, float volume);
void meshi_audio_register_finished_callback(struct MeshiEngine* engine, void* user_data, MeshiAudioFinishedCallback cb);

// Graphics
MESHI_DEPRECATED
MeshiRenderObjectHandle meshi_gfx_create_mesh_object(struct MeshiEngine* render, const MeshiMeshObjectInfo* info);
MeshiRenderObjectHandle meshi_gfx_create_render_object(struct MeshiEngine* render, const MeshiRenderObjectInfo* info);
void meshi_gfx_release_render_object(struct MeshiEngine* render, const MeshiRenderObjectHandle* h);
void meshi_gfx_set_transform(struct MeshiEngine* render, MeshiRenderObjectHandle h, const MeshiMat4* transform);
MeshiLightHandle meshi_gfx_create_light(struct MeshiEngine* render, const MeshiLightInfo* info);
void meshi_gfx_release_light(struct MeshiEngine* render, const MeshiLightHandle* h);
void meshi_gfx_set_light_transform(struct MeshiEngine* render, MeshiLightHandle h, const MeshiMat4* transform);
void meshi_gfx_set_light_info(struct MeshiEngine* render, MeshiLightHandle h, const MeshiLightInfo* info);
void meshi_gfx_set_camera_transform(struct MeshiEngine* render, const MeshiMat4* transform);
MeshiCameraHandle meshi_gfx_register_camera(struct MeshiEngine* render, const MeshiMat4* initial_transform);
void meshi_gfx_set_camera_projection(struct MeshiEngine* render, const MeshiMat4* transform);
void meshi_gfx_capture_mouse(struct MeshiEngine* render, int32_t value);


// Physics
struct MeshiEngine* meshi_get_physics_system(struct MeshiEngine* engine);
void meshi_physx_set_gravity(struct MeshiEngine* engine, float gravity_mps);
MeshiMaterialHandle meshi_physx_create_material(struct MeshiEngine* engine, const MeshiMaterialInfo* info);
void meshi_physx_release_material(struct MeshiEngine* engine, const MeshiMaterialHandle* h);
MeshiRigidBodyHandle meshi_physx_create_rigid_body(struct MeshiEngine* engine, const MeshiRigidBodyInfo* info);
void meshi_physx_release_rigid_body(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h);
void meshi_physx_apply_force_to_rigid_body(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiForceApplyInfo* info);
int32_t meshi_physx_set_rigid_body_transform(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiActorStatus* info);
int32_t meshi_physx_get_rigid_body_status(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, MeshiActorStatus* out_status);
// Returns the current velocity of a rigid body or a zero vector on failure.
MeshiVec3 meshi_physx_get_rigid_body_velocity(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h);
int32_t meshi_physx_set_collision_shape(struct MeshiEngine* engine, const MeshiRigidBodyHandle* h, const MeshiCollisionShape* shape);
size_t meshi_physx_get_contacts(struct MeshiEngine* engine, MeshiContactInfo* out_contacts, size_t max);
MeshiCollisionShape meshi_physx_collision_shape_sphere(float radius);
MeshiCollisionShape meshi_physx_collision_shape_box(MeshiVec3 dimensions);
MeshiCollisionShape meshi_physx_collision_shape_capsule(float half_height, float radius);
int32_t meshi_pair_render_physics(
    struct MeshiEngine* engine,
    MeshiRenderObjectHandle render_handle,
    MeshiRigidBodyHandle physics_handle);
void meshi_unpair_render_physics(
    struct MeshiEngine* engine,
    const MeshiRenderObjectHandle* render_handle,
    const MeshiRigidBodyHandle* physics_handle);

#ifdef __cplusplus
} // extern "C"
#endif
