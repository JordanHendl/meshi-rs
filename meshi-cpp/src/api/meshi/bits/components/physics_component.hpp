#pragma once
#include "meshi/bits/components/actor_component.hpp"
#include "meshi/engine.hpp"
#include <meshi/bits/util/slice.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <glm/gtx/quaternion.hpp>
namespace meshi {
class PhysicsComponent : public ActorComponent {
public:
  using CreateInfo = RigidBodyCreateInfo;

  PhysicsComponent() {
    auto info = CreateInfo {
      .material = {},
      .has_gravity = false,
    };

    m_sync_from_physics = info.has_gravity != 0;
    m_handle = engine()->backend().physics().create_rigid_body(info);
  }

  PhysicsComponent(CreateInfo info) {
    m_sync_from_physics = info.has_gravity != 0;
    m_handle = engine()->backend().physics().create_rigid_body(info);
  }

  //////////////////////////////////////////////////////
  ////VIRTUAL FUNCITONS/////////////////////////////////
  //////////////////////////////////////////////////////

  virtual ~PhysicsComponent() = default;

  virtual auto update(float dt) -> void {
    ActorComponent::update(dt);

    auto root = this->get_root_component();
    if (root) {
      auto c = root->as_type<ActorComponent>();
      if (c) {
        if (m_sync_from_physics) {
          auto status = engine()->backend().physics().get_rigid_body_status(m_handle);
          auto mat = glm::translate(glm::mat4(1.0f), status.position) *
                     glm::toMat4(status.rotation);
          c->set_transform(mat);
        } else {
          auto world = c->world_transform();
          PhysicsActorStatus status{};
          status.position = glm::vec3(world[3]);
          status.rotation = glm::quat_cast(world);
          engine()->backend().physics().set_rigid_body_status(m_handle, status);
        }
      }
    }
  }

  //////////////////////////////////////////////////////
  ////NON-VIRTUAL FUNCTIONS/////////////////////////////
  //////////////////////////////////////////////////////

protected:
  Handle<RigidBody> m_handle;
  bool m_sync_from_physics = false;
};
} // namespace meshi
