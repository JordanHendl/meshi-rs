#pragma once

#include "meshi/bits/action.hpp"
#include <cmath>
#include <functional>
#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <meshi/bits/components/actor_component.hpp>
#include <meshi/bits/components/camera_component.hpp>
#include <meshi/engine.hpp>
#include <utility>

namespace meshi {

class ThirdPersonCameraComponent : public CameraComponent {
public:
  static constexpr glm::vec3 WORLD_UP = glm::vec3(0.0f, -1.0f, 0.0f);

  struct InputFrame {
    glm::vec2 look_delta = glm::vec2(0.0f);
    glm::vec2 move = glm::vec2(0.0f);
    bool orbit_active = false;
    bool capture_cursor = false;
  };

  struct KeyboardMouseBindings {
    KeyCode forward = KeyCode::W;
    KeyCode backward = KeyCode::S;
    KeyCode left = KeyCode::A;
    KeyCode right = KeyCode::D;
    MouseButton orbit_button = MouseButton::Right;
    bool orbit_requires_button = false;
  };

  struct Config {
    float follow_distance = 8.0f;
    float min_follow_distance = 2.0f;
    float max_follow_distance = 40.0f;
    float rotation_speed = 250.0f;
    float look_sensitivity = 0.001f;
    float movement_speed = 12.0f;
    float movement_input_sensitivity = 1.0f;
    float min_pitch_deg = -75.0f;
    float max_pitch_deg = 75.0f;
    bool constrain_yaw = false;
    float min_yaw_deg = -180.0f;
    float max_yaw_deg = 180.0f;
    glm::vec3 focus_offset = glm::vec3(0.0f, -1.6f, 0.0f);
    KeyboardMouseBindings bindings{};
  };

  ThirdPersonCameraComponent() : ThirdPersonCameraComponent(Config{}) {}

  explicit ThirdPersonCameraComponent(const Config &config)
      : CameraComponent(), m_config(config), m_distance(config.follow_distance) {
    m_event = std::make_shared<ActionRegister<ThirdPersonCameraComponent>>(
        engine()->action().make_registry(this));

    engine()->action().register_action(
        "Third-Person-Camera-Mouse",
        [](const Event &event, Action &action) {
          if (event.source == EventSource::Mouse &&
              (event.type == EventType::Motion2D ||
               event.type == EventType::CursorMoved)) {
            action.type = "camera-look";
            return true;
          }
          return false;
        });

    m_event->register_action("Third-Person-Camera-Mouse",
                             &ThirdPersonCameraComponent::handle_mouse_motion);

    m_input_provider = [this]() { return keyboard_mouse_input(); };
    sync_angles_from_transform();
  }

  virtual ~ThirdPersonCameraComponent() = default;

  inline auto attach_target(ActorComponent *target_component) -> void {
    m_target = target_component;
    sync_angles_from_transform();
  }

  inline auto target() const -> ActorComponent * { return m_target; }

  inline auto set_input_provider(std::function<InputFrame()> provider) -> void {
    if (provider) {
      m_input_provider = std::move(provider);
    }
  }

  inline auto set_config(const Config &config) -> void {
    m_config = config;
    m_distance = glm::clamp(m_distance, m_config.min_follow_distance,
                            m_config.max_follow_distance);
    m_pitch_deg = glm::clamp(m_pitch_deg, m_config.min_pitch_deg, m_config.max_pitch_deg);
    if (m_config.constrain_yaw) {
      m_yaw_deg = glm::clamp(m_yaw_deg, m_config.min_yaw_deg, m_config.max_yaw_deg);
    }
  }

  inline auto config() const -> const Config & { return m_config; }

  virtual auto update(float dt) -> void override {
    CameraComponent::update(dt);

    if (!m_target) {
      return;
    }

    auto input = m_input_provider ? m_input_provider() : InputFrame{};
    engine()->backend().graphics().capture_mouse(input.capture_cursor);

    apply_orbit(input.look_delta, input.orbit_active, dt);
    apply_target_movement(input.move, dt);
    update_camera_transform();
  }

private:
  inline auto handle_mouse_motion(const Action &event) -> void {
    m_mouse_look_delta += event.event.payload.motion2d.motion;
  }

  inline auto keyboard_mouse_input() -> InputFrame {
    InputFrame input{};
    auto &events = engine()->event();

    if (events.is_pressed(m_config.bindings.forward)) {
      input.move.y += 1.0f;
    }
    if (events.is_pressed(m_config.bindings.backward)) {
      input.move.y -= 1.0f;
    }
    if (events.is_pressed(m_config.bindings.right)) {
      input.move.x += 1.0f;
    }
    if (events.is_pressed(m_config.bindings.left)) {
      input.move.x -= 1.0f;
    }

    const bool orbit_button_pressed =
        events.is_pressed(m_config.bindings.orbit_button);
    input.orbit_active = m_config.bindings.orbit_requires_button
                            ? orbit_button_pressed
                            : true;
    input.capture_cursor =
        m_config.bindings.orbit_requires_button && orbit_button_pressed;
    input.look_delta = m_mouse_look_delta;
    m_mouse_look_delta = glm::vec2(0.0f);

    return input;
  }

  inline auto apply_orbit(const glm::vec2 &look_delta, bool orbit_active,
                          float dt) -> void {
    if (!orbit_active) {
      return;
    }

    const auto rotation_scale =
        m_config.rotation_speed * m_config.look_sensitivity * dt;
    m_yaw_deg += look_delta.x * rotation_scale;
    if (m_config.constrain_yaw) {
      m_yaw_deg = glm::clamp(m_yaw_deg, m_config.min_yaw_deg, m_config.max_yaw_deg);
    }
    m_pitch_deg = glm::clamp(m_pitch_deg + look_delta.y * rotation_scale,
                             m_config.min_pitch_deg, m_config.max_pitch_deg);
  }

  inline auto apply_target_movement(glm::vec2 movement, float dt) -> void {
    if (movement == glm::vec2(0.0f) || !m_target) {
      return;
    }

    movement *= m_config.movement_input_sensitivity;
    if (glm::length(movement) > 1.0f) {
      movement = glm::normalize(movement);
    }

    const auto yaw_rad = glm::radians(m_yaw_deg);
    const glm::vec3 planar_forward =
        glm::normalize(glm::vec3(std::sin(yaw_rad), 0.0f, std::cos(yaw_rad)));
    const glm::vec3 planar_right =
        glm::normalize(glm::cross(planar_forward, WORLD_UP));

    const glm::vec3 translation =
        (planar_forward * movement.y + planar_right * movement.x) *
        (m_config.movement_speed * dt);

    auto target_local = m_target->local_transform();
    target_local = glm::translate(target_local, translation);
    m_target->set_transform(target_local);
  }

  inline auto update_camera_transform() -> void {
    if (!m_target) {
      return;
    }

    const auto focus = glm::vec3(m_target->world_transform()[3]) +
                       m_config.focus_offset;
    const auto yaw_rad = glm::radians(m_yaw_deg);
    const auto pitch_rad = glm::radians(m_pitch_deg);

    const glm::vec3 forward = glm::normalize(glm::vec3(
        std::cos(pitch_rad) * std::sin(yaw_rad), -std::sin(pitch_rad),
        std::cos(pitch_rad) * std::cos(yaw_rad)));

    const glm::vec3 camera_position = focus - forward * m_distance;
    const auto view = glm::lookAt(camera_position, focus, WORLD_UP);
    auto transform = glm::inverse(view);
    set_transform(transform);
  }

  inline auto sync_angles_from_transform() -> void {
    m_distance = glm::clamp(m_config.follow_distance, m_config.min_follow_distance,
                            m_config.max_follow_distance);
    m_pitch_deg = glm::clamp(m_pitch_deg, m_config.min_pitch_deg, m_config.max_pitch_deg);
    if (m_config.constrain_yaw) {
      m_yaw_deg = glm::clamp(m_yaw_deg, m_config.min_yaw_deg, m_config.max_yaw_deg);
    }
  }

  Config m_config{};
  float m_distance = 8.0f;
  float m_yaw_deg = 180.0f;
  float m_pitch_deg = 15.0f;

  ActorComponent *m_target = nullptr;
  glm::vec2 m_mouse_look_delta = glm::vec2(0.0f);

  std::function<InputFrame()> m_input_provider;
  std::shared_ptr<ActionRegister<ThirdPersonCameraComponent>> m_event;
};

} // namespace meshi
