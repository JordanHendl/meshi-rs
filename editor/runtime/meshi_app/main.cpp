#include <meshi/meshi.hpp>

int main(int argc, char** argv) {
  // TODO: Update these paths for your project layout.
  const char* app_name = "Meshi App";
  const char* app_dir = ".";

  meshi::EngineInfo info{};
  info.application_name = app_name;
  info.application_location = app_dir;
  info.headless = false;
  info.debug_mode = true;

  auto engine = meshi::Engine::create(info);
  if (!engine) {
    return 1;
  }

  while (engine->running()) {
    engine->update();
    // TODO: Submit game/app logic here.
  }

  return 0;
}
