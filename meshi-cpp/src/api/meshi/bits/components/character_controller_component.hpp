#pragma once

#define GLM_FORCE_LEFT_HANDED
#define GLM_FORCE_DEPTH_ZERO_TO_ONE
#include <glm/glm.hpp>
#include <meshi/bits/components/actor_component.hpp>
#include <meshi/engine.hpp>
#include <optional>

namespace meshi {

class CharacterControllerComponent : public ActorComponent {
public:
  static constexpr glm::vec3 WORLD_UP = glm::vec3(0.0f, 1.0f, 0.0f);

  struct KeyboardBindings {
    KeyCode forward = KeyCode::W;
    KeyCode backward = KeyCode::S;
    KeyCode left = KeyCode::A;
    KeyCode right = KeyCode::D;
  };

  struct Config {
    float movement_speed = 12.0f;
    float movement_input_sensitivity = 1.0f;
    bool use_character_controller = true;
    float controller_radius = 0.4f;
    float controller_half_height = 0.9f;
    KeyboardBindings bindings{};
  };

  CharacterControllerComponent() : CharacterControllerComponent(Config{}) {}
  explicit CharacterControllerComponent(const Config &config) : m_config(config) {}

  virtual ~CharacterControllerComponent() {
    release_controller();
  }

  inline auto set_config(const Config &config) -> void {
    m_config = config;
  }

  inline auto set_orientation_source(ActorComponent *source) -> void {
    m_orientation_source = source;
  }

  virtual auto update(float dt) -> void override {
    ActorComponent::update(dt);

    auto *target = m_parent ? m_parent->as_type<ActorComponent>() : nullptr;
    if (!target) {
      return;
    }

    ensure_controller(target);

    glm::vec2 movement(0.0f);
    auto &events = engine()->event();
    if (events.is_pressed(m_config.bindings.forward)) {
      movement.y += 1.0f;
    }
    if (events.is_pressed(m_config.bindings.backward)) {
      movement.y -= 1.0f;
    }
    if (events.is_pressed(m_config.bindings.right)) {
      movement.x += 1.0f;
    }
    if (events.is_pressed(m_config.bindings.left)) {
      movement.x -= 1.0f;
    }

    if (movement == glm::vec2(0.0f)) {
      return;
    }

    movement *= m_config.movement_input_sensitivity;
    if (glm::length(movement) > 1.0f) {
      movement = glm::normalize(movement);
    }

    auto *orientation = m_orientation_source ? m_orientation_source : target;

    glm::vec3 forward = orientation->front();
    forward.y = 0.0f;
    if (glm::length(forward) <= 0.0001f) {
      forward = glm::vec3(0.0f, 0.0f, -1.0f);
    } else {
      forward = glm::normalize(forward);
    }

    glm::vec3 right = glm::cross(WORLD_UP, forward);
    if (glm::length(right) <= 0.0001f) {
      right = glm::vec3(1.0f, 0.0f, 0.0f);
    } else {
      right = glm::normalize(right);
    }

    const glm::vec3 desired_translation =
        (forward * movement.y + right * movement.x) * (m_config.movement_speed * dt);

    if (m_controller.has_value()) {
      CharacterControllerMoveResult result{};
      auto controller_handle = *m_controller;
      if (engine()->backend().physics().move_character_controller(
              controller_handle, desired_translation, result)) {
        if (auto status = engine()->backend().physics().get_character_controller_status(
                controller_handle)) {
          auto target_local = target->local_transform();
          target_local[3] = glm::vec4(status->position, 1.0f);
          target->set_transform(target_local);
          return;
        }
      }
    }

    auto target_local = target->local_transform();
    target_local[3] += glm::vec4(desired_translation, 0.0f);
    target->set_transform(target_local);
  }

private:
  inline auto release_controller() -> void {
    if (m_controller.has_value()) {
      auto controller = *m_controller;
      engine()->backend().physics().release_character_controller(controller);
      m_controller = {};
    }
  }

  inline auto ensure_controller(ActorComponent *target) -> void {
    if (!m_config.use_character_controller) {
      release_controller();
      return;
    }

    if (m_controller.has_value()) {
      return;
    }

    CharacterControllerCreateInfo controller_info{};
    controller_info.initial_position = glm::vec3(target->world_transform()[3]);
    controller_info.radius = m_config.controller_radius;
    controller_info.half_height = m_config.controller_half_height;
    m_controller =
        engine()->backend().physics().create_character_controller(controller_info);
  }

  Config m_config{};
  ActorComponent *m_orientation_source = nullptr;
  std::optional<Handle<CharacterController>> m_controller{};
};

} // namespace meshi
