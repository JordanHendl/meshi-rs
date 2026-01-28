#pragma once
#include <glm/glm.hpp>
#include <glm/gtc/quaternion.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <meshi.h>
#include "meshi/types.hpp"

namespace meshi {

class PhysicsSystem {
public:
  auto create_material(PhysicsMaterialCreateInfo &info)
      -> Handle<PhysicsMaterial> {
    return api_->physx_create_material(m_phys, &info);
  }

  void release_material(Handle<PhysicsMaterial> &material) {
    api_->physx_release_material(m_phys, &material);
  }

  auto create_rigid_body(RigidBodyCreateInfo &info) -> Handle<RigidBody> {
    MeshiRigidBodyInfo ffi{};
    ffi.material = info.material;
    ffi.initial_position = {info.initial_position.x, info.initial_position.y,
                           info.initial_position.z};
    ffi.initial_velocity = {info.initial_velocity.x, info.initial_velocity.y,
                           info.initial_velocity.z};
    ffi.initial_rotation = {info.initial_rotation.x, info.initial_rotation.y,
                           info.initial_rotation.z, info.initial_rotation.w};
    ffi.has_gravity = info.has_gravity;
    ffi.collision_shape = info.collision_shape;
    return api_->physx_create_rigid_body(m_phys, &ffi);
  }

  void release_rigid_body(Handle<RigidBody> &rigidBody) {
    api_->physx_release_rigid_body(m_phys, &rigidBody);
  }

  void apply_force_to_rigid_body(Handle<RigidBody> &rigidBody,
                                 ForceApplyInfo &info) {
    api_->physx_apply_force_to_rigid_body(m_phys, &rigidBody, &info);
  }

  auto get_rigid_body_status(Handle<RigidBody> &rigidBody)
      -> PhysicsActorStatus {
    MeshiActorStatus ffi{};
    api_->physx_get_rigid_body_status(m_phys, &rigidBody, &ffi);
    PhysicsActorStatus status{};
    status.position = {ffi.position.x, ffi.position.y, ffi.position.z};
    status.rotation = {ffi.rotation.w, ffi.rotation.x, ffi.rotation.y,
                       ffi.rotation.z};
    return status;
  }

private:
  friend class EngineBackend;
  PhysicsSystem() = default;
  explicit PhysicsSystem(const MeshiPluginApi *api, RawPhysicsSystem *ptr)
      : api_(api), m_phys(ptr) {}
  ~PhysicsSystem() = default;

  const MeshiPluginApi *api_{};
  RawPhysicsSystem *m_phys{};
};

} // namespace meshi
