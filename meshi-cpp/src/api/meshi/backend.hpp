#pragma once

#include <cstdint>
#include <functional>
#include <glm/glm.hpp>
#include <memory>
#include <meshi.h>
#include "meshi/types.hpp"
#include <meshi/graphics.hpp>
#include <meshi/physics.hpp>
#include "meshi/bits/util/loader.hpp"
#include <cassert>
namespace meshi {

class EngineBackend {
public:
  explicit EngineBackend(const EngineBackendInfo &info)
      : api_(resolve_api()) {
    assert(api_ && "Meshi plugin API is required.");
    engine_ = api_->make_engine(&info);

    auto raw_phys = api_->get_physics_system(engine_);
    auto raw_gfx = api_->get_graphics_system(engine_);
    m_phys = PhysicsSystem(api_, raw_phys);
    m_gfx = GraphicsSystem(api_, raw_gfx);
  }

  ~EngineBackend() {
    if (api_ && engine_) {
      api_->destroy_engine(engine_);
    }
  }

  void register_event_callback(void *user_data,
                               void (*callback)(MeshiEvent *, void *)) {
    api_->register_event_callback(engine_, user_data, callback);
  }

  float update() { return api_->update(engine_); }

  inline auto physics() -> PhysicsSystem & { return m_phys; }

  inline auto graphics() -> GraphicsSystem & { return m_gfx; }

private:
  static const MeshiPluginApi *resolve_api() {
    static void* dl =meshi::detail::loader_function("libmeshi-rs.so");
    auto loader_fn = [](const char* name) {
      return meshi::detail::get_plugin_symbol(dl, name);
    };

    return MESHI_PLUGIN_LOAD_API(loader_fn);
  }

  PhysicsSystem m_phys;
  GraphicsSystem m_gfx;
  RawEngineBackend *engine_{};
  const MeshiPluginApi *api_{};
};

} // namespace meshi
