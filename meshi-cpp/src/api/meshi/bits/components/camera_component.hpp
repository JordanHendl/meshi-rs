#pragma once
#define GLM_FORCE_LEFT_HANDED
#define GLM_FORCE_DEPTH_ZERO_TO_ONE
#include "glm/ext/matrix_clip_space.hpp"
#include <glm/gtc/matrix_transform.hpp>
#include <glm/gtc/matrix_inverse.hpp>
#include <meshi/engine.hpp>
#include <meshi/bits/components/actor_component.hpp>
namespace meshi { 
class CameraComponent : public ActorComponent {
public:
  CameraComponent() {
    constexpr auto fov = 20.0;
    constexpr auto aspect = 16.0/9.0;
    constexpr auto near = 0.1;
    constexpr auto far = 200000.0;
    m_projection = glm::perspective(glm::radians(fov), aspect, near, far);
    m_camera = engine()->backend().graphics().register_camera(glm::mat4(1.0f));
  };
  virtual ~CameraComponent() = default;
  inline auto view_matrix() -> glm::mat4 {
    return (glm::inverse(this->world_transform()));
  }

  virtual auto update(float dt) -> void override {
    auto transform = this->world_transform();
    engine()->backend().graphics().set_camera_transform(m_camera, transform);
    engine()->backend().graphics().set_camera_projection(m_camera, m_projection);
  }
  inline auto projection() -> glm::mat4 { return this->m_projection; }

  inline auto attach_to_display(Handle<gfx::Display> display) -> void {
    engine()->backend().graphics().attach_camera_to_display(display, m_camera);
  }

private:
  glm::mat4 m_projection;
  Handle<gfx::Camera> m_camera;
};
} // namespace meshi
