#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <iostream>
#include <meshi/bits/components/model_component.hpp>
#include <meshi/bits/objects/denizen.hpp>
#include <meshi/meshi.hpp>

#include "example_helper.hpp"
#include "meshi/bits/components/third_person_camera_component.hpp"
#include "meshi/bits/components/character_controller_component.hpp"

class ModelObject : public meshi::Actor {
public:
  ModelObject() {
    add_subobject<meshi::ModelComponent>(meshi::ModelComponent::CreateInfo{
                                            .model = "model/cube",
                                            .rigid_body_info = {},
                                        })
        ->attach_to(root_component());
  }
};

class MyObject : public meshi::Denizen {
public:
  MyObject() {
    m_target_model =
        add_subobject<meshi::ModelComponent>(meshi::ModelComponent::CreateInfo{
            .model = "model/cube",
            .rigid_body_info = {},
        });
    m_target_model->attach_to(root_component());

    auto target_transform =
        glm::translate(glm::mat4(1.0f), glm::vec3(0.0f, 1.0f, 0.0f));
    root_component()->set_transform(target_transform);

    m_camera = add_subobject<meshi::ThirdPersonCameraComponent>();
    m_camera->attach_target(root_component());

    m_controller = add_subobject<meshi::CharacterControllerComponent>();
    m_controller->set_orientation_source(m_camera);
    m_controller->attach_to(root_component());
  }

  auto update(float dt) -> void override { meshi::Denizen::update(dt); }

  inline auto attach_to_display(meshi::Handle<meshi::gfx::Display> display)
      -> void {
    if (m_camera) {
      m_camera->attach_to_display(display);
    }
  }

private:
  meshi::ModelComponent *m_target_model = nullptr;
  meshi::ThirdPersonCameraComponent *m_camera = nullptr;
  meshi::CharacterControllerComponent *m_controller = nullptr;
};

class Application {
public:
  Application() : m_event(meshi::engine()->event().make_registry(this)) {
    m_event.register_event(
        [](auto &event) { return event.type == meshi::EventType::Quit; },
        [this](auto &) {
          std::cout << "QUITTING" << std::endl;
          m_running = false;
        });

    m_display = meshi::engine()->backend().graphics().register_display(
        meshi::gfx::DisplayInfo{
            .title = "Hello Engine!",
            .width = 1280,
            .height = 720,
            .resizable = false,
            .vsync = true,
        });

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
