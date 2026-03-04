#pragma once

#include <meshi_types.h>
#include <glm/glm.hpp>
#include <cstdint>

namespace meshi {

using EngineBackendInfo = MeshiEngineInfo;
using RawEngineBackend = MeshiEngine;
using RawGraphicsSystem = MeshiEngine;
using RawPhysicsSystem = MeshiEngine;

template <typename T> using Handle = MeshiHandle;

namespace gfx {
using Renderable = MeshiRenderObjectHandle;
struct RenderableCreateInfo {
  const char *mesh = "";
  const char *material = "";
  glm::mat4 transform = glm::mat4(1.0f);
};
using Display = MeshiDisplayHandle;
struct DisplayInfo {
  const char *title = "";
  std::uint32_t width = 1280;
  std::uint32_t height = 720;
  bool resizable = false;
  bool vsync = false;
};

struct SkyboxSettingsInfo {
  float intensity = 1.0f;
  bool use_procedural_cubemap = true;
  std::uint32_t update_interval_frames = 1;
};

struct SkySettingsInfo {
  bool enabled = true;
  bool has_sun_direction = false;
  glm::vec3 sun_direction{0.0f, -1.0f, 0.0f};
};

struct EnvironmentLightingInfo {
  SkySettingsInfo sky{};
  float sun_light_intensity = 1.0f;
  float moon_light_intensity = 0.1f;
};

struct OceanSettingsInfo {
  bool enabled = true;
  float wind_speed = 2.0f;
  float wave_amplitude = 4.0f;
  float gerstner_amplitude = 0.35f;
};

struct CloudSettingsInfo {
  bool enabled = true;
};

struct TerrainSettingsInfo {
  bool enabled = true;
  std::uint32_t clipmap_resolution = 18;
  std::uint32_t max_tiles = 12 * 12;
  std::uint32_t lod_levels = 6;
};

using Camera = MeshiCameraHandle;
using DirectionalLight = MeshiLightHandle;
struct DirectionalLightInfo {
  glm::vec4 direction{0.0f};
  glm::vec4 color{1.0f};
  float intensity = 1.0f;
  float range = 0.0f;
  std::uint32_t flags = static_cast<std::uint32_t>(MeshiLightFlags::None);
};
} // namespace gfx

using PhysicsMaterial = MeshiMaterial;
using PhysicsMaterialCreateInfo = MeshiMaterialInfo;
using RigidBody = MeshiRigidBody;
using CharacterController = MeshiCharacterControllerHandle;
struct RigidBodyCreateInfo {
  Handle<PhysicsMaterial> material{};
  glm::vec3 initial_position{0.0f};
  glm::vec3 initial_velocity{0.0f};
  glm::quat initial_rotation{1.0f, 0.0f, 0.0f, 0.0f};
  std::uint32_t has_gravity{0};
  MeshiCollisionShape collision_shape{};
};
using ForceApplyInfo = MeshiForceApplyInfo;
struct PhysicsActorStatus {
  glm::vec3 position{0.0f};
  glm::quat rotation{1.0f, 0.0f, 0.0f, 0.0f};
};

struct CharacterControllerCreateInfo {
  glm::vec3 initial_position{0.0f};
  float radius{0.4f};
  float half_height{0.9f};
  float step_height{0.35f};
  float slope_limit_deg{50.0f};
};

struct CharacterControllerMoveResult {
  glm::vec3 applied_motion{0.0f};
  std::uint32_t grounded{0};
};

} // namespace meshi
