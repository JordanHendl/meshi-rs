#pragma once
#define GLM_ENABLE_EXPERIMENTAL
#include <glm/glm.hpp>
#include <glm/gtc/type_ptr.hpp>
#include <cstdint>
#include <cstring>
#include <meshi.h>
#include "meshi/types.hpp"

namespace meshi {

class GraphicsSystem {
public:
  auto create_renderable(const gfx::RenderableCreateInfo &info)
      -> Handle<gfx::Renderable> {
    MeshiRenderObjectInfo ffi_info{
        info.mesh,
        info.material,
        to_meshi_mat4(info.transform),
    };
    return api_->gfx_create_render_object(m_gfx, &ffi_info);
  }

  auto create_directional_light(const gfx::DirectionalLightInfo &info)
      -> Handle<gfx::DirectionalLight> {
    MeshiLightInfo ffi{};
    ffi.ty = MeshiLightType::Directional;
    ffi.flags = info.flags;
    ffi.intensity = info.intensity;
    ffi.range = info.range;
    ffi.color_r = info.color.x;
    ffi.color_g = info.color.y;
    ffi.color_b = info.color.z;
    ffi.dir_x = info.direction.x;
    ffi.dir_y = info.direction.y;
    ffi.dir_z = info.direction.z;
    return api_->gfx_create_light(m_gfx, &ffi);
  }

  void set_transform(Handle<gfx::Renderable> &renderable,
                     const glm::mat4 &transform) {
    MeshiMat4 t = to_meshi_mat4(transform);
    api_->gfx_set_transform(m_gfx, renderable, &t);
  }

  auto register_display(const gfx::DisplayInfo &info) -> Handle<gfx::Display> {
    MeshiDisplayInfo ffi_info{};
    ffi_info.vsync = static_cast<int32_t>(info.vsync);
    ffi_info.window.title = info.title;
    ffi_info.window.width = info.width;
    ffi_info.window.height = info.height;
    ffi_info.window.resizable = static_cast<int32_t>(info.resizable);
    return api_->gfx_register_display(m_gfx, &ffi_info);
  }

  void attach_camera_to_display(Handle<gfx::Display> &display,
                                Handle<gfx::Camera> &camera) {
    api_->gfx_attach_camera_to_display(m_gfx, display, camera);
  }

  auto register_camera(const glm::mat4 &initial_transform)
      -> Handle<gfx::Camera> {
    MeshiMat4 t = to_meshi_mat4(initial_transform);
    return api_->gfx_register_camera(m_gfx, &t);
  }

  void set_camera_transform(Handle<gfx::Camera> &camera,
                            const glm::mat4 &transform) {
    MeshiMat4 t = to_meshi_mat4(transform);
    api_->gfx_set_camera_transform(m_gfx, camera, &t);
  }

  void set_camera_projection(Handle<gfx::Camera> &camera,
                             const glm::mat4 &projection_matrix) {
    MeshiMat4 t = to_meshi_mat4(projection_matrix);
    api_->gfx_set_camera_projection(m_gfx, camera, &t);
  }

  void set_skybox_settings(const gfx::SkyboxSettingsInfo &info) {
    MeshiSkyboxSettingsInfo ffi{};
    ffi.intensity = info.intensity;
    ffi.use_procedural_cubemap =
        static_cast<std::int32_t>(info.use_procedural_cubemap);
    ffi.update_interval_frames = info.update_interval_frames;
    api_->gfx_set_skybox_settings(m_gfx, &ffi);
  }

  void set_environment_lighting(const gfx::EnvironmentLightingInfo &info) {
    MeshiEnvironmentLightingInfo ffi{};
    ffi.sky.enabled = static_cast<std::int32_t>(info.sky.enabled);
    ffi.sky.has_sun_direction =
        static_cast<std::int32_t>(info.sky.has_sun_direction);
    ffi.sky.sun_direction.x = info.sky.sun_direction.x;
    ffi.sky.sun_direction.y = info.sky.sun_direction.y;
    ffi.sky.sun_direction.z = info.sky.sun_direction.z;
    ffi.sun_light_intensity = info.sun_light_intensity;
    ffi.moon_light_intensity = info.moon_light_intensity;
    api_->gfx_set_environment_lighting(m_gfx, &ffi);
  }

  void set_ocean_settings(const gfx::OceanSettingsInfo &info) {
    MeshiOceanSettingsInfo ffi{};
    ffi.enabled = static_cast<std::int32_t>(info.enabled);
    ffi.wind_speed = info.wind_speed;
    ffi.wave_amplitude = info.wave_amplitude;
    ffi.gerstner_amplitude = info.gerstner_amplitude;
    api_->gfx_set_ocean_settings(m_gfx, &ffi);
  }

  void set_cloud_settings(const gfx::CloudSettingsInfo &info) {
    MeshiCloudSettingsInfo ffi{};
    ffi.enabled = static_cast<std::int32_t>(info.enabled);
    api_->gfx_set_cloud_settings(m_gfx, &ffi);
  }

  void set_terrain_settings(const gfx::TerrainSettingsInfo &info) {
    MeshiTerrainSettingsInfo ffi{};
    ffi.enabled = static_cast<std::int32_t>(info.enabled);
    ffi.clipmap_resolution = info.clipmap_resolution;
    ffi.max_tiles = info.max_tiles;
    ffi.lod_levels = info.lod_levels;
    api_->gfx_set_terrain_settings(m_gfx, &ffi);
  }

  void set_terrain_project_key(const char *project_key) {
    if (!project_key) {
      return;
    }
    api_->gfx_set_terrain_project_key(m_gfx, project_key);
  }

  inline auto capture_mouse(bool value) -> void {
    api_->gfx_capture_mouse(m_gfx, static_cast<int>(value));
  }

private:
  friend class EngineBackend;
  GraphicsSystem() = default;
  explicit GraphicsSystem(const MeshiPluginApi *api, RawGraphicsSystem *ptr)
      : api_(api), m_gfx(ptr) {}
  ~GraphicsSystem() = default;

  static MeshiMat4 to_meshi_mat4(const glm::mat4 &m) {
    MeshiMat4 out{};
    std::memcpy(out.m, glm::value_ptr(m), sizeof(MeshiMat4));
    return out;
  }

  const MeshiPluginApi *api_{};
  RawGraphicsSystem *m_gfx{};
};

} // namespace meshi
