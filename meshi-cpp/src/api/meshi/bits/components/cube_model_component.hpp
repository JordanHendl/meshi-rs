#pragma once
#include <meshi/bits/components/model_component.hpp>
#include <meshi/bits/util/slice.hpp>
namespace meshi {
struct CubeModelComponentInfo {
  const char *material = "";
  RigidBodyCreateInfo rigid_body_info{};
};
class CubeModelComponent : public ModelComponent {
public:
  using CreateInfo = CubeModelComponentInfo;
  CubeModelComponent(CubeModelComponentInfo info = {})
      : ModelComponent({.model = "model/cube",
                        .material = info.material,
                        .rigid_body_info = info.rigid_body_info}) {}
};
} // namespace meshi
