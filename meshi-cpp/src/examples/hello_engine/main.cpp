#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <iostream>
#include <meshi/bits/components/character_controller_component.hpp>
#include <meshi/bits/components/model_component.hpp>
#include <meshi/bits/components/physics_component.hpp>
#include <meshi/bits/components/third_person_camera_component.hpp>
#include <meshi/bits/objects/denizen.hpp>
#include <meshi/meshi.hpp>

#include "example_helper.hpp"

class TerrainColliderObject : public meshi::Actor {
public:
  TerrainColliderObject() {
    auto root_transform =
        glm::translate(glm::mat4(1.0f), glm::vec3(0.0f, -4.0f, 0.0f));
    root_component()->set_transform(root_transform);

    meshi::RigidBodyCreateInfo body_info{};
    body_info.has_gravity = 0;
    body_info.collision_shape =
        meshi::PhysicsSystem::collision_shape_box(glm::vec3(40000.0f, 8.0f, 40000.0f));

    add_subobject<meshi::PhysicsComponent>(body_info)->attach_to(root_component());
  }
};

class PlayerObject : public meshi::Denizen {
public:
  PlayerObject() {
    meshi::RigidBodyCreateInfo visual_body_info{};
    visual_body_info.has_gravity = 0;
    // Keep the visual model out of controller collision queries.
    visual_body_info.collision_shape =
        meshi::PhysicsSystem::collision_shape_sphere(0.0f);

    m_target_model =
        add_subobject<meshi::ModelComponent>(meshi::ModelComponent::CreateInfo{
            .model = "model/cube",
            .rigid_body_info = visual_body_info,
        });
    m_target_model->attach_to(root_component());

    auto spawn_transform =
        glm::translate(glm::mat4(1.0f), glm::vec3(0.0f, 16.0f, 0.0f));
    root_component()->set_transform(spawn_transform);

    m_camera = add_subobject<meshi::ThirdPersonCameraComponent>();
    auto camera_config = m_camera->config();
    camera_config.follow_distance = 10.0f;
    camera_config.min_follow_distance = 3.0f;
    camera_config.max_follow_distance = 24.0f;
    camera_config.focus_offset = glm::vec3(0.0f, 1.1f, 0.0f);
    camera_config.look_sensitivity = 0.08f;
    m_camera->set_config(camera_config);
    m_camera->attach_target(root_component());

    meshi::CharacterControllerComponent::Config controller_config{};
    controller_config.movement_speed = 16.0f;
    controller_config.controller_radius = 0.5f;
    controller_config.controller_half_height = 1.0f;
    controller_config.enable_gravity = true;
    controller_config.enable_jump = true;
    controller_config.jump_velocity = 10.0f;
    m_controller =
        add_subobject<meshi::CharacterControllerComponent>(controller_config);
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

    initialize_environment();
    meshi::engine()->backend().physics().set_gravity(-9.8f);

    meshi::engine()->world().spawn_object<TerrainColliderObject>()->activate();

    auto player = meshi::engine()->world().spawn_object<PlayerObject>();
    player->attach_to_display(m_display);
    player->activate();
  }

  auto run() -> void {
    while (m_running) {
      meshi::engine()->update();
    }
  }

private:
  auto initialize_environment() -> void {
    auto &graphics = meshi::engine()->backend().graphics();

    graphics.set_skybox_settings(meshi::gfx::SkyboxSettingsInfo{
        .intensity = 1.0f,
        .use_procedural_cubemap = true,
        .update_interval_frames = 2,
    });

    graphics.set_environment_lighting(meshi::gfx::EnvironmentLightingInfo{
        .sky =
            meshi::gfx::SkySettingsInfo{
                .enabled = true,
                .has_sun_direction = true,
                .sun_direction = glm::normalize(glm::vec3(-0.2f, -1.0f, -0.3f)),
            },
        .sun_light_intensity = 3.0f,
        .moon_light_intensity = 0.35f,
    });

    graphics.set_cloud_settings(meshi::gfx::CloudSettingsInfo{
        .enabled = true,
    });

    graphics.set_ocean_settings(meshi::gfx::OceanSettingsInfo{
        .enabled = true,
        .wind_speed = 2.0f,
        .wave_amplitude = 4.0f,
        .gerstner_amplitude = 0.35f,
    });

    graphics.set_terrain_settings(meshi::gfx::TerrainSettingsInfo{
        .enabled = true,
        .clipmap_resolution = 18,
        .max_tiles = 12 * 12,
        .lod_levels = 6,
    });
    graphics.set_terrain_project_key("iceland");
  }

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
