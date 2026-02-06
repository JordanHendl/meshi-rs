#pragma once
#include "meshi/bits/components/actor_component.hpp"
#include "meshi/bits/components/physics_component.hpp"
#include "meshi/engine.hpp"
#include <meshi/bits/util/slice.hpp>
namespace meshi {
class RenderableComponent : public PhysicsComponent {
public:
  struct CreateInfo {
    gfx::RenderableCreateInfo render_info;
    RigidBodyCreateInfo rigid_body_info{};
  };

  RenderableComponent(CreateInfo info) : PhysicsComponent(info.rigid_body_info) {
    m_handle = engine()->backend().graphics().create_renderable(info.render_info);
  }

  virtual auto update(float dt) -> void override {
    PhysicsComponent::update(dt);
    auto transform = this->world_transform();
    engine()->backend().graphics().set_transform(m_handle, transform);
  }

  virtual ~RenderableComponent() = default;

protected:
  Handle<gfx::Renderable> m_handle;
};
} // namespace meshi
