#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <glm/gtc/type_ptr.hpp>
#include <iostream>
#include <meshi/bits/components/model_component.hpp>
#include <meshi/bits/components/editor_camera_component.hpp>
#include <meshi/bits/objects/denizen.hpp>
#include <meshi/meshi.hpp>

#include "example_helper.hpp"
#include "meshi/bits/components/directional_light_component.hpp"

class ModelObject : public meshi::Actor {
public:
  ModelObject() {
    add_subobject<meshi::ModelComponent>(meshi::ModelComponent::CreateInfo{
                                            .model = "model/witch",
                                            .rigid_body_info = {},
                                        })
        ->attach_to(root_component());
  }
};

class MyObject : public meshi::Denizen {
public:
  MyObject()
      : m_event(std::make_shared<meshi::ActionRegister<MyObject>>(
            meshi::engine()->action().make_registry(this))) {
    // Subobjects make up things. Nice for grouping objects together.
    // Note: Transforms do not propagate for subobjects.
    m_camera = add_subobject<meshi::EditorCameraComponent>();
    // Attach components to our root.
    m_camera->attach_to(root_component());

    auto initial_transform =
        glm::translate(glm::mat4(1.0), glm::vec3(0.0, 5.0, 30.0));
    initial_transform =
        glm::rotate(initial_transform, (float)glm::radians(0.0), this->up());
    m_camera->set_transform(initial_transform);
    
    m_light = add_subobject<meshi::DirectionalLightComponent>(
        meshi::DirectionalLightComponent::CreateInfo{
            .direction = glm::vec4(-0.4, -0.7, -0.4, 1.0),
            .color = glm::vec4(0.8, 0.7, 0.8, 1.0),
            .intensity = 0.5,
        });
  }

  auto update(float dt) -> void override { meshi::Denizen::update(dt); }
  inline auto attach_to_display(meshi::Handle<meshi::gfx::Display> display)
      -> void {
    if (m_camera) {
      m_camera->attach_to_display(display);
    }
  }

private:
  std::shared_ptr<meshi::ActionRegister<MyObject>> m_event;
  meshi::CameraComponent *m_camera = nullptr;
  meshi::DirectionalLightComponent *m_light = nullptr;
};

////////////////////////////////////////////////////////////

class Application {
public:
  Application() : m_event(meshi::engine()->event().make_registry(this)) {
    // Register quit event
    m_event.register_event(
        [](auto &event) { return event.type == meshi::EventType::Quit; },
        [this](auto &event) {
          std::cout << "QUITTING" << std::endl;
          m_running = false;
        });

    m_display = meshi::engine()->backend().graphics().register_display(
        meshi::gfx::DisplayInfo{
            .title = "Hello Lighting!",
            .width = 1280,
            .height = 720,
            .resizable = false,
            .vsync = true,
        });
    // Spawn our object, and activate it.
    meshi::engine()->world().spawn_object<ModelObject>()->activate();
    auto camera_actor = meshi::engine()->world().spawn_object<MyObject>();
    camera_actor->attach_to_display(m_display);
    camera_actor->activate();
  }

  auto run() -> void {
    while (m_running) {
      meshi::engine()->update();
    }
  }

private:
  bool m_running = true;
  meshi::Handle<meshi::gfx::Display> m_display{};
  meshi::EventRegister<Application> m_event;
};

auto main() -> int {
  meshi::initialize_meshi_engine(meshi::EngineInfo{
      .application_name = "Hello Engine!",
      .application_root = EXAMPLE_APP_DIR,
  });

  auto app = Application();
  app.run();

  return 0;
}
