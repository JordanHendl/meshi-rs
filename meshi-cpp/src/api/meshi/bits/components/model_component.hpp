#pragma once
#include "meshi/bits/components/renderable_component.hpp"
#include "meshi/engine.hpp"
#include <meshi/bits/util/slice.hpp>
namespace meshi {
struct ModelComponentCreateInfo {
  const char *model = "";
  const char *material = "";
  glm::mat4 transform = glm::mat4(1.0f);
  RigidBodyCreateInfo rigid_body_info{};
};
class ModelComponent : public RenderableComponent {
public:
  using CreateInfo = ModelComponentCreateInfo;
  ModelComponent(CreateInfo info)
      : RenderableComponent({gfx::RenderableCreateInfo{
                                 .mesh = info.model,
                                 .material = info.material,
                                 .transform = info.transform,
                             },
                             info.rigid_body_info}) {}
};
} // namespace meshi
